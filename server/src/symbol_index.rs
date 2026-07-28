use std::collections::HashMap;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};
use tree_sitter::{Node, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CeaSymbolKind {
    Label,
    Allocation,
    Definition,
    Registered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OccurrenceRole {
    Declaration,
    Definition,
    Reference,
    Registration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub name: String,
    pub range: Range,
    pub kind: CeaSymbolKind,
    pub role: OccurrenceRole,
    pub strict_reference: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentSymbolIndex {
    occurrences: Vec<SymbolOccurrence>,
}

impl DocumentSymbolIndex {
    pub fn build(root: Node<'_>, source: &str) -> Self {
        let mut occurrences = Vec::new();
        collect_occurrences(root, source, &mut occurrences);
        Self { occurrences }
    }

    pub fn occurrences(&self) -> &[SymbolOccurrence] {
        &self.occurrences
    }

    pub fn occurrence_at(&self, position: Position) -> Option<&SymbolOccurrence> {
        self.occurrences
            .iter()
            .filter(|occurrence| contains(occurrence.range, position))
            .min_by_key(|occurrence| range_size(occurrence.range))
    }
}

#[derive(Debug, Clone)]
pub struct IndexedOccurrence {
    pub uri: Url,
    pub occurrence: SymbolOccurrence,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceSymbolIndex {
    documents: HashMap<Url, DocumentSymbolIndex>,
}

impl WorkspaceSymbolIndex {
    pub fn update(&mut self, uri: Url, index: DocumentSymbolIndex) {
        self.documents.insert(uri, index);
    }

    pub fn remove(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn occurrence_at(&self, uri: &Url, position: Position) -> Option<&SymbolOccurrence> {
        self.documents
            .get(uri)
            .and_then(|index| index.occurrence_at(position))
    }

    pub fn occurrences_named(&self, name: &str) -> Vec<IndexedOccurrence> {
        let normalized = normalize(name);
        self.documents
            .iter()
            .flat_map(|(uri, index)| {
                index
                    .occurrences()
                    .iter()
                    .filter(|occurrence| normalize(&occurrence.name) == normalized)
                    .map(|occurrence| IndexedOccurrence {
                        uri: uri.clone(),
                        occurrence: occurrence.clone(),
                    })
            })
            .collect()
    }

    pub fn declarations_named(&self, name: &str) -> Vec<IndexedOccurrence> {
        let mut occurrences: Vec<_> = self
            .occurrences_named(name)
            .into_iter()
            .filter(|indexed| {
                matches!(
                    indexed.occurrence.role,
                    OccurrenceRole::Declaration | OccurrenceRole::Definition
                )
            })
            .collect();
        occurrences.sort_by_key(|indexed| match indexed.occurrence.role {
            OccurrenceRole::Definition => 0,
            OccurrenceRole::Declaration => 1,
            _ => 2,
        });
        occurrences
    }

    pub fn symbol_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .documents
            .values()
            .flat_map(DocumentSymbolIndex::occurrences)
            .filter(|occurrence| {
                matches!(
                    occurrence.role,
                    OccurrenceRole::Declaration | OccurrenceRole::Definition
                )
            })
            .map(|occurrence| occurrence.name.clone())
            .collect();
        names.sort_by_key(|name| normalize(name));
        names.dedup_by(|left, right| normalize(left) == normalize(right));
        names
    }

    pub fn semantic_diagnostics(&self) -> HashMap<Url, Vec<Diagnostic>> {
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = self
            .documents
            .keys()
            .cloned()
            .map(|uri| (uri, Vec::new()))
            .collect();

        for (uri, document) in &self.documents {
            for occurrence in document.occurrences().iter().filter(|occurrence| {
                occurrence.strict_reference || occurrence.role == OccurrenceRole::Registration
            }) {
                if self.declarations_named(&occurrence.name).is_empty() {
                    diagnostics.entry(uri.clone()).or_default().push(diagnostic(
                        occurrence.range,
                        format!("unresolved CEA symbol `{}`", occurrence.name),
                    ));
                }
            }
        }

        let mut declarations: HashMap<String, Vec<IndexedOccurrence>> = HashMap::new();
        for (uri, document) in &self.documents {
            for occurrence in document.occurrences().iter().filter(|occurrence| {
                matches!(
                    occurrence.role,
                    OccurrenceRole::Declaration | OccurrenceRole::Definition
                )
            }) {
                declarations
                    .entry(normalize(&occurrence.name))
                    .or_default()
                    .push(IndexedOccurrence {
                        uri: uri.clone(),
                        occurrence: occurrence.clone(),
                    });
            }
        }
        for occurrences in declarations.values() {
            for duplicate in duplicate_declarations(occurrences) {
                diagnostics
                    .entry(duplicate.uri.clone())
                    .or_default()
                    .push(diagnostic(
                        duplicate.occurrence.range,
                        format!(
                            "duplicate CEA {} `{}`",
                            symbol_kind_name(duplicate.occurrence.kind),
                            duplicate.occurrence.name
                        ),
                    ));
            }
        }

        diagnostics
    }
}

fn duplicate_declarations(occurrences: &[IndexedOccurrence]) -> Vec<&IndexedOccurrence> {
    let mut seen: HashMap<(CeaSymbolKind, OccurrenceRole), &IndexedOccurrence> = HashMap::new();
    let mut duplicates = Vec::new();
    for occurrence in occurrences {
        let key = (occurrence.occurrence.kind, occurrence.occurrence.role);
        if seen.insert(key, occurrence).is_some() {
            duplicates.push(occurrence);
        }
    }
    duplicates
}

fn diagnostic(range: Range, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("cea".into()),
        message,
        ..Diagnostic::default()
    }
}

fn symbol_kind_name(kind: CeaSymbolKind) -> &'static str {
    match kind {
        CeaSymbolKind::Label => "label",
        CeaSymbolKind::Allocation => "allocation",
        CeaSymbolKind::Definition => "definition",
        CeaSymbolKind::Registered => "registration",
    }
}

fn collect_occurrences(node: Node<'_>, source: &str, occurrences: &mut Vec<SymbolOccurrence>) {
    match node.kind() {
        "label_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                push_occurrence(
                    name,
                    source,
                    CeaSymbolKind::Label,
                    OccurrenceRole::Definition,
                    false,
                    occurrences,
                );
            }
        }
        "aa_command" => collect_command(node, source, occurrences),
        "operation" => collect_operation_references(node, source, occurrences),
        "lua_chunk" => {
            collect_lua_symbol_references(node, source, occurrences);
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_occurrences(child, source, occurrences);
    }
}

fn collect_command(node: Node<'_>, source: &str, occurrences: &mut Vec<SymbolOccurrence>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Some(command) = node_text(name_node, source) else {
        return;
    };
    let arguments = argument_nodes(node);
    let normalized = command.to_ascii_lowercase();

    match normalized.as_str() {
        "alloc" | "globalalloc" => {
            if let Some(first) = arguments.first().copied() {
                push_occurrence(
                    first,
                    source,
                    CeaSymbolKind::Allocation,
                    OccurrenceRole::Declaration,
                    false,
                    occurrences,
                );
            }
            collect_identifier_references(
                &arguments[arguments.len().min(2)..],
                source,
                false,
                occurrences,
            );
        }
        "define" => {
            if let Some(first) = arguments.first().copied() {
                push_occurrence(
                    first,
                    source,
                    CeaSymbolKind::Definition,
                    OccurrenceRole::Declaration,
                    false,
                    occurrences,
                );
            }
            collect_identifier_references(
                &arguments[arguments.len().min(1)..],
                source,
                false,
                occurrences,
            );
        }
        "label" => {
            for argument in arguments {
                push_occurrence(
                    argument,
                    source,
                    CeaSymbolKind::Label,
                    OccurrenceRole::Declaration,
                    false,
                    occurrences,
                );
            }
        }
        "registersymbol" => {
            for argument in arguments {
                push_occurrence(
                    argument,
                    source,
                    CeaSymbolKind::Registered,
                    OccurrenceRole::Registration,
                    true,
                    occurrences,
                );
            }
        }
        "dealloc" | "unregistersymbol" => {
            collect_identifier_references(&arguments, source, true, occurrences);
        }
        _ => collect_identifier_references(&arguments, source, false, occurrences),
    }
}

fn collect_operation_references(
    node: Node<'_>,
    source: &str,
    occurrences: &mut Vec<SymbolOccurrence>,
) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    let mut cursor = node.walk();
    let arguments: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|child| child.id() != name.id())
        .collect();
    collect_identifier_references(&arguments, source, false, occurrences);
}

fn argument_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    let Some(arguments) = argument_list(node) else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    arguments.named_children(&mut cursor).collect()
}

fn argument_list(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("arguments").or_else(|| {
        let mut cursor = node.walk();
        let arguments = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "argument_list");
        arguments
    })
}

fn collect_identifier_references(
    nodes: &[Node<'_>],
    source: &str,
    strict: bool,
    occurrences: &mut Vec<SymbolOccurrence>,
) {
    for node in nodes {
        if node.kind() == "identifier" {
            push_occurrence(
                *node,
                source,
                CeaSymbolKind::Label,
                OccurrenceRole::Reference,
                strict,
                occurrences,
            );
        }
    }
}

fn collect_lua_symbol_references(
    node: Node<'_>,
    source: &str,
    occurrences: &mut Vec<SymbolOccurrence>,
) {
    let range = node.byte_range();
    let Some(chunk) = source.get(range.clone()) else {
        return;
    };
    for (value_start, value_end) in lua_address_arguments(chunk) {
        let absolute_start = range.start + value_start;
        let absolute_end = range.start + value_end;
        occurrences.push(SymbolOccurrence {
            name: chunk[value_start..value_end].to_owned(),
            range: byte_range(source, absolute_start, absolute_end),
            kind: CeaSymbolKind::Registered,
            role: OccurrenceRole::Reference,
            strict_reference: true,
        });
    }
}

fn lua_address_arguments(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut arguments = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' | b'"' => cursor = quoted_end(bytes, cursor).unwrap_or(bytes.len()),
            b'-' if bytes.get(cursor + 1) == Some(&b'-') => {
                cursor = bytes[cursor + 2..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |offset| cursor + 3 + offset);
            }
            b'[' if bytes.get(cursor + 1) == Some(&b'[') => {
                cursor = bytes[cursor + 2..]
                    .windows(2)
                    .position(|window| window == b"]]")
                    .map_or(bytes.len(), |offset| cursor + 4 + offset);
            }
            byte if is_lua_identifier_start(byte) => {
                let identifier_start = cursor;
                cursor += 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| is_lua_identifier_continue(*byte))
                {
                    cursor += 1;
                }
                let identifier = &source[identifier_start..cursor];
                if !matches!(identifier, "getAddress" | "getAddressSafe")
                    || previous_non_whitespace(bytes, identifier_start)
                        .is_some_and(|byte| matches!(byte, b'.' | b':'))
                {
                    continue;
                }
                let mut call = skip_ascii_whitespace(bytes, cursor);
                if bytes.get(call) != Some(&b'(') {
                    continue;
                }
                call = skip_ascii_whitespace(bytes, call + 1);
                let Some(quote @ (b'\'' | b'"')) = bytes.get(call).copied() else {
                    continue;
                };
                let value_start = call + 1;
                let Some(value_end) = unescaped_quote(bytes, value_start, quote) else {
                    continue;
                };
                if value_start < value_end
                    && !bytes[value_start..value_end].contains(&b'\\')
                    && source.is_char_boundary(value_start)
                    && source.is_char_boundary(value_end)
                {
                    arguments.push((value_start, value_end));
                }
                cursor = value_end + 1;
            }
            _ => cursor += 1,
        }
    }
    arguments
}

fn quoted_end(source: &[u8], quote_start: usize) -> Option<usize> {
    unescaped_quote(source, quote_start + 1, source[quote_start]).map(|end| end + 1)
}

fn unescaped_quote(source: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut cursor = start;
    while cursor < source.len() {
        if source[cursor] == b'\\' {
            cursor += 2;
        } else if source[cursor] == quote {
            return Some(cursor);
        } else {
            cursor += 1;
        }
    }
    None
}

fn skip_ascii_whitespace(source: &[u8], mut cursor: usize) -> usize {
    while source
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    cursor
}

fn previous_non_whitespace(source: &[u8], cursor: usize) -> Option<u8> {
    source[..cursor]
        .iter()
        .rev()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
}

fn is_lua_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_lua_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn push_occurrence(
    node: Node<'_>,
    source: &str,
    kind: CeaSymbolKind,
    role: OccurrenceRole,
    strict_reference: bool,
    occurrences: &mut Vec<SymbolOccurrence>,
) {
    if let Some(name) = node_text(node, source) {
        occurrences.push(SymbolOccurrence {
            name: name.to_owned(),
            range: node_range(node, source),
            kind,
            role,
            strict_reference,
        });
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    source.get(node.byte_range())
}

fn node_range(node: Node<'_>, source: &str) -> Range {
    Range::new(
        lsp_position(source, node.start_position()),
        lsp_position(source, node.end_position()),
    )
}

fn byte_range(source: &str, start: usize, end: usize) -> Range {
    Range::new(byte_position(source, start), byte_position(source, end))
}

fn byte_position(source: &str, byte: usize) -> Position {
    let prefix = source.get(..byte).unwrap_or(source);
    let line = prefix.bytes().filter(|value| *value == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    Position::new(
        line as u32,
        prefix[line_start..].encode_utf16().count() as u32,
    )
}

fn lsp_position(source: &str, point: Point) -> Position {
    let line = source.split('\n').nth(point.row).unwrap_or_default();
    let prefix = line.get(..point.column).unwrap_or(line);
    Position::new(point.row as u32, prefix.encode_utf16().count() as u32)
}

fn contains(range: Range, position: Position) -> bool {
    range.start <= position && position <= range.end
}

fn range_size(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

pub fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{ffi::TSLanguage, Language, Parser};

    extern "C" {
        fn tree_sitter_cea() -> *const TSLanguage;
    }

    fn index(source: &str) -> DocumentSymbolIndex {
        let language = unsafe { Language::from_raw(tree_sitter_cea()) };
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        DocumentSymbolIndex::build(tree.root_node(), source)
    }

    #[test]
    fn indexes_declarations_definitions_registrations_and_references() {
        let source = "\
[ENABLE]
define(target,game.exe+10)
alloc(storage,100,target)
label(return)
registersymbol(storage)
return:
  mov rax,storage
[DISABLE]
unregistersymbol(storage)
dealloc(storage)
";

        let index = index(source);
        let summarized: Vec<_> = index
            .occurrences()
            .iter()
            .map(|occurrence| {
                (
                    occurrence.name.as_str(),
                    occurrence.kind,
                    occurrence.role,
                    occurrence.strict_reference,
                )
            })
            .collect();

        assert!(summarized.contains(&(
            "target",
            CeaSymbolKind::Definition,
            OccurrenceRole::Declaration,
            false
        )));
        assert!(summarized.contains(&(
            "storage",
            CeaSymbolKind::Allocation,
            OccurrenceRole::Declaration,
            false
        )));
        assert!(summarized.contains(&(
            "return",
            CeaSymbolKind::Label,
            OccurrenceRole::Definition,
            false
        )));
        assert!(summarized.contains(&(
            "storage",
            CeaSymbolKind::Registered,
            OccurrenceRole::Registration,
            true
        )));
        assert!(summarized.contains(&(
            "storage",
            CeaSymbolKind::Label,
            OccurrenceRole::Reference,
            true
        )));
    }

    #[test]
    fn links_lua_address_apis_to_cea_symbols_with_utf16_ranges() {
        let source = "\
[ENABLE]
alloc(café,100)
{$lua}
local address = getAddress(\"café\")
local safe = getAddressSafe ( 'missing' )
{$asm}
";

        let index = index(source);
        let lua_references: Vec<_> = index
            .occurrences()
            .iter()
            .filter(|occurrence| {
                occurrence.role == OccurrenceRole::Reference && occurrence.strict_reference
            })
            .collect();

        assert_eq!(
            lua_references
                .iter()
                .map(|occurrence| occurrence.name.as_str())
                .collect::<Vec<_>>(),
            ["café", "missing"]
        );
        assert_eq!(
            lua_references[0].range,
            Range::new(Position::new(3, 28), Position::new(3, 32))
        );
    }

    #[test]
    fn ignores_address_api_text_that_is_not_a_direct_lua_call() {
        let source = "\
{$lua}
-- getAddress(\"comment\")
local text = \"getAddress('string')\"
object.getAddress(\"method\")
getAddress(dynamic)
getAddress(\"direct\")
{$asm}
";

        let index = index(source);
        let references: Vec<_> = index
            .occurrences()
            .iter()
            .filter(|occurrence| occurrence.strict_reference)
            .map(|occurrence| occurrence.name.as_str())
            .collect();

        assert_eq!(references, ["direct"]);
    }

    #[test]
    fn workspace_index_matches_symbols_case_insensitively() {
        let uri = Url::parse("file:///player.cea").unwrap();
        let mut workspace = WorkspaceSymbolIndex::default();
        workspace.update(uri, index("alloc(PlayerHealth,100)\n"));

        assert_eq!(workspace.declarations_named("playerhealth").len(), 1);
        assert_eq!(workspace.symbol_names(), ["PlayerHealth"]);
    }

    #[test]
    fn diagnoses_duplicate_declarations_and_strict_unresolved_references() {
        let first_uri = Url::parse("file:///one.cea").unwrap();
        let second_uri = Url::parse("file:///two.cea").unwrap();
        let mut workspace = WorkspaceSymbolIndex::default();
        workspace.update(
            first_uri.clone(),
            index("alloc(storage,100)\nregistersymbol(missing)\n"),
        );
        workspace.update(
            second_uri.clone(),
            index("alloc(STORAGE,200)\ndealloc(unknown)\n"),
        );

        let diagnostics = workspace.semantic_diagnostics();

        assert_eq!(diagnostics[&first_uri].len(), 1);
        assert!(diagnostics[&first_uri][0]
            .message
            .contains("unresolved CEA symbol `missing`"));
        assert_eq!(diagnostics[&second_uri].len(), 2);
        assert!(diagnostics[&second_uri]
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate CEA allocation")));
        assert!(diagnostics[&second_uri].iter().any(|diagnostic| diagnostic
            .message
            .contains("unresolved CEA symbol `unknown`")));
    }

    #[test]
    fn allows_a_label_declaration_and_its_definition() {
        let uri = Url::parse("file:///labels.cea").unwrap();
        let mut workspace = WorkspaceSymbolIndex::default();
        workspace.update(uri.clone(), index("label(return)\nreturn:\n"));

        assert!(workspace.semantic_diagnostics()[&uri].is_empty());
    }
}
