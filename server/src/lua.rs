use std::{
    collections::{HashMap, HashSet},
    env,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use serde_json::{json, Value};
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
        BufReader,
    },
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex, RwLock},
    time::{timeout, Duration},
};
use tower_lsp::{
    lsp_types::{MessageType, Position, PublishDiagnosticsParams, Range, Url},
    Client,
};

use crate::{diagnostics::DiagnosticPublisher, document::LuaVirtualDocument};

const INITIALIZE_REQUEST_ID: u64 = 1;
const FIRST_PROXY_REQUEST_ID: u64 = 10;
const DEFAULT_COMMAND: &str = "lua-language-server";
type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Clone)]
struct ProxyDocument {
    source_uri: Url,
    ranges: Vec<Range>,
}

pub struct LuaProxy {
    sender: mpsc::UnboundedSender<Value>,
    documents: Arc<RwLock<HashMap<Url, ProxyDocument>>>,
    child: Mutex<Child>,
    diagnostics: DiagnosticPublisher,
    pending: PendingRequests,
    next_request_id: AtomicU64,
}

impl LuaProxy {
    pub async fn start(
        client: Client,
        diagnostics: DiagnosticPublisher,
        workspace_uri: Option<Url>,
    ) -> Result<Self, String> {
        let command =
            env::var("CEA_LUA_LANGUAGE_SERVER").unwrap_or_else(|_| DEFAULT_COMMAND.to_owned());
        let mut child = Command::new(&command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("failed to start {command}: {error}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("{command} did not provide stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("{command} did not provide stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("{command} did not provide stderr"))?;

        let (sender, mut receiver) = mpsc::unbounded_channel::<Value>();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(message) = receiver.recv().await {
                if write_message(&mut stdin, &message).await.is_err() {
                    break;
                }
            }
        });

        let documents = Arc::new(RwLock::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (initialized_sender, initialized_receiver) = oneshot::channel();
        tokio::spawn(read_messages(
            stdout,
            sender.clone(),
            documents.clone(),
            pending.clone(),
            diagnostics.clone(),
            client.clone(),
            initialized_sender,
        ));
        tokio::spawn(log_stderr(stderr, client.clone()));

        sender
            .send(initialize_message(workspace_uri.as_ref()))
            .map_err(|_| format!("{command} stopped before initialization"))?;
        timeout(Duration::from_secs(10), initialized_receiver)
            .await
            .map_err(|_| format!("timed out initializing {command}"))?
            .map_err(|_| format!("{command} stopped during initialization"))??;

        sender
            .send(json!({
                "jsonrpc": "2.0",
                "method": "initialized",
                "params": {}
            }))
            .map_err(|_| format!("{command} stopped after initialization"))?;
        if let Some(configuration) = lua_configuration() {
            sender
                .send(json!({
                    "jsonrpc": "2.0",
                    "method": "workspace/didChangeConfiguration",
                    "params": { "settings": configuration }
                }))
                .map_err(|_| format!("{command} stopped while configuring its workspace"))?;
        }

        Ok(Self {
            sender,
            documents,
            child: Mutex::new(child),
            diagnostics,
            pending,
            next_request_id: AtomicU64::new(FIRST_PROXY_REQUEST_ID),
        })
    }

    pub async fn open(&self, source_uri: Url, version: i32, virtual_document: LuaVirtualDocument) {
        let virtual_uri = virtual_uri(&source_uri);
        self.documents.write().await.insert(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                ranges: virtual_document.ranges,
            },
        );
        self.diagnostics.set_lua(source_uri, Vec::new()).await;
        let _ = self.sender.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": virtual_uri,
                    "languageId": "lua",
                    "version": version,
                    "text": virtual_document.source
                }
            }
        }));
    }

    pub async fn change(
        &self,
        source_uri: Url,
        version: i32,
        virtual_document: LuaVirtualDocument,
    ) {
        let virtual_uri = virtual_uri(&source_uri);
        self.documents.write().await.insert(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                ranges: virtual_document.ranges,
            },
        );
        self.diagnostics.set_lua(source_uri, Vec::new()).await;
        let _ = self.sender.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": virtual_uri,
                    "version": version
                },
                "contentChanges": [{ "text": virtual_document.source }]
            }
        }));
    }

    pub async fn close(&self, source_uri: &Url) {
        let virtual_uri = virtual_uri(source_uri);
        self.documents.write().await.remove(&virtual_uri);
        let _ = self.sender.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": { "uri": virtual_uri }
            }
        }));
    }

    pub async fn request(
        &self,
        method: &str,
        source_uri: &Url,
        position: Option<Position>,
        mut params: Value,
    ) -> Result<Option<Value>, String> {
        let virtual_uri = virtual_uri(source_uri);
        let documents = self.documents.read().await;
        let document = match documents.get(&virtual_uri) {
            Some(document) => document,
            None => return Ok(None),
        };
        if position.is_some_and(|position| {
            !document
                .ranges
                .iter()
                .any(|range| contains(range, position))
        }) {
            return Ok(None);
        }
        drop(documents);

        if let Some(uri) = params.pointer_mut("/textDocument/uri") {
            *uri = json!(virtual_uri);
        }

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (response_sender, response_receiver) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(request_id, response_sender);
        if self
            .sender
            .send(json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params
            }))
            .is_err()
        {
            self.pending.lock().await.remove(&request_id);
            return Err("LuaLS stopped before receiving a request".into());
        }

        let response = timeout(Duration::from_secs(10), response_receiver)
            .await
            .map_err(|_| format!("timed out waiting for LuaLS {method} response"))?
            .map_err(|_| format!("LuaLS stopped while handling {method}"))??;
        let documents = self.documents.read().await;
        Ok(Some(translate_response_uris(response, &documents)))
    }

    pub async fn shutdown(&self) {
        let _ = self.sender.send(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown",
            "params": null
        }));
        let _ = self.sender.send(json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }));
        let mut child = self.child.lock().await;
        if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
            let _ = child.kill().await;
        }
    }
}

fn initialize_message(workspace_uri: Option<&Url>) -> Value {
    let workspace_folders = workspace_uri.map(|uri| {
        vec![json!({
            "uri": uri,
            "name": "CEA workspace"
        })]
    });
    json!({
        "jsonrpc": "2.0",
        "id": INITIALIZE_REQUEST_ID,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": workspace_uri,
            "workspaceFolders": workspace_folders,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                },
                "workspace": {
                    "configuration": false,
                    "workspaceFolders": true
                }
            }
        }
    })
}

fn lua_configuration() -> Option<Value> {
    let lua_path = env::var("LUA_PATH").ok()?;
    let (runtime_paths, libraries) = lua_path_configuration(&lua_path);
    if runtime_paths.is_empty() && libraries.is_empty() {
        return None;
    }

    Some(json!({
        "Lua": {
            "runtime": { "path": runtime_paths },
            "workspace": { "library": libraries }
        }
    }))
}

fn lua_path_configuration(lua_path: &str) -> (Vec<String>, Vec<String>) {
    let runtime_paths: Vec<_> = lua_path
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    let mut seen = HashSet::new();
    let libraries = runtime_paths
        .iter()
        .filter_map(|pattern| pattern.split_once('?').map(|(prefix, _)| prefix))
        .map(|prefix| prefix.trim_end_matches(['/', '\\']))
        .filter(|prefix| !prefix.is_empty())
        .filter(|prefix| seen.insert((*prefix).to_owned()))
        .map(str::to_owned)
        .collect();
    (runtime_paths, libraries)
}

fn virtual_uri(source_uri: &Url) -> Url {
    let mut uri = source_uri.clone();
    uri.set_path(&format!("{}.lua", source_uri.path()));
    uri.set_query(None);
    uri.set_fragment(None);
    uri
}

async fn read_messages(
    stdout: impl AsyncRead + Unpin,
    sender: mpsc::UnboundedSender<Value>,
    documents: Arc<RwLock<HashMap<Url, ProxyDocument>>>,
    pending: PendingRequests,
    diagnostics: DiagnosticPublisher,
    client: Client,
    initialized_sender: oneshot::Sender<Result<(), String>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut initialized_sender = Some(initialized_sender);
    loop {
        let message = match read_message(&mut reader).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(error) => {
                client
                    .log_message(MessageType::ERROR, format!("LuaLS protocol error: {error}"))
                    .await;
                break;
            }
        };

        if message.get("id") == Some(&json!(INITIALIZE_REQUEST_ID)) {
            if let Some(initialized_sender) = initialized_sender.take() {
                let result = if let Some(error) = message.get("error") {
                    Err(format!("LuaLS initialization failed: {error}"))
                } else {
                    Ok(())
                };
                let _ = initialized_sender.send(result);
            }
            continue;
        }

        if message.get("method").is_none() {
            if let Some(id) = message.get("id").and_then(Value::as_u64) {
                if let Some(response_sender) = pending.lock().await.remove(&id) {
                    let result = if let Some(error) = message.get("error") {
                        Err(format!("LuaLS request failed: {error}"))
                    } else {
                        Ok(message.get("result").cloned().unwrap_or(Value::Null))
                    };
                    let _ = response_sender.send(result);
                }
            }
            continue;
        }

        if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
        {
            if let Some(params) = message.get("params").cloned() {
                match serde_json::from_value::<PublishDiagnosticsParams>(params) {
                    Ok(params) => {
                        if let Some(document) = documents.read().await.get(&params.uri).cloned() {
                            let filtered =
                                filter_lua_diagnostics(params.diagnostics, &document.ranges);
                            diagnostics.set_lua(document.source_uri, filtered).await;
                        }
                    }
                    Err(error) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("invalid LuaLS diagnostics: {error}"),
                            )
                            .await;
                    }
                }
            }
            continue;
        }

        if message.get("id").is_some() && message.get("method").is_some() {
            let _ = sender.send(json!({
                "jsonrpc": "2.0",
                "id": message["id"],
                "result": null
            }));
        }
    }

    if let Some(initialized_sender) = initialized_sender {
        let _ = initialized_sender.send(Err("LuaLS exited before initialization".into()));
    }
    for (_, response_sender) in pending.lock().await.drain() {
        let _ = response_sender.send(Err("LuaLS exited".into()));
    }
}

async fn log_stderr(stderr: impl AsyncRead + Unpin, client: Client) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        client
            .log_message(MessageType::LOG, format!("LuaLS: {line}"))
            .await;
    }
}

fn contains(range: &Range, position: Position) -> bool {
    position_tuple(range.start) <= position_tuple(position)
        && position_tuple(position) < position_tuple(range.end)
}

fn filter_lua_diagnostics(
    diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>,
    ranges: &[Range],
) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            ranges
                .iter()
                .any(|range| contains(range, diagnostic.range.start))
        })
        .collect()
}

fn position_tuple(position: Position) -> (u32, u32) {
    (position.line, position.character)
}

fn translate_response_uris(mut value: Value, documents: &HashMap<Url, ProxyDocument>) -> Value {
    match &mut value {
        Value::String(string) => {
            if let Ok(uri) = Url::parse(string) {
                if let Some(document) = documents.get(&uri) {
                    *string = document.source_uri.to_string();
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                *value = translate_response_uris(value.take(), documents);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                *value = translate_response_uris(value.take(), documents);
            }
        }
        _ => {}
    }
    value
}

async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &Value,
) -> std::io::Result<()> {
    let body = message.to_string();
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await
}

async fn read_message(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await? == 0 {
            return Ok(None);
        }
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length: ") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let length = content_length.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
    })?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_virtual_uri_beside_source_document() {
        let source = Url::parse("file:///project/scripts/player.cea").unwrap();

        assert_eq!(
            virtual_uri(&source).as_str(),
            "file:///project/scripts/player.cea.lua"
        );
    }

    #[test]
    fn translates_lua_path_into_runtime_patterns_and_unique_libraries() {
        let (runtime_paths, libraries) =
            lua_path_configuration("/opt/lua/?.lua;/opt/lua/?/init.lua;./?.lua;;");

        assert_eq!(
            runtime_paths,
            ["/opt/lua/?.lua", "/opt/lua/?/init.lua", "./?.lua"]
        );
        assert_eq!(libraries, ["/opt/lua", "."]);
    }

    #[test]
    fn range_contains_start_but_not_end() {
        let range = Range::new(Position::new(3, 2), Position::new(5, 0));

        assert!(contains(&range, Position::new(3, 2)));
        assert!(contains(&range, Position::new(4, 20)));
        assert!(!contains(&range, Position::new(5, 0)));
    }

    #[test]
    fn translates_only_virtual_document_uris_in_responses() {
        let virtual_uri = Url::parse("file:///project/player.cea.lua").unwrap();
        let source_uri = Url::parse("file:///project/player.cea").unwrap();
        let real_lua_uri = Url::parse("file:///project/helpers.lua").unwrap();
        let documents = HashMap::from([(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                ranges: Vec::new(),
            },
        )]);
        let response = json!({
            "originSelectionRange": null,
            "targetUri": virtual_uri,
            "related": [{ "uri": real_lua_uri }]
        });

        let translated = translate_response_uris(response, &documents);

        assert_eq!(translated["targetUri"], source_uri.as_str());
        assert_eq!(
            translated["related"][0]["uri"],
            "file:///project/helpers.lua"
        );
    }

    #[test]
    fn filters_diagnostics_outside_embedded_lua_ranges() {
        let ranges = [Range::new(Position::new(2, 0), Position::new(4, 0))];
        let inside = tower_lsp::lsp_types::Diagnostic::new_simple(
            Range::new(Position::new(3, 1), Position::new(3, 4)),
            "inside".into(),
        );
        let outside = tower_lsp::lsp_types::Diagnostic::new_simple(
            Range::new(Position::new(5, 0), Position::new(5, 3)),
            "outside".into(),
        );

        let filtered = filter_lua_diagnostics(vec![inside, outside], &ranges);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "inside");
    }
}
