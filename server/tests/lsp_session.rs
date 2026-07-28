use serde_json::{json, Value};
use std::{
    env,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};

fn send(writer: &mut impl Write, message: Value) {
    let body = message.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    writer.flush().unwrap();
}

fn receive(reader: &mut impl BufRead) -> Value {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length: ") {
            content_length = Some(value.trim().parse::<usize>().unwrap());
        }
    }

    let mut body = vec![0; content_length.expect("response must have a Content-Length header")];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn receive_matching(reader: &mut impl BufRead, predicate: impl Fn(&Value) -> bool) -> Value {
    loop {
        let message = receive(reader);
        if predicate(&message) {
            return message;
        }
    }
}

#[test]
fn serves_diagnostics_and_document_symbols_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cea-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {},
                "processId": null,
                "rootUri": null
            }
        }),
    );
    let initialize = receive_matching(&mut stdout, |message| message["id"] == 1);
    assert_eq!(initialize["result"]["capabilities"]["textDocumentSync"], 1);
    assert_eq!(
        initialize["result"]["capabilities"]["documentSymbolProvider"],
        true
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///fixture.cea",
                    "languageId": "cea",
                    "version": 1,
                    "text": "[ENABLE]\ndefine(value, 10)\nentry:\n"
                }
            }
        }),
    );
    let diagnostics = receive_matching(&mut stdout, |message| {
        message["method"] == "textDocument/publishDiagnostics"
    });
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": {
                "textDocument": {
                    "uri": "file:///fixture.cea"
                }
            }
        }),
    );
    let symbols = receive_matching(&mut stdout, |message| message["id"] == 2);
    let names: Vec<_> = symbols["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["[ENABLE]", "define(value)", "entry"]);

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "shutdown",
            "params": null
        }),
    );
    receive_matching(&mut stdout, |message| message["id"] == 3);
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    );
    drop(stdin);

    assert!(child.wait().unwrap().success());
}

#[test]
fn forwards_embedded_lua_syntax_and_semantic_diagnostics_over_stdio() {
    if !command_exists("lua-language-server") {
        eprintln!("skipping Lua proxy integration test: lua-language-server is not on PATH");
        return;
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_cea-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {},
                "processId": null,
                "rootUri": "file:///tmp/cea-lsp-proxy-test"
            }
        }),
    );
    receive_matching(&mut stdout, |message| message["id"] == 1);
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );
    receive_matching(&mut stdout, |message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"] == "Lua language server proxy initialized"
    });

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea",
                    "languageId": "cea",
                    "version": 1,
                    "text": "{$lua}\nlocal broken =\n{$asm}\n"
                }
            }
        }),
    );
    let diagnostics = receive_matching(&mut stdout, |message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["uri"] == "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| {
                    diagnostics.iter().any(|diagnostic| {
                        diagnostic["source"]
                            .as_str()
                            .is_some_and(|source| source.contains("Lua"))
                    })
                })
    });
    assert!(diagnostics["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .all(|diagnostic| diagnostic["range"]["start"]["line"] == 1));

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea",
                    "version": 2
                },
                "contentChanges": [{
                    "text": "{$lua}\n[ENABLE]\n\nlocal enabled = true\n[DISABLE]\n\n---@param y number\n---@return number\nlocal function x(y)\n  return y + 1\nend\n\nx(\"hello\")\n"
                }]
            }
        }),
    );
    let diagnostics = receive_matching(&mut stdout, |message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["uri"] == "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| {
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic["code"] == "param-type-mismatch")
                })
    });
    assert_eq!(
        diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["code"] == "param-type-mismatch")
            .unwrap()["range"]["start"]["line"],
        12
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea",
                    "version": 3
                },
                "contentChanges": [{
                    "text": "{$lua}\nlocal proxy_value = 1\nprint(proxy_value)\n{$asm}\n"
                }]
            }
        }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea"
                },
                "position": {
                    "line": 2,
                    "character": 8
                }
            }
        }),
    );
    let definition = receive_matching(&mut stdout, |message| message["id"] == 3);
    let definition_uri = definition["result"]
        .get(0)
        .and_then(|location| location.get("uri").or_else(|| location.get("targetUri")))
        .or_else(|| {
            definition["result"]
                .get("uri")
                .or_else(|| definition["result"].get("targetUri"))
        });
    assert_eq!(
        definition_uri.and_then(Value::as_str),
        Some("file:///tmp/cea-lsp-proxy-test/proxy-fixture.cea")
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "shutdown",
            "params": null
        }),
    );
    receive_matching(&mut stdout, |message| message["id"] == 4);
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    );
    drop(stdin);

    assert!(child.wait().unwrap().success());
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| Path::new(&directory).join(command).is_file())
    })
}
