use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
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
    assert_eq!(
        initialize["result"]["capabilities"]["workspace"]["workspaceFolders"]["supported"],
        true
    );
    assert_eq!(
        initialize["result"]["capabilities"]["workspace"]["workspaceFolders"]
            ["changeNotifications"],
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

#[test]
fn restarts_lua_ls_and_resynchronizes_open_documents() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture_dir =
        env::temp_dir().join(format!("cea-luals-restart-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&fixture_dir).unwrap();
    let fake_lua_ls = write_fake_lua_ls(&fixture_dir, FAKE_RESTARTING_LUA_LS);
    let workspace_uri = format!("file://{}", fixture_dir.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_cea-language-server"))
        .env("CEA_LUA_LANGUAGE_SERVER", &fake_lua_ls)
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
                "rootUri": workspace_uri
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
                    "uri": "file:///restart-fixture.cea",
                    "languageId": "cea",
                    "version": 4,
                    "text": "{$lua}\nprint('resync me')\n{$asm}\n"
                }
            }
        }),
    );
    let failure = receive_matching(&mut stdout, |message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|message| message.ends_with("; restarting LuaLS"))
    });
    let failure_message = failure["params"]["message"].as_str().unwrap();
    assert!(failure_message.contains("LuaLS stdout closed unexpectedly"));
    assert!(failure_message.contains("process exited with exit status: 1"));
    let restarted = receive_matching(&mut stdout, |message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("LuaLS restarted and resynchronized"))
    });
    assert_eq!(
        restarted["params"]["message"],
        "LuaLS restarted and resynchronized 1 open virtual documents"
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown",
            "params": null
        }),
    );
    receive_matching(&mut stdout, |message| message["id"] == 2);
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    );
    drop(stdin);

    assert!(child.wait().unwrap().success());
    fs::remove_dir_all(fixture_dir).unwrap();
}

#[test]
fn forwards_client_request_cancellation_to_lua_ls() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture_dir =
        env::temp_dir().join(format!("cea-luals-cancel-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&fixture_dir).unwrap();
    let fake_lua_ls = write_fake_lua_ls(&fixture_dir, FAKE_CANCELLATION_LUA_LS);

    let mut child = Command::new(env!("CARGO_BIN_EXE_cea-language-server"))
        .env("CEA_LUA_LANGUAGE_SERVER", &fake_lua_ls)
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
                    "uri": "file:///cancel-fixture.cea",
                    "languageId": "cea",
                    "version": 1,
                    "text": "{$lua}\nprint('cancel me')\n{$asm}\n"
                }
            }
        }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///cancel-fixture.cea" },
                "position": { "line": 1, "character": 2 }
            }
        }),
    );
    receive_matching(&mut stdout, |message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"] == "LuaLS: hover received"
    });

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "$/cancelRequest",
            "params": { "id": 5 }
        }),
    );
    receive_matching(&mut stdout, |message| {
        message["method"] == "window/logMessage"
            && message["params"]["message"] == "LuaLS: cancellation received for 10"
    });

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "shutdown",
            "params": null
        }),
    );
    receive_matching(&mut stdout, |message| message["id"] == 6);
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    );
    drop(stdin);

    assert!(child.wait().unwrap().success());
    fs::remove_dir_all(fixture_dir).unwrap();
}

fn command_exists(command: &str) -> bool {
    command_path(command).is_some()
}

fn command_path(command: &str) -> Option<std::path::PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| Path::new(&directory).join(command))
            .find(|candidate| candidate.is_file())
    })
}

fn write_fake_lua_ls(fixture_dir: &Path, body: &str) -> std::path::PathBuf {
    let bash = command_path("bash").expect("bash must be available to run the LuaLS fixture");
    let fake_lua_ls = fixture_dir.join("fake-lua-language-server");
    fs::write(&fake_lua_ls, format!("#!{}\n{body}", bash.display())).unwrap();
    fs::set_permissions(&fake_lua_ls, fs::Permissions::from_mode(0o755)).unwrap();
    fake_lua_ls
}

const FAKE_RESTARTING_LUA_LS: &str = r#"set -euo pipefail

state_file="$(dirname "$0")/launch-count"
launch_count=0
if [[ -f "$state_file" ]]; then
    launch_count="$(<"$state_file")"
fi
launch_count=$((launch_count + 1))
printf '%s' "$launch_count" >"$state_file"

send() {
    local response="$1"
    printf 'Content-Length: %s\r\n\r\n%s' "${#response}" "$response"
}

content_length=
while IFS= read -r header; do
    header="${header%$'\r'}"
    if [[ -n "$header" ]]; then
        if [[ "$header" == "Content-Length: "* ]]; then
            content_length="${header#Content-Length: }"
        fi
        continue
    fi

    IFS= read -r -N "$content_length" body
    content_length=
    if [[ "$body" == *'"method":"initialize"'* ]]; then
        send '{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
    elif [[ "$body" == *'"method":"textDocument/didOpen"'* && "$launch_count" -eq 1 ]]; then
        exit 1
    elif [[ "$body" == *'"method":"shutdown"'* ]]; then
        send '{"jsonrpc":"2.0","id":2,"result":null}'
    elif [[ "$body" == *'"method":"exit"'* ]]; then
        exit 0
    fi
done
"#;

const FAKE_CANCELLATION_LUA_LS: &str = r#"set -euo pipefail

send() {
    local response="$1"
    printf 'Content-Length: %s\r\n\r\n%s' "${#response}" "$response"
}

content_length=
while IFS= read -r header; do
    header="${header%$'\r'}"
    if [[ -n "$header" ]]; then
        if [[ "$header" == "Content-Length: "* ]]; then
            content_length="${header#Content-Length: }"
        fi
        continue
    fi

    IFS= read -r -N "$content_length" body
    content_length=
    if [[ "$body" == *'"method":"initialize"'* ]]; then
        send '{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true}}}'
    elif [[ "$body" == *'"method":"textDocument/hover"'* ]]; then
        printf 'hover received\n' >&2
    elif [[ "$body" == *'"method":"$/cancelRequest"'* ]]; then
        request_id="${body##*\"id\":}"
        request_id="${request_id%%\}*}"
        printf 'cancellation received for %s\n' "$request_id" >&2
    elif [[ "$body" == *'"method":"shutdown"'* ]]; then
        send '{"jsonrpc":"2.0","id":2,"result":null}'
    elif [[ "$body" == *'"method":"exit"'* ]]; then
        exit 0
    fi
done
"#;
