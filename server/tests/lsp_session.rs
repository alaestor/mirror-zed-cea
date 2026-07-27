use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
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
