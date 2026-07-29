use std::collections::HashSet;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentSymbol, Position, Range, SymbolKind,
};
use tree_sitter::{ffi::TSLanguage, Language, Node, Parser, Point, Tree};

use crate::symbol_index::{CeaSymbolKind, DocumentSymbolIndex, OccurrenceRole};

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
        let symbol_index = self.symbol_index();
        collect_diagnostics(self.tree.root_node(), &self.source, &mut diagnostics, false);
        collect_structure_diagnostics(self.tree.root_node(), &self.source, &mut diagnostics);
        collect_label_diagnostics(
            self.tree.root_node(),
            &self.source,
            &symbol_index,
            &mut diagnostics,
        );
        collect_strict_diagnostics(&self.source, &symbol_index, &mut diagnostics);
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

    pub fn integer_hover(&self, position: Position) -> Option<(String, Range)> {
        let byte = byte_offset_at_position(&self.source, position)?;
        let end = (byte + 1).min(self.source.len());
        let mut node = self.tree.root_node().descendant_for_byte_range(byte, end)?;
        while !matches!(node.kind(), "number" | "typed_number") {
            node = node.parent()?;
        }

        let text = node_text(node, &self.source)?;
        integer_conversion(text).map(|hover| (hover, node_range(node, &self.source)))
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

    pub fn native_completion_allowed(&self, position: Position) -> bool {
        if self
            .lua_virtual_document()
            .ranges
            .iter()
            .any(|range| range.start <= position && position < range.end)
        {
            return false;
        }
        let Some(line) = self.source.lines().nth(position.line as usize) else {
            return false;
        };
        let prefix: String = line
            .chars()
            .scan(0_u32, |width, character| {
                *width += character.len_utf16() as u32;
                (*width <= position.character).then_some(character)
            })
            .collect();
        let trimmed = prefix.trim_start();
        !trimmed.starts_with("//") && !trimmed.starts_with("{$lua")
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
    let missing_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    if !enable_seen {
        diagnostics.push(cea_diagnostic(
            missing_range,
            "missing required [ENABLE] section".into(),
        ));
    }
    if !disable_seen {
        diagnostics.push(cea_diagnostic(
            missing_range,
            "missing required [DISABLE] section".into(),
        ));
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
            "alloc" | "allocnx" | "allocxo" | "globalalloc" => (2..=3).contains(&count),
            "kalloc" => count == 2,
            "define" | "aobscan" | "assert" => count == 2,
            "aobscanmodule" => count == 3,
            "aobscanregion" => count == 4,
            "dealloc" | "createthread" | "include" | "loadlibrary" | "reassemble" => count == 1,
            "createthreadandwait" => (1..=2).contains(&count),
            "label" | "registersymbol" | "unregistersymbol" => count >= 1,
            "fullaccess" | "loadbinary" | "readmem" => count == 2,
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

    collect_value_notation_diagnostics(root, source, diagnostics);
}

fn collect_value_notation_diagnostics(
    node: Node<'_>,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.kind() == "type_cast"
        && node
            .parent()
            .is_none_or(|parent| parent.kind() != "typed_number")
        && node_text(node, source).is_some_and(|cast| {
            matches!(
                cast.to_ascii_lowercase().as_str(),
                "(int)" | "(float)" | "(double)"
            )
        })
    {
        diagnostics.push(cea_diagnostic(
            node_range(node, source),
            format!(
                "invalid value notation: `{}` must be followed by a decimal value",
                node_text(node, source).unwrap_or_default()
            ),
        ));
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_value_notation_diagnostics(child, source, diagnostics);
    }
}

fn collect_strict_diagnostics(
    source: &str,
    index: &DocumentSymbolIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !source.to_ascii_uppercase().contains("{$STRICT}") {
        return;
    }
    let mut occurrences = index.occurrences().to_vec();
    occurrences.sort_by_key(|occurrence| occurrence.range.start);
    let mut declared_labels = HashSet::new();
    for occurrence in occurrences {
        if occurrence.kind != CeaSymbolKind::Label {
            continue;
        }
        if occurrence.role == OccurrenceRole::Declaration {
            declared_labels.insert(occurrence.name.to_ascii_lowercase());
        } else if occurrence.role == OccurrenceRole::Definition
            && !declared_labels.contains(&occurrence.name.to_ascii_lowercase())
        {
            diagnostics.push(cea_diagnostic(
                occurrence.range,
                format!(
                    "`{{$STRICT}}` requires label `{}` to be declared before use",
                    occurrence.name
                ),
            ));
        }
    }
}

fn collect_label_diagnostics(
    root: Node<'_>,
    source: &str,
    index: &DocumentSymbolIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let definitions: HashSet<_> = index
        .occurrences()
        .iter()
        .filter(|occurrence| {
            occurrence.kind == CeaSymbolKind::Label && occurrence.role == OccurrenceRole::Definition
        })
        .map(|occurrence| occurrence.name.to_ascii_lowercase())
        .collect();

    for declaration in index.occurrences().iter().filter(|occurrence| {
        occurrence.kind == CeaSymbolKind::Label
            && occurrence.role == OccurrenceRole::Declaration
            && !definitions.contains(&occurrence.name.to_ascii_lowercase())
    }) {
        diagnostics.push(cea_diagnostic(
            declaration.range,
            format!(
                "label `{}` is declared but never defined in this script",
                declaration.name
            ),
        ));
    }

    collect_invalid_label_lines(root, source, diagnostics);
}

fn collect_invalid_label_lines(node: Node<'_>, source: &str, diagnostics: &mut Vec<Diagnostic>) {
    if node.kind() == "invalid_label_definition_line" {
        if let Some(definition) = node.named_child(0) {
            let text = node_text(definition, source).unwrap_or_default();
            if let Some(label) = text.split(':').next() {
                diagnostics.push(cea_diagnostic(
                    node_range(definition, source),
                    format!(
                        "label `{label}:` must be on its own line; only a comment may follow it"
                    ),
                ));
            }
        }
        return;
    }
    if node.kind() == "lua_chunk" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_invalid_label_lines(child, source, diagnostics);
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
            if node
                .parent()
                .is_none_or(|parent| parent.kind() != "label_definition_line")
            {
                return None;
            }
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
                "alloc" | "allocnx" | "allocxo" | "globalalloc" | "kalloc" => SymbolKind::VARIABLE,
                "define" | "aobscan" | "aobscanmodule" | "aobscanregion" => SymbolKind::CONSTANT,
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

fn byte_offset_at_position(source: &str, position: Position) -> Option<usize> {
    let line_start = source
        .split_inclusive('\n')
        .take(position.line as usize)
        .map(str::len)
        .sum::<usize>();
    let line = source.get(line_start..)?.split('\n').next()?;
    let mut utf16 = 0_u32;
    for (byte, character) in line.char_indices() {
        if utf16 == position.character {
            return Some(line_start + byte);
        }
        utf16 += character.len_utf16() as u32;
        if utf16 > position.character {
            return None;
        }
    }
    (utf16 == position.character).then_some(line_start + line.len())
}

fn integer_conversion(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("(float)") || lower.starts_with("(double)") {
        return None;
    }

    if let Some(decimal) = lower.strip_prefix("(int)") {
        return signed_decimal_conversion(decimal);
    }
    if let Some(decimal) = text.strip_prefix('#') {
        if decimal.starts_with(['+', '-']) {
            return signed_decimal_conversion(decimal);
        }
        let value = decimal.parse::<u64>().ok()?;
        return Some(unsigned_conversion(value, format!("{value:X}").len()));
    }

    let digits = text
        .strip_prefix('$')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    let value = u64::from_str_radix(digits, 16).ok()?;
    Some(unsigned_conversion(value, digits.len()))
}

fn signed_decimal_conversion(decimal: &str) -> Option<String> {
    let value = decimal.parse::<i64>().ok()?;
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    Some(format!("{sign}0x{magnitude:X}\n{sign}0d{magnitude}"))
}

fn unsigned_conversion(value: u64, hexadecimal_digits: usize) -> String {
    let mut hover = format!("0x{value:X}\n0d{value}");
    let width = match hexadecimal_digits {
        0..=2 => 8,
        3..=4 => 16,
        5..=8 => 32,
        _ => 64,
    };
    let sign_bit = 1_u64 << (width - 1);
    if value & sign_bit != 0 {
        let signed = if width == 64 {
            value as i64 as i128
        } else {
            value as i128 - (1_i128 << width)
        };
        hover.push_str(&format!("\n{signed}"));
    }
    hover
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
        let source = "[ENABLE]\r\n{$lua}\r\nprint('ok')\r\n{$asm}\r\nlabel:\r\n[DISABLE]\r\n";
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
        assert!(!messages.contains(&"missing required [ENABLE] section"));
        assert!(!messages.contains(&"missing required [DISABLE] section"));
    }

    #[test]
    fn validates_documented_command_argument_counts() {
        let source = "\
[ENABLE]
aobscanregion(result,start,stop)
globalalloc(storage)
kalloc(kernelstorage)
fullaccess(storage)
createthreadandwait(worker,1000,extra)
loadbinary(storage)
readmem(storage)
[DISABLE]
";
        let document = Document::parse(source.into()).unwrap();
        let messages: Vec<_> = document
            .diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        for command in [
            "aobscanregion",
            "globalalloc",
            "kalloc",
            "fullaccess",
            "createthreadandwait",
            "loadbinary",
            "readmem",
        ] {
            assert!(
                messages
                    .iter()
                    .any(|message| message.contains(&format!("`{command}`"))),
                "missing diagnostic for {command}: {messages:?}"
            );
        }
    }

    #[test]
    fn accepts_documented_value_notation_and_rejects_missing_values() {
        let source = "\
[ENABLE]
define(decimal,#100)
define(whole,(int)-100)
define(single,(float)100.1)
define(scientific,(double)-1.25e+2)
dd (float)
[DISABLE]
";
        let document = Document::parse(source.into()).unwrap();
        let diagnostics = document.diagnostics();

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.message.contains("invalid value notation"))
                .count(),
            1
        );
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("`(float)` must be followed by a decimal value")));
    }

    #[test]
    fn converts_integer_literals_for_hover() {
        let document = Document::parse(
            "\
[ENABLE]
dd $FFFFFFFF
dd #4294967295
dd 10
dd (int)-1
dd (float)1.0
[DISABLE]
"
            .into(),
        )
        .unwrap();

        assert_eq!(
            document.integer_hover(Position::new(1, 5)).unwrap().0,
            "0xFFFFFFFF\n0d4294967295\n-1"
        );
        assert_eq!(
            document.integer_hover(Position::new(2, 5)).unwrap().0,
            "0xFFFFFFFF\n0d4294967295\n-1"
        );
        assert_eq!(
            document.integer_hover(Position::new(3, 4)).unwrap().0,
            "0x10\n0d16"
        );
        assert_eq!(
            document.integer_hover(Position::new(4, 9)).unwrap().0,
            "-0x1\n-0d1"
        );
        assert!(document.integer_hover(Position::new(5, 11)).is_none());
    }

    #[test]
    fn requires_enable_and_disable_sections() {
        let document = Document::parse("nop\n".into()).unwrap();
        let messages: Vec<_> = document
            .diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        assert!(messages.contains(&"missing required [ENABLE] section".into()));
        assert!(messages.contains(&"missing required [DISABLE] section".into()));
    }

    #[test]
    fn strict_requires_label_commands_above_label_definitions() {
        let source = "\
{$STRICT}
[ENABLE]
missing:
label(ready)
ready:
too_late:
label(too_late)
jmp not_a_label_definition
00400500:
[ptr]:
[DISABLE]
";
        let document = Document::parse(source.into()).unwrap();
        let diagnostics = document.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `missing` to be declared before use")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `too_late` to be declared before use")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("label `ready`")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `not_a_label_definition`")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("00400500")));
    }

    #[test]
    fn diagnoses_labels_that_are_missing_definitions_or_share_code_lines() {
        let source = "\
[ENABLE]
label(missing)
label(valid)
valid: // comment
label(block_comment)
block_comment: { comment }
label(invalid)
invalid: nop
[DISABLE]
";
        let document = Document::parse(source.into()).unwrap();
        let diagnostics = document.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `missing` is declared but never defined")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `invalid:` must be on its own line")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `valid` is declared but never defined")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("label `block_comment` is declared but never defined")));
    }

    #[test]
    fn limits_native_completion_to_assembly_code() {
        let document = Document::parse(
            "[ENABLE]\nmov eax,\n// shared\n{$lua}\nshared\n{$asm}\n[DISABLE]\n".into(),
        )
        .unwrap();

        assert!(document.native_completion_allowed(Position::new(1, 8)));
        assert!(!document.native_completion_allowed(Position::new(2, 5)));
        assert!(!document.native_completion_allowed(Position::new(4, 3)));
    }
}
