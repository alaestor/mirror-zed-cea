use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tower_lsp::lsp_types::request::{
    GotoDeclarationParams, GotoDeclarationResponse, GotoImplementationParams,
    GotoImplementationResponse, GotoTypeDefinitionParams, GotoTypeDefinitionResponse,
};
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionItem,
        CompletionOptions, CompletionParams, CompletionResponse, DeclarationCapability,
        DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DocumentHighlight, DocumentHighlightParams,
        DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
        Hover, HoverParams, HoverProviderCapability, ImplementationProviderCapability,
        InitializeParams, InitializeResult, InitializedParams, InlayHint, InlayHintParams,
        Location, MessageType, OneOf, PrepareRenameResponse, ReferenceParams, RenameOptions,
        RenameParams, ServerCapabilities, ServerInfo, SignatureHelp, SignatureHelpOptions,
        SignatureHelpParams, TextDocumentPositionParams, TextDocumentSyncCapability,
        TextDocumentSyncKind, TypeDefinitionProviderCapability, Url, WorkDoneProgressOptions,
        WorkspaceEdit, WorkspaceFolder, WorkspaceFoldersServerCapabilities,
        WorkspaceServerCapabilities,
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
                    resolve_provider: Some(true),
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
                declaration_provider: Some(DeclarationCapability::Simple(true)),
                type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
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
        let response = self
            .lua_request(
                "textDocument/completion",
                &source_uri,
                Some(position),
                params,
            )
            .await?;
        Ok(response.map(|mut response| {
            let items = match &mut response {
                CompletionResponse::Array(items) => items,
                CompletionResponse::List(list) => &mut list.items,
            };
            for item in items {
                item.data = Some(serde_json::json!({
                    "ceaSourceUri": source_uri,
                    "luaData": item.data.take(),
                }));
            }
            response
        }))
    }

    async fn completion_resolve(&self, mut params: CompletionItem) -> Result<CompletionItem> {
        let Some(mut proxy_data) = params.data.take() else {
            return Ok(params);
        };
        let Some(source_uri) = proxy_data
            .get("ceaSourceUri")
            .and_then(serde_json::Value::as_str)
            .and_then(|uri| Url::parse(uri).ok())
        else {
            params.data = Some(proxy_data);
            return Ok(params);
        };
        params.data = proxy_data
            .get_mut("luaData")
            .map(serde_json::Value::take)
            .filter(|data| !data.is_null());
        let fallback = params.clone();
        Ok(self
            .lua_request("completionItem/resolve", &source_uri, None, params)
            .await?
            .unwrap_or(fallback))
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

    async fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/declaration",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn goto_type_definition(
        &self,
        params: GotoTypeDefinitionParams,
    ) -> Result<Option<GotoTypeDefinitionResponse>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/typeDefinition",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn goto_implementation(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/implementation",
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

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let source_uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        self.lua_request(
            "textDocument/documentHighlight",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let source_uri = params.text_document.uri.clone();
        let position = params.position;
        self.lua_request(
            "textDocument/prepareRename",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let source_uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        self.lua_request("textDocument/rename", &source_uri, Some(position), params)
            .await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let source_uri = params.text_document.uri.clone();
        let position = params.range.start;
        self.lua_request(
            "textDocument/codeAction",
            &source_uri,
            Some(position),
            params,
        )
        .await
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let source_uri = params.text_document.uri.clone();
        // LuaLS sees a position-preserving document with non-Lua text masked out,
        // so it can safely handle visible ranges that only partially overlap Lua.
        self.lua_request("textDocument/inlayHint", &source_uri, None, params)
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
