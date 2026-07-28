use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
        DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
        Hover, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
        InitializedParams, Location, MessageType, OneOf, ReferenceParams, ServerCapabilities,
        ServerInfo, SignatureHelp, SignatureHelpOptions, SignatureHelpParams,
        TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceFolder,
        WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
    },
    Client, LanguageServer,
};

use crate::document::Document;
use crate::{diagnostics::DiagnosticPublisher, lua::LuaProxy};

struct OpenDocument {
    document: Document,
    version: i32,
}

pub struct Backend {
    client: Client,
    diagnostics: DiagnosticPublisher,
    documents: RwLock<HashMap<Url, OpenDocument>>,
    lua: RwLock<Option<LuaProxy>>,
    workspace_folders: Arc<RwLock<Vec<WorkspaceFolder>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let diagnostics = DiagnosticPublisher::new(client.clone());
        Self {
            client,
            diagnostics,
            documents: RwLock::new(HashMap::new()),
            lua: RwLock::new(None),
            workspace_folders: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn update_document(&self, uri: Url, text: String, version: i32, open: bool) {
        match Document::parse(text) {
            Ok(document) => {
                let diagnostics = document.diagnostics();
                let virtual_document = document.lua_virtual_document();
                self.documents
                    .write()
                    .await
                    .insert(uri.clone(), OpenDocument { document, version });
                self.diagnostics.set_cea(uri.clone(), diagnostics).await;
                if let Some(lua) = self.lua.read().await.as_ref() {
                    if open {
                        lua.open(uri, version, virtual_document).await;
                    } else {
                        lua.change(uri, version, virtual_document).await;
                    }
                }
            }
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("failed to initialize CEA parser: {error}"),
                    )
                    .await;
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        #[allow(deprecated)]
        let workspace_folders = params.workspace_folders.unwrap_or_else(|| {
            params
                .root_uri
                .map(|uri| {
                    vec![WorkspaceFolder {
                        name: uri
                            .path_segments()
                            .and_then(Iterator::last)
                            .filter(|name| !name.is_empty())
                            .unwrap_or("CEA workspace")
                            .to_owned(),
                        uri,
                    }]
                })
                .unwrap_or_default()
        });
        *self.workspace_folders.write().await = workspace_folders;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(
                        [".", ":", "'", "\"", "/", "@", "*", "#"]
                            .into_iter()
                            .map(str::to_owned)
                            .collect(),
                    ),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(["(", ","].into_iter().map(str::to_owned).collect()),
                    retrigger_characters: Some([")"].into_iter().map(str::to_owned).collect()),
                    ..SignatureHelpOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    ..WorkspaceServerCapabilities::default()
                }),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "CEA Language Server".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "CEA language server initialized")
            .await;
        match LuaProxy::start(
            self.client.clone(),
            self.diagnostics.clone(),
            self.workspace_folders.clone(),
        )
        .await
        {
            Ok(proxy) => {
                let mut lua = self.lua.write().await;
                *lua = Some(proxy);
                if let Some(proxy) = lua.as_ref() {
                    for (uri, document) in self.documents.read().await.iter() {
                        proxy
                            .open(
                                uri.clone(),
                                document.version,
                                document.document.lua_virtual_document(),
                            )
                            .await;
                    }
                }
                self.client
                    .log_message(MessageType::INFO, "Lua language server proxy initialized")
                    .await;
            }
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("Lua language features are unavailable: {error}"),
                    )
                    .await;
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(lua) = self.lua.read().await.as_ref() {
            lua.shutdown().await;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.update_document(document.uri, document.text, document.version, true)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(
                params.text_document.uri,
                change.text,
                params.text_document.version,
                false,
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(lua) = self.lua.read().await.as_ref() {
            lua.close(&params.text_document.uri).await;
        }
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.diagnostics.clear(params.text_document.uri).await;
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        {
            let mut workspace_folders = self.workspace_folders.write().await;
            workspace_folders.retain(|folder| {
                !params
                    .event
                    .removed
                    .iter()
                    .any(|removed| removed.uri == folder.uri)
            });
            for added in &params.event.added {
                if !workspace_folders
                    .iter()
                    .any(|folder| folder.uri == added.uri)
                {
                    workspace_folders.push(added.clone());
                }
            }
        }
        if let Some(lua) = self.lua.read().await.as_ref() {
            lua.change_workspace_folders(params.event).await;
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let documents = self.documents.read().await;
        Ok(documents
            .get(&params.text_document.uri)
            .map(|document| DocumentSymbolResponse::Nested(document.document.symbols())))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let source_uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        self.lua_request(
            "textDocument/completion",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request("textDocument/hover", &source_uri, Some(position), params)
            .await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/signatureHelp",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/definition",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let source_uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        self.lua_request(
            "textDocument/references",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }
}

impl Backend {
    async fn lua_request<P, R>(
        &self,
        method: &str,
        source_uri: &Url,
        position: Option<tower_lsp::lsp_types::Position>,
        params: P,
    ) -> Result<Option<R>>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let lua = self.lua.read().await;
        let Some(lua) = lua.as_ref() else {
            return Ok(None);
        };
        let params = serde_json::to_value(params)
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
        let response = match lua.request(method, source_uri, position, params).await {
            Ok(response) => response,
            Err(error) => {
                self.client.log_message(MessageType::ERROR, error).await;
                return Err(tower_lsp::jsonrpc::Error::internal_error());
            }
        };
        response
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())
    }
}
