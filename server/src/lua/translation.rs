use std::collections::HashMap;

use serde_json::Value;
use tower_lsp::lsp_types::{Diagnostic, Position, Range, Url};

use super::ProxyDocument;

pub fn contains(range: &Range, position: Position) -> bool {
    (range.start.line, range.start.character) <= (position.line, position.character)
        && (position.line, position.character) < (range.end.line, range.end.character)
}

pub fn filter_lua_diagnostics(diagnostics: Vec<Diagnostic>, ranges: &[Range]) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            ranges
                .iter()
                .any(|range| contains(range, diagnostic.range.start))
        })
        .collect()
}

pub fn translate_diagnostic_uris(
    mut diagnostics: Vec<Diagnostic>,
    documents: &HashMap<Url, ProxyDocument>,
) -> Vec<Diagnostic> {
    for diagnostic in &mut diagnostics {
        if let Some(related_information) = &mut diagnostic.related_information {
            for related in related_information {
                if let Some(document) = documents.get(&related.location.uri) {
                    related.location.uri = document.source_uri.clone();
                }
            }
        }
        if let Some(data) = diagnostic.data.take() {
            diagnostic.data = Some(translate_response_uris(data, documents));
        }
    }
    diagnostics
}

pub fn translate_response_uris(value: Value, documents: &HashMap<Url, ProxyDocument>) -> Value {
    translate_uri_fields(value, None, documents)
}

pub fn translate_request_uris(value: Value, source_uri: &Url, virtual_uri: &Url) -> Value {
    translate_matching_uri_fields(value, None, source_uri, virtual_uri)
}

fn translate_matching_uri_fields(
    mut value: Value,
    field: Option<&str>,
    source_uri: &Url,
    virtual_uri: &Url,
) -> Value {
    match &mut value {
        Value::String(string) => {
            if field.is_some_and(is_uri_field) && string == source_uri.as_str() {
                *string = virtual_uri.to_string();
            }
        }
        Value::Array(values) => {
            for value in values {
                *value =
                    translate_matching_uri_fields(value.take(), field, source_uri, virtual_uri);
            }
        }
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (key, value) in original {
                let field_name = key.clone();
                let translated_key = if key == source_uri.as_str() {
                    virtual_uri.to_string()
                } else {
                    key
                };
                values.insert(
                    translated_key,
                    translate_matching_uri_fields(
                        value,
                        Some(&field_name),
                        source_uri,
                        virtual_uri,
                    ),
                );
            }
        }
        _ => {}
    }
    value
}

fn translate_uri_fields(
    mut value: Value,
    field: Option<&str>,
    documents: &HashMap<Url, ProxyDocument>,
) -> Value {
    match &mut value {
        Value::String(string) => {
            if field.is_some_and(is_uri_field) {
                if let Ok(uri) = Url::parse(string) {
                    if let Some(document) = documents.get(&uri) {
                        *string = document.source_uri.to_string();
                    }
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                *value = translate_uri_fields(value.take(), field, documents);
            }
        }
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (key, value) in original {
                let field_name = key.clone();
                let translated_key = Url::parse(&key)
                    .ok()
                    .and_then(|uri| documents.get(&uri))
                    .map(|document| document.source_uri.to_string())
                    .unwrap_or(key);
                values.insert(
                    translated_key,
                    translate_uri_fields(value, Some(&field_name), documents),
                );
            }
        }
        _ => {}
    }
    value
}

fn is_uri_field(field: &str) -> bool {
    field == "uri" || field.ends_with("Uri")
}
