use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentSymbol, Position, Range, SymbolKind,
};
use tree_sitter::{ffi::TSLanguage, Language, Node, Parser, Point, Tree};

use crate::symbol_index::DocumentSymbolIndex;

extern "C" {
    fn tree_sitter_cea() -> *const TSLanguage;
}

pub struct Document {
    source: String,
    tree: Tree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaVirtualDocument {
    pub source: String,
    pub ranges: Vec<Range>,
}

impl Document {
    pub fn parse(source: String) -> Result<Self, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&cea_language())
            .map_err(|error| error.to_string())?;
        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| "Tree-sitter parser returned no tree".to_owned())?;

        Ok(Self { source, tree })
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        collect_diagnostics(self.tree.root_node(), &self.source, &mut diagnostics, false);
        collect_structure_diagnostics(self.tree.root_node(), &self.source, &mut diagnostics);
        diagnostics
    }

    pub fn symbols(&self) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        collect_symbols(self.tree.root_node(), &self.source, &mut symbols);
        symbols
    }

    pub fn symbol_index(&self) -> DocumentSymbolIndex {
        DocumentSymbolIndex::build(self.tree.root_node(), &self.source)
    }

    pub fn lua_virtual_document(&self) -> LuaVirtualDocument {
        let mut byte_ranges = Vec::new();
        collect_lua_chunks(self.tree.root_node(), &mut byte_ranges);
        byte_ranges.sort_by_key(|range| range.start);

        let ranges = byte_ranges
            .iter()
            .filter_map(|range| {
                self.tree
                    .root_node()
                    .descendant_for_byte_range(range.start, range.end)
            })
            .map(|node| node_range(node, &self.source))
            .collect();

        let mut source = String::with_capacity(self.source.len());
        let mut range_index = 0;
        for (byte, character) in self.source.char_indices() {
            while byte_ranges
                .get(range_index)
                .is_some_and(|range| byte >= range.end)
            {
                range_index += 1;
            }
            let in_lua = byte_ranges
                .get(range_index)
                .is_some_and(|range| range.contains(&byte));

            if in_lua || matches!(character, '\r' | '\n') {
                source.push(character);
            } else {
                source.extend(std::iter::repeat_n(' ', character.len_utf16()));
            }
        }

        LuaVirtualDocument { source, ranges }
    }
}

fn collect_structure_diagnostics(root: Node<'_>, source: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut sections = Vec::new();
    let mut commands = Vec::new();
    collect_nodes(root, &mut sections, &mut commands);

    let mut enable_seen = false;
    let mut disable_seen = false;
    for section in sections {
        let name = node_text(section, source)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let invalid = match name.as_str() {
            "[enable]" => {
                let duplicate = enable_seen || disable_seen;
                enable_seen = true;
                duplicate
            }
            "[disable]" => {
                let invalid = disable_seen || !enable_seen;
                disable_seen = true;
                invalid
            }
            _ => false,
        };
        if invalid {
            diagnostics.push(cea_diagnostic(
                node_range(section, source),
                format!("invalid section usage: {name}"),
            ));
        }
    }

    for command in commands {
        let Some(name_node) = command.child_by_field_name("name") else {
            continue;
        };
        let name = node_text(name_node, source)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let count = argument_count(command, source);
        let valid = match name.as_str() {
            "alloc" => (2..=3).contains(&count),
            "globalalloc" => count == 2,
            "define" | "aobscan" | "assert" => count == 2,
            "aobscanmodule" => count == 3,
            "dealloc" | "createthread" => count == 1,
            "label" | "registersymbol" | "unregistersymbol" => count >= 1,
            "fullaccess" => (1..=2).contains(&count),
            _ => true,
        };
        if !valid {
            diagnostics.push(cea_diagnostic(
                node_range(command, source),
                format!(
                    "invalid arguments for `{}`: received {count}",
                    node_text(name_node, source).unwrap_or_default()
                ),
            ));
        }
    }
}

fn collect_nodes<'tree>(
    node: Node<'tree>,
    sections: &mut Vec<Node<'tree>>,
    commands: &mut Vec<Node<'tree>>,
) {
    match node.kind() {
        "section_header" => sections.push(node),
        "aa_command" => commands.push(node),
        "lua_chunk" => return,
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_nodes(child, sections, commands);
    }
}

fn argument_count(node: Node<'_>, source: &str) -> usize {
    let Some(arguments) = node.child_by_field_name("arguments").or_else(|| {
        let mut cursor = node.walk();
        let arguments = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "argument_list");
        arguments
    }) else {
        return 0;
    };
    let text = node_text(arguments, source).unwrap_or_default();
    usize::from(!text.trim().is_empty()) + text.bytes().filter(|byte| *byte == b',').count()
}

fn cea_diagnostic(range: Range, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("cea".into()),
        message,
        ..Diagnostic::default()
    }
}

fn cea_language() -> Language {
    unsafe { Language::from_raw(tree_sitter_cea()) }
}

fn collect_diagnostics(
    node: Node<'_>,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    parent_reported: bool,
) {
    let report = !parent_reported && (node.is_error() || node.is_missing());
    if report {
        let description = if node.is_missing() {
            format!("expected {}", node.kind())
        } else {
            "could not parse this CEA syntax".to_owned()
        };
        diagnostics.push(Diagnostic {
            range: node_range(node, source),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("cea".into()),
            message: description,
            ..Diagnostic::default()
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_diagnostics(child, source, diagnostics, parent_reported || report);
    }
}

fn collect_symbols(node: Node<'_>, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    if let Some(symbol) = symbol_for_node(node, source) {
        symbols.push(symbol);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_symbols(child, source, symbols);
    }
}

fn collect_lua_chunks(node: Node<'_>, ranges: &mut Vec<std::ops::Range<usize>>) {
    if node.kind() == "lua_chunk" {
        ranges.push(node.byte_range());
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_lua_chunks(child, ranges);
    }
}

#[allow(deprecated)]
fn symbol_for_node(node: Node<'_>, source: &str) -> Option<DocumentSymbol> {
    match node.kind() {
        "section_header" => Some(symbol(
            node_text(node, source)?,
            SymbolKind::NAMESPACE,
            node,
            node,
            source,
        )),
        "label_definition" => {
            let name = node.child_by_field_name("name")?;
            Some(symbol(
                node_text(name, source)?,
                SymbolKind::VARIABLE,
                node,
                name,
                source,
            ))
        }
        "aa_command" => {
            let name = node.child_by_field_name("name")?;
            let command = node_text(name, source)?;
            let kind = match command.to_ascii_lowercase().as_str() {
                "alloc" | "globalalloc" => SymbolKind::VARIABLE,
                "define" => SymbolKind::CONSTANT,
                _ => return None,
            };
            let display_name = first_argument(node, source)
                .map(|argument| format!("{command}({argument})"))
                .unwrap_or_else(|| command.to_owned());
            Some(symbol(display_name, kind, node, name, source))
        }
        _ => None,
    }
}

#[allow(deprecated)]
fn symbol(
    name: impl Into<String>,
    kind: SymbolKind,
    node: Node<'_>,
    selection: Node<'_>,
    source: &str,
) -> DocumentSymbol {
    DocumentSymbol {
        name: name.into(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: node_range(node, source),
        selection_range: node_range(selection, source),
        children: None,
    }
}

fn first_argument<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    let arguments = node.child_by_field_name("arguments").or_else(|| {
        let mut cursor = node.walk();
        let argument_list = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "argument_list");
        argument_list
    })?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    node_text(first, source)
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    source.get(node.byte_range())
}

fn node_range(node: Node<'_>, source: &str) -> Range {
    Range {
        start: lsp_position(source, node.start_position()),
        end: lsp_position(source, node.end_position()),
    }
}

fn lsp_position(source: &str, point: Point) -> Position {
    let line = source.split('\n').nth(point.row).unwrap_or_default();
    let prefix = line.get(..point.column).unwrap_or(line);
    Position::new(point.row as u32, prefix.encode_utf16().count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_sections_labels_allocations_and_definitions() {
        let source = "\
[ENABLE]
define(value, 10)
alloc(storage, 100)
entry:
  mov eax,value
[DISABLE]
dealloc(storage)
";
        let document = Document::parse(source.into()).unwrap();

        let symbols = document.symbols();
        let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();

        assert_eq!(
            names,
            [
                "[ENABLE]",
                "define(value)",
                "alloc(storage)",
                "entry",
                "[DISABLE]"
            ]
        );
        assert_eq!(symbols[1].kind, SymbolKind::CONSTANT);
        assert_eq!(symbols[2].kind, SymbolKind::VARIABLE);
    }

    #[test]
    fn reports_malformed_lua_transition() {
        let document = Document::parse("{$lua}".into()).unwrap();

        let diagnostics = document.diagnostics();

        assert!(!diagnostics.is_empty());
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.source.as_deref() == Some("cea")));
    }

    #[test]
    fn accepts_valid_mixed_document_without_diagnostics() {
        let source = "[ENABLE]\r\n{$lua}\r\nprint('ok')\r\n{$asm}\r\nlabel:\r\n";
        let document = Document::parse(source.into()).unwrap();

        assert!(document.diagnostics().is_empty());
    }

    #[test]
    fn converts_byte_columns_to_utf16_columns() {
        let position = lsp_position("é😀label:\n", Point::new(0, 6));

        assert_eq!(position, Position::new(0, 3));
    }

    #[test]
    fn constructs_position_preserving_virtual_lua_document() {
        let source = "[ENABLE]\r\n{$lua}\r\nlocal café = '😀'\r\n{$asm}\r\nlabel:\r\n";
        let document = Document::parse(source.into()).unwrap();

        let virtual_document = document.lua_virtual_document();

        assert_eq!(
            virtual_document.source,
            "        \r\n      \r\nlocal café = '😀'\r\n      \r\n      \r\n"
        );
        assert_eq!(
            virtual_document.ranges,
            vec![Range::new(Position::new(2, 0), Position::new(3, 0))]
        );
        for (source_line, virtual_line) in source.lines().zip(virtual_document.source.lines()) {
            assert_eq!(
                source_line.encode_utf16().count(),
                virtual_line.encode_utf16().count()
            );
        }
    }

    #[test]
    fn keeps_multiple_lua_regions_in_one_virtual_document() {
        let source = "\
{$lua}
first()
{$asm}
nop
{$lua}
second()
{$asm}
";
        let document = Document::parse(source.into()).unwrap();

        let virtual_document = document.lua_virtual_document();

        assert!(virtual_document.source.contains("first()"));
        assert!(virtual_document.source.contains("second()"));
        assert!(!virtual_document.source.contains("nop"));
        assert_eq!(virtual_document.ranges.len(), 2);
    }

    #[test]
    fn diagnoses_invalid_section_order_and_known_command_arguments() {
        let source = "\
[DISABLE]
alloc(storage)
[ENABLE]
define(value)
[ENABLE]
";
        let document = Document::parse(source.into()).unwrap();

        let diagnostics = document.diagnostics();
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.contains(&"invalid section usage: [disable]"));
        assert!(messages.contains(&"invalid section usage: [enable]"));
        assert!(messages.contains(&"invalid arguments for `alloc`: received 1"));
        assert!(messages.contains(&"invalid arguments for `define`: received 1"));
    }
}
