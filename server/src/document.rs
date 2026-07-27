use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentSymbol, Position, Range, SymbolKind,
};
use tree_sitter::{ffi::TSLanguage, Language, Node, Parser, Point, Tree};

extern "C" {
    fn tree_sitter_cea() -> *const TSLanguage;
}

pub struct Document {
    source: String,
    tree: Tree,
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
        diagnostics
    }

    pub fn symbols(&self) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        collect_symbols(self.tree.root_node(), &self.source, &mut symbols);
        symbols
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
}
