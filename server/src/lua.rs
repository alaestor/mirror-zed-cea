use std::{
    collections::{HashMap, HashSet},
    env,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use serde_json::{json, Value};
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
        BufReader,
    },
    process::{Child, Command},
    sync::{mpsc, oneshot, RwLock},
    time::{sleep, timeout, Duration},
};
use tower_lsp::{
    lsp_types::{
        ConfigurationParams, MessageType, Position, PublishDiagnosticsParams, Range, Url,
        WorkspaceFolder, WorkspaceFoldersChangeEvent,
    },
    Client,
};

use crate::{diagnostics::DiagnosticPublisher, document::LuaVirtualDocument};

const INITIALIZE_REQUEST_ID: u64 = 1;
const FIRST_PROXY_REQUEST_ID: u64 = 10;
const DEFAULT_COMMAND: &str = "lua-language-server";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Clone)]
struct ProxyContext {
    documents: Arc<RwLock<HashMap<Url, ProxyDocument>>>,
    workspace_folders: Arc<RwLock<Vec<WorkspaceFolder>>>,
    pending: PendingRequests,
    diagnostics: DiagnosticPublisher,
    client: Client,
}

#[derive(Clone)]
struct ProxyDocument {
    source_uri: Url,
    version: i32,
    source: String,
    ranges: Vec<Range>,
}

enum SupervisorCommand {
    Message(Value),
    Shutdown(oneshot::Sender<()>),
}

struct PendingRequestGuard {
    request_id: u64,
    pending: PendingRequests,
    sender: mpsc::UnboundedSender<SupervisorCommand>,
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        if lock_pending(&self.pending)
            .remove(&self.request_id)
            .is_some()
        {
            let _ = self
                .sender
                .send(SupervisorCommand::Message(cancel_request_message(
                    self.request_id,
                )));
        }
    }
}

pub struct LuaProxy {
    sender: mpsc::UnboundedSender<SupervisorCommand>,
    documents: Arc<RwLock<HashMap<Url, ProxyDocument>>>,
    diagnostics: DiagnosticPublisher,
    pending: PendingRequests,
    next_request_id: AtomicU64,
}

impl LuaProxy {
    pub async fn start(
        client: Client,
        diagnostics: DiagnosticPublisher,
        workspace_folders: Arc<RwLock<Vec<WorkspaceFolder>>>,
    ) -> Result<Self, String> {
        let command =
            env::var("CEA_LUA_LANGUAGE_SERVER").unwrap_or_else(|_| DEFAULT_COMMAND.to_owned());
        let documents = Arc::new(RwLock::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let context = ProxyContext {
            documents: documents.clone(),
            workspace_folders,
            pending: pending.clone(),
            diagnostics: diagnostics.clone(),
            client,
        };
        let (sender, receiver) = mpsc::unbounded_channel();
        let (started_sender, started_receiver) = oneshot::channel();
        tokio::spawn(supervise(
            command.clone(),
            receiver,
            context,
            started_sender,
        ));
        timeout(Duration::from_secs(10), started_receiver)
            .await
            .map_err(|_| format!("timed out initializing {command}"))?
            .map_err(|_| format!("{command} stopped during initialization"))??;

        Ok(Self {
            sender,
            documents,
            diagnostics,
            pending,
            next_request_id: AtomicU64::new(FIRST_PROXY_REQUEST_ID),
        })
    }

    pub async fn open(&self, source_uri: Url, version: i32, virtual_document: LuaVirtualDocument) {
        let virtual_uri = lua_document_uri(&source_uri);
        self.documents.write().await.insert(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                version,
                source: virtual_document.source.clone(),
                ranges: virtual_document.ranges,
            },
        );
        self.diagnostics.set_lua(source_uri, Vec::new()).await;
        self.send(json!({
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
        let virtual_uri = lua_document_uri(&source_uri);
        self.documents.write().await.insert(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                version,
                source: virtual_document.source.clone(),
                ranges: virtual_document.ranges,
            },
        );
        self.diagnostics.set_lua(source_uri, Vec::new()).await;
        self.send(json!({
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
        let virtual_uri = lua_document_uri(source_uri);
        self.documents.write().await.remove(&virtual_uri);
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": { "uri": virtual_uri }
            }
        }));
    }

    pub async fn change_workspace_folders(&self, event: WorkspaceFoldersChangeEvent) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWorkspaceFolders",
            "params": { "event": event }
        }));
    }

    pub async fn request(
        &self,
        method: &str,
        source_uri: &Url,
        position: Option<Position>,
        mut params: Value,
    ) -> Result<Option<Value>, String> {
        let virtual_uri = lua_document_uri(source_uri);
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
        lock_pending(&self.pending).insert(request_id, response_sender);
        let _request_guard = PendingRequestGuard {
            request_id,
            pending: self.pending.clone(),
            sender: self.sender.clone(),
        };
        if self
            .sender
            .send(SupervisorCommand::Message(json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params
            })))
            .is_err()
        {
            return Err("LuaLS stopped before receiving a request".into());
        }

        let request_context = request_context(method, request_id, source_uri);
        let response = match timeout(REQUEST_TIMEOUT, response_receiver).await {
            Err(_) => {
                return Err(format!(
                    "{request_context} timed out after {}s",
                    REQUEST_TIMEOUT.as_secs()
                ));
            }
            Ok(Err(_)) => {
                return Err(format!("LuaLS stopped before completing {request_context}"));
            }
            Ok(Ok(Err(error))) => {
                return Err(format!("{request_context} failed: {error}"));
            }
            Ok(Ok(Ok(response))) => response,
        };
        let documents = self.documents.read().await;
        Ok(Some(translate_response_uris(response, &documents)))
    }

    pub async fn shutdown(&self) {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        if self
            .sender
            .send(SupervisorCommand::Shutdown(shutdown_sender))
            .is_ok()
        {
            let _ = shutdown_receiver.await;
        }
    }

    fn send(&self, message: Value) {
        let _ = self.sender.send(SupervisorCommand::Message(message));
    }
}

fn request_context(method: &str, request_id: u64, source_uri: &Url) -> String {
    format!("LuaLS {method} request {request_id} for {source_uri}")
}

fn lock_pending(
    pending: &PendingRequests,
) -> MutexGuard<'_, HashMap<u64, oneshot::Sender<Result<Value, String>>>> {
    pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn cancel_request_message(request_id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "$/cancelRequest",
        "params": { "id": request_id }
    })
}

struct LuaProcess {
    sender: mpsc::UnboundedSender<Value>,
    child: Child,
    failures: mpsc::UnboundedReceiver<String>,
}

async fn supervise(
    command: String,
    mut receiver: mpsc::UnboundedReceiver<SupervisorCommand>,
    context: ProxyContext,
    started_sender: oneshot::Sender<Result<(), String>>,
) {
    let mut started_sender = Some(started_sender);
    let mut backlog = Vec::new();
    loop {
        let process = start_process(&command, context.clone()).await;
        let mut process = match process {
            Ok(process) => process,
            Err(error) => {
                if let Some(started_sender) = started_sender.take() {
                    let _ = started_sender.send(Err(error));
                    return;
                }
                context
                    .client
                    .log_message(
                        MessageType::ERROR,
                        format!("failed to restart LuaLS: {error}; retrying"),
                    )
                    .await;
                sleep(Duration::from_secs(1)).await;
                while let Ok(command) = receiver.try_recv() {
                    match command {
                        SupervisorCommand::Message(message) => backlog.push(message),
                        SupervisorCommand::Shutdown(shutdown_sender) => {
                            let _ = shutdown_sender.send(());
                            return;
                        }
                    }
                }
                if receiver.is_closed() {
                    return;
                }
                continue;
            }
        };

        let open_count = replay_open_documents(&process.sender, &context.documents).await;
        for message in backlog.drain(..) {
            if process.sender.send(message).is_err() {
                break;
            }
        }
        if let Some(started_sender) = started_sender.take() {
            let _ = started_sender.send(Ok(()));
        } else {
            context
                .client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "LuaLS restarted and resynchronized {open_count} open virtual documents"
                    ),
                )
                .await;
        }

        let failure = loop {
            tokio::select! {
                command = receiver.recv() => match command {
                    Some(SupervisorCommand::Message(message)) => {
                        if process.sender.send(message).is_err() {
                            break "LuaLS protocol input channel closed".to_owned();
                        }
                    }
                    Some(SupervisorCommand::Shutdown(shutdown_sender)) => {
                        shutdown_process(&process.sender, &mut process.child).await;
                        let _ = shutdown_sender.send(());
                        return;
                    }
                    None => {
                        let _ = process.child.kill().await;
                        return;
                    }
                },
                failure = process.failures.recv() => {
                    break failure.unwrap_or_else(|| "LuaLS process monitors stopped".to_owned());
                },
            }
        };

        let exit = match timeout(Duration::from_secs(2), process.child.wait()).await {
            Ok(Ok(status)) => format!("; process exited with {status}"),
            Ok(Err(error)) => format!("; failed to wait for process: {error}"),
            Err(_) => {
                let _ = process.child.kill().await;
                "; process did not exit after 2s and was killed".to_owned()
            }
        };
        context
            .client
            .log_message(
                MessageType::WARNING,
                format!("{failure}{exit}; restarting LuaLS"),
            )
            .await;
    }
}

async fn start_process(command: &str, context: ProxyContext) -> Result<LuaProcess, String> {
    let mut child = Command::new(command)
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
    let (failure_sender, mut failures) = mpsc::unbounded_channel();
    let writer_failure_sender = failure_sender.clone();
    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(message) = receiver.recv().await {
            if let Err(error) = write_message(&mut stdin, &message).await {
                let _ = writer_failure_sender.send(format!("LuaLS protocol write error: {error}"));
                return;
            }
        }
    });

    let (initialized_sender, initialized_receiver) = oneshot::channel();
    tokio::spawn(read_messages(
        stdout,
        sender.clone(),
        context.clone(),
        initialized_sender,
        failure_sender,
    ));
    tokio::spawn(log_stderr(stderr, context.client));

    let workspace_folders = context.workspace_folders.read().await;
    sender
        .send(initialize_message(&workspace_folders))
        .map_err(|_| format!("{command} stopped before initialization"))?;
    drop(workspace_folders);
    tokio::select! {
        initialized = timeout(Duration::from_secs(10), initialized_receiver) => {
            initialized
                .map_err(|_| format!("timed out initializing {command} after 10s"))?
                .map_err(|_| format!("{command} stopped during initialization"))??;
        }
        failure = failures.recv() => {
            return Err(failure.unwrap_or_else(|| {
                format!("{command} process monitors stopped during initialization")
            }));
        }
    }
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

    Ok(LuaProcess {
        sender,
        child,
        failures,
    })
}

async fn replay_open_documents(
    sender: &mpsc::UnboundedSender<Value>,
    documents: &RwLock<HashMap<Url, ProxyDocument>>,
) -> usize {
    let documents = documents.read().await;
    for (virtual_uri, document) in documents.iter() {
        let _ = sender.send(did_open_message(virtual_uri, document));
    }
    documents.len()
}

fn did_open_message(virtual_uri: &Url, document: &ProxyDocument) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": virtual_uri,
                "languageId": "lua",
                "version": document.version,
                "text": document.source
            }
        }
    })
}

async fn shutdown_process(sender: &mpsc::UnboundedSender<Value>, child: &mut Child) {
    let _ = sender.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": null
    }));
    let _ = sender.send(json!({
        "jsonrpc": "2.0",
        "method": "exit"
    }));
    if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
}

fn initialize_message(workspace_folders: &[WorkspaceFolder]) -> Value {
    let root_uri = workspace_folders.first().map(|folder| &folder.uri);
    json!({
        "jsonrpc": "2.0",
        "id": INITIALIZE_REQUEST_ID,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": root_uri,
            "workspaceFolders": workspace_folders,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                },
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true
                }
            }
        }
    })
}

fn client_request_response(
    message: &Value,
    workspace_folders: &[WorkspaceFolder],
    configuration: &Value,
) -> Value {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let result = match method {
        "workspace/configuration" => configuration_response(
            message.get("params").cloned().unwrap_or(Value::Null),
            configuration,
        ),
        "workspace/workspaceFolders" => Ok(json!(workspace_folders)),
        "workspace/applyEdit" => Ok(json!({
            "applied": false,
            "failureReason": "LuaLS workspace edits are disabled to protect virtual CEA documents"
        })),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create" => Ok(Value::Null),
        _ => Err((
            -32601,
            format!("unsupported LuaLS client request: {method}"),
        )),
    };

    match result {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message
            }
        }),
    }
}

fn configuration_response(params: Value, configuration: &Value) -> Result<Value, (i32, String)> {
    let params = serde_json::from_value::<ConfigurationParams>(params).map_err(|error| {
        (
            -32602,
            format!("invalid workspace/configuration params: {error}"),
        )
    })?;
    Ok(Value::Array(
        params
            .items
            .into_iter()
            .map(|item| {
                item.section
                    .as_deref()
                    .map(|section| configuration_section(configuration, section))
                    .unwrap_or_else(|| configuration.clone())
            })
            .collect(),
    ))
}

fn configuration_section(configuration: &Value, section: &str) -> Value {
    if section.is_empty() {
        return configuration.clone();
    }
    section
        .split('.')
        .try_fold(configuration, |value, key| value.get(key))
        .cloned()
        .unwrap_or(Value::Null)
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

fn lua_document_uri(source_uri: &Url) -> Url {
    // Some LuaLS releases omit semantic diagnostics for nonexistent file URIs.
    // This child server can safely overlay virtual Lua text on the real CEA URI.
    let mut uri = source_uri.clone();
    uri.set_query(None);
    uri.set_fragment(None);
    uri
}

async fn read_messages(
    stdout: impl AsyncRead + Unpin,
    sender: mpsc::UnboundedSender<Value>,
    context: ProxyContext,
    initialized_sender: oneshot::Sender<Result<(), String>>,
    failure_sender: mpsc::UnboundedSender<String>,
) {
    let mut reader = BufReader::new(stdout);
    let mut initialized_sender = Some(initialized_sender);
    let mut failure = "LuaLS stdout closed unexpectedly".to_owned();
    loop {
        let message = match read_message(&mut reader).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(error) => {
                failure = format!("LuaLS protocol read error: {error}");
                context
                    .client
                    .log_message(MessageType::ERROR, failure.clone())
                    .await;
                break;
            }
        };

        if initialized_sender.is_some()
            && message.get("method").is_none()
            && message.get("id") == Some(&json!(INITIALIZE_REQUEST_ID))
        {
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
                if let Some(response_sender) = lock_pending(&context.pending).remove(&id) {
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
                        let documents = context.documents.read().await;
                        if let Some(document) = documents.get(&params.uri).cloned() {
                            let diagnostics =
                                translate_diagnostic_uris(params.diagnostics, &documents);
                            let filtered = filter_lua_diagnostics(diagnostics, &document.ranges);
                            drop(documents);
                            context
                                .diagnostics
                                .set_lua(document.source_uri, filtered)
                                .await;
                        }
                    }
                    Err(error) => {
                        context
                            .client
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
            let workspace_folders = context.workspace_folders.read().await;
            let response = client_request_response(
                &message,
                &workspace_folders,
                &lua_configuration().unwrap_or_else(|| json!({})),
            );
            drop(workspace_folders);
            if let Some(error) = response.pointer("/error/message").and_then(Value::as_str) {
                context
                    .client
                    .log_message(MessageType::WARNING, error.to_owned())
                    .await;
            }
            let _ = sender.send(response);
        }
    }

    if let Some(initialized_sender) = initialized_sender {
        let _ = initialized_sender.send(Err(failure.clone()));
    }
    for (_, response_sender) in lock_pending(&context.pending).drain() {
        let _ = response_sender.send(Err(failure.clone()));
    }
    let _ = failure_sender.send(failure);
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

fn translate_diagnostic_uris(
    mut diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>,
    documents: &HashMap<Url, ProxyDocument>,
) -> Vec<tower_lsp::lsp_types::Diagnostic> {
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

fn translate_response_uris(value: Value, documents: &HashMap<Url, ProxyDocument>) -> Value {
    translate_uri_fields(value, None, documents)
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
    fn uses_source_uri_for_virtual_document() {
        let source = Url::parse("file:///project/scripts/player.cea").unwrap();

        assert_eq!(
            lua_document_uri(&source).as_str(),
            "file:///project/scripts/player.cea"
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
    fn initializes_lua_ls_with_every_workspace_folder() {
        let folders = [
            WorkspaceFolder {
                uri: Url::parse("file:///project/one").unwrap(),
                name: "one".into(),
            },
            WorkspaceFolder {
                uri: Url::parse("file:///project/two").unwrap(),
                name: "two".into(),
            },
        ];

        let message = initialize_message(&folders);

        assert_eq!(message["params"]["rootUri"], folders[0].uri.as_str());
        assert_eq!(message["params"]["workspaceFolders"], json!(folders));
        assert_eq!(
            message["params"]["capabilities"]["workspace"]["configuration"],
            true
        );
    }

    #[test]
    fn answers_lua_ls_configuration_requests_by_section() {
        let message = json!({
            "jsonrpc": "2.0",
            "id": "configuration",
            "method": "workspace/configuration",
            "params": {
                "items": [
                    { "section": "Lua.runtime" },
                    { "section": "Lua.workspace.library" },
                    { "section": "Lua.missing" }
                ]
            }
        });
        let configuration = json!({
            "Lua": {
                "runtime": { "path": ["?.lua"] },
                "workspace": { "library": ["/opt/lua"] }
            }
        });

        let response = client_request_response(&message, &[], &configuration);

        assert_eq!(
            response["result"],
            json!([
                { "path": ["?.lua"] },
                ["/opt/lua"],
                null
            ])
        );
    }

    #[test]
    fn acknowledges_dynamic_registration_and_rejects_unknown_client_requests() {
        let registration = client_request_response(
            &json!({
                "id": 8,
                "method": "client/registerCapability",
                "params": { "registrations": [] }
            }),
            &[],
            &json!({}),
        );
        let unknown = client_request_response(
            &json!({
                "id": 9,
                "method": "workspace/unknown",
                "params": {}
            }),
            &[],
            &json!({}),
        );

        assert_eq!(registration["result"], Value::Null);
        assert_eq!(unknown["error"]["code"], -32601);
        assert_eq!(
            unknown["error"]["message"],
            "unsupported LuaLS client request: workspace/unknown"
        );
    }

    #[test]
    fn rejects_lua_ls_workspace_edits() {
        let response = client_request_response(
            &json!({
                "id": 10,
                "method": "workspace/applyEdit",
                "params": {
                    "edit": {
                        "changes": {
                            "file:///project/player.cea": []
                        }
                    }
                }
            }),
            &[],
            &json!({}),
        );

        assert_eq!(response["result"]["applied"], false);
        assert_eq!(
            response["result"]["failureReason"],
            "LuaLS workspace edits are disabled to protect virtual CEA documents"
        );
    }

    #[test]
    fn request_errors_include_method_id_and_source_uri() {
        let source_uri = Url::parse("file:///project/player.cea").unwrap();

        assert_eq!(
            request_context("textDocument/hover", 17, &source_uri),
            "LuaLS textDocument/hover request 17 for file:///project/player.cea"
        );
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
                version: 1,
                source: String::new(),
                ranges: Vec::new(),
            },
        )]);
        let response = json!({
            "originSelectionRange": null,
            "targetUri": virtual_uri,
            "related": [{ "uri": real_lua_uri }],
            "changes": {
                virtual_uri.as_str(): [{ "newText": virtual_uri.as_str() }]
            }
        });

        let translated = translate_response_uris(response, &documents);

        assert_eq!(translated["targetUri"], source_uri.as_str());
        assert_eq!(
            translated["related"][0]["uri"],
            "file:///project/helpers.lua"
        );
        assert!(translated["changes"]
            .get(source_uri.as_str())
            .is_some_and(|changes| changes[0]["newText"] == virtual_uri.as_str()));
    }

    #[test]
    fn translates_diagnostic_related_information_and_data_uris() {
        let virtual_uri = Url::parse("file:///project/player.cea.lua").unwrap();
        let source_uri = Url::parse("file:///project/player.cea").unwrap();
        let documents = HashMap::from([(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: source_uri.clone(),
                version: 1,
                source: String::new(),
                ranges: Vec::new(),
            },
        )]);
        let mut diagnostic =
            tower_lsp::lsp_types::Diagnostic::new_simple(Range::default(), "related".into());
        diagnostic.related_information =
            Some(vec![tower_lsp::lsp_types::DiagnosticRelatedInformation {
                location: tower_lsp::lsp_types::Location::new(
                    virtual_uri.clone(),
                    Range::default(),
                ),
                message: "declaration".into(),
            }]);
        diagnostic.data = Some(json!({
            "targetUri": virtual_uri,
            "label": "file:///project/player.cea.lua"
        }));

        let translated = translate_diagnostic_uris(vec![diagnostic], &documents);

        assert_eq!(
            translated[0].related_information.as_ref().unwrap()[0]
                .location
                .uri,
            source_uri
        );
        assert_eq!(
            translated[0].data.as_ref().unwrap()["targetUri"],
            source_uri.as_str()
        );
        assert_eq!(
            translated[0].data.as_ref().unwrap()["label"],
            "file:///project/player.cea.lua"
        );
    }

    #[tokio::test]
    async fn replays_latest_open_document_state() {
        let virtual_uri = Url::parse("file:///project/player.cea").unwrap();
        let documents = RwLock::new(HashMap::from([(
            virtual_uri.clone(),
            ProxyDocument {
                source_uri: virtual_uri.clone(),
                version: 7,
                source: "      \nprint('latest')\n".into(),
                ranges: Vec::new(),
            },
        )]));
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let open_count = replay_open_documents(&sender, &documents).await;
        let message = receiver.recv().await.unwrap();

        assert_eq!(open_count, 1);
        assert_eq!(message["method"], "textDocument/didOpen");
        assert_eq!(
            message["params"]["textDocument"]["uri"],
            virtual_uri.as_str()
        );
        assert_eq!(message["params"]["textDocument"]["languageId"], "lua");
        assert_eq!(message["params"]["textDocument"]["version"], 7);
        assert_eq!(
            message["params"]["textDocument"]["text"],
            "      \nprint('latest')\n"
        );
    }

    #[test]
    fn cancels_and_discards_an_abandoned_pending_request() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (response_sender, _response_receiver) = oneshot::channel();
        lock_pending(&pending).insert(42, response_sender);
        let (sender, mut receiver) = mpsc::unbounded_channel();

        drop(PendingRequestGuard {
            request_id: 42,
            pending: pending.clone(),
            sender,
        });

        assert!(lock_pending(&pending).is_empty());
        let SupervisorCommand::Message(message) = receiver.try_recv().unwrap() else {
            panic!("abandoning a request must forward a cancellation notification");
        };
        assert_eq!(message, cancel_request_message(42));
    }

    #[test]
    fn does_not_cancel_a_request_that_already_completed() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (sender, mut receiver) = mpsc::unbounded_channel();

        drop(PendingRequestGuard {
            request_id: 42,
            pending,
            sender,
        });

        assert!(receiver.try_recv().is_err());
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

    #[tokio::test]
    async fn reports_missing_protocol_content_length() {
        let input = b"Content-Type: application/vscode-jsonrpc\r\n\r\n{}";
        let mut reader = BufReader::new(&input[..]);

        let error = read_message(&mut reader).await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "missing Content-Length");
    }
}
