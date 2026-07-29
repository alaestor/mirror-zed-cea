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
        CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
        DeclarationCapability, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
        DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentHighlight, DocumentHighlightParams, DocumentSymbolParams, DocumentSymbolResponse,
        FileChangeType, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
        HoverParams, HoverProviderCapability, ImplementationProviderCapability, InitializeParams,
        InitializeResult, InitializedParams, InlayHint, InlayHintParams, Location, MarkupContent,
        MarkupKind, MessageType, OneOf, PrepareRenameResponse, ReferenceParams, RenameOptions,
        RenameParams, ServerCapabilities, ServerInfo, SignatureHelp, SignatureHelpOptions,
        SignatureHelpParams, TextDocumentPositionParams, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextEdit, TypeDefinitionProviderCapability, Url,
        WorkDoneProgressOptions, WorkspaceEdit, WorkspaceFolder,
        WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
    },
    Client, LanguageServer,
};

use crate::document::Document;
use crate::{
    diagnostics::DiagnosticPublisher,
    lua::{LuaConfig, LuaProxy},
    symbol_index::{CeaSymbolKind, OccurrenceRole, WorkspaceSymbolIndex},
    workspace,
};

struct OpenDocument {
    document: Document,
    version: i32,
}

pub struct Backend {
    client: Client,
    diagnostics: DiagnosticPublisher,
    documents: RwLock<HashMap<Url, OpenDocument>>,
    disk_documents: RwLock<HashMap<Url, Document>>,
    symbols: RwLock<WorkspaceSymbolIndex>,
    lua: RwLock<Option<LuaProxy>>,
    workspace_folders: Arc<RwLock<Vec<WorkspaceFolder>>>,
    lua_config: RwLock<LuaConfig>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let diagnostics = DiagnosticPublisher::new(client.clone());
        Self {
            client,
            diagnostics,
            documents: RwLock::new(HashMap::new()),
            disk_documents: RwLock::new(HashMap::new()),
            symbols: RwLock::new(WorkspaceSymbolIndex::default()),
            lua: RwLock::new(None),
            workspace_folders: Arc::new(RwLock::new(Vec::new())),
            lua_config: RwLock::new(LuaConfig::default()),
        }
    }

    async fn update_document(&self, uri: Url, text: String, version: i32, open: bool) {
        match Document::parse(text) {
            Ok(document) => {
                let virtual_document = document.lua_virtual_document();
                let symbol_index = document.symbol_index();
                self.documents
                    .write()
                    .await
                    .insert(uri.clone(), OpenDocument { document, version });
                self.symbols.write().await.update(uri.clone(), symbol_index);
                self.refresh_cea_diagnostics().await;
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

    async fn refresh_cea_diagnostics(&self) {
        let mut semantic = self.symbols.read().await.semantic_diagnostics();
        let mut diagnostics: Vec<_> = self
            .documents
            .read()
            .await
            .iter()
            .map(|(uri, document)| {
                let mut diagnostics = document.document.diagnostics();
                diagnostics.extend(semantic.remove(uri).unwrap_or_default());
                (uri.clone(), diagnostics)
            })
            .collect();
        let open_uris: std::collections::HashSet<_> =
            diagnostics.iter().map(|(uri, _)| uri.clone()).collect();
        diagnostics.extend(
            self.disk_documents
                .read()
                .await
                .iter()
                .filter(|(uri, _)| !open_uris.contains(*uri))
                .map(|(uri, document)| {
                    let mut diagnostics = document.diagnostics();
                    diagnostics.extend(semantic.remove(uri).unwrap_or_default());
                    (uri.clone(), diagnostics)
                }),
        );
        for (uri, diagnostics) in diagnostics {
            self.diagnostics.set_cea(uri, diagnostics).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let lua_config =
            LuaConfig::from_initialization_options(params.initialization_options.clone())
                .map_err(tower_lsp::jsonrpc::Error::invalid_params)?;
        *self.lua_config.write().await = lua_config;
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
        let discovered = workspace::discover(&self.workspace_folders.read().await);
        {
            let mut symbols = self.symbols.write().await;
            for (uri, document) in &discovered {
                symbols.update(uri.clone(), document.symbol_index());
            }
        }
        *self.disk_documents.write().await = discovered;

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
        self.refresh_cea_diagnostics().await;
        match LuaProxy::start(
            self.client.clone(),
            self.diagnostics.clone(),
            self.workspace_folders.clone(),
            self.lua_config.read().await.clone(),
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
                let message = format!(
                    "Lua language features are unavailable; native CEA features remain active: {error}"
                );
                self.client
                    .show_message(MessageType::WARNING, message)
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
        let folders = self.workspace_folders.read().await;
        let disk_document = workspace::belongs_to(&params.text_document.uri, &folders)
            .then(|| workspace::load(&params.text_document.uri))
            .flatten();
        drop(folders);
        if let Some(document) = disk_document {
            self.symbols
                .write()
                .await
                .update(params.text_document.uri.clone(), document.symbol_index());
            self.disk_documents
                .write()
                .await
                .insert(params.text_document.uri.clone(), document);
        } else {
            self.symbols.write().await.remove(&params.text_document.uri);
            self.disk_documents
                .write()
                .await
                .remove(&params.text_document.uri);
        }
        self.refresh_cea_diagnostics().await;
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
        let folders = self.workspace_folders.read().await.clone();
        let discovered = workspace::discover(&folders);
        let open = self.documents.read().await;
        let mut disk_documents = self.disk_documents.write().await;
        let mut symbols = self.symbols.write().await;
        let previous_uris: Vec<_> = disk_documents.keys().cloned().collect();
        for uri in previous_uris {
            if !workspace::belongs_to(&uri, &folders) {
                disk_documents.remove(&uri);
                if !open.contains_key(&uri) {
                    symbols.remove(&uri);
                }
            }
        }
        for (uri, document) in discovered {
            if !open.contains_key(&uri) {
                symbols.update(uri.clone(), document.symbol_index());
            }
            disk_documents.insert(uri, document);
        }
        drop(symbols);
        drop(disk_documents);
        drop(open);
        self.refresh_cea_diagnostics().await;
        if let Some(lua) = self.lua.read().await.as_ref() {
            lua.change_workspace_folders(params.event).await;
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let open = self.documents.read().await;
        let mut disk_documents = self.disk_documents.write().await;
        let mut symbols = self.symbols.write().await;
        for change in params.changes {
            if open.contains_key(&change.uri) {
                continue;
            }
            if change.typ == FileChangeType::DELETED {
                disk_documents.remove(&change.uri);
                symbols.remove(&change.uri);
            } else if let Some(document) = workspace::load(&change.uri) {
                symbols.update(change.uri.clone(), document.symbol_index());
                disk_documents.insert(change.uri, document);
            }
        }
        drop(symbols);
        drop(disk_documents);
        drop(open);
        self.refresh_cea_diagnostics().await;
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
        let native_items = {
            let documents = self.documents.read().await;
            let allowed = documents
                .get(&source_uri)
                .is_some_and(|document| document.document.native_completion_allowed(position));
            drop(documents);
            if allowed {
                self.native_completions().await
            } else {
                Vec::new()
            }
        };
        let response = self
            .lua_request(
                "textDocument/completion",
                &source_uri,
                Some(position),
                params,
            )
            .await?;
        let mut response = response.unwrap_or_else(|| CompletionResponse::Array(Vec::new()));
        {
            let items = match &mut response {
                CompletionResponse::Array(items) => items,
                CompletionResponse::List(list) => &mut list.items,
            };
            for item in items.iter_mut() {
                item.data = Some(serde_json::json!({
                    "ceaSourceUri": source_uri,
                    "luaData": item.data.take(),
                }));
            }
            items.extend(native_items);
        }
        Ok(Some(response))
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
        let native = self
            .documents
            .read()
            .await
            .get(&source_uri)
            .and_then(|document| document.document.integer_hover(position));
        if let Some((value, range)) = native {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value,
                }),
                range: Some(range),
            }));
        }
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
        if let Some(locations) = self.native_definitions(&source_uri, position).await {
            return Ok(Some(GotoDefinitionResponse::Array(locations)));
        }
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
        if let Some(locations) = self.native_definitions(&source_uri, position).await {
            return Ok(Some(GotoDeclarationResponse::Array(locations)));
        }
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
        if let Some(locations) = self
            .native_references(&source_uri, position, params.context.include_declaration)
            .await
        {
            return Ok(Some(locations));
        }
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
        if let Some(highlights) = self.native_highlights(&source_uri, position).await {
            return Ok(Some(highlights));
        }
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
        if let Some(occurrence) = self
            .symbols
            .read()
            .await
            .occurrence_at(&source_uri, position)
            .cloned()
        {
            return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: occurrence.range,
                placeholder: occurrence.name,
            }));
        }
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
        if let Some(edit) = self
            .native_rename(&source_uri, position, &params.new_name)
            .await
        {
            return Ok(Some(edit));
        }
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
    async fn native_completions(&self) -> Vec<CompletionItem> {
        let index = self.symbols.read().await;
        index
            .symbol_names()
            .into_iter()
            .map(|name| {
                let kind = index
                    .declarations_named(&name)
                    .first()
                    .map(|indexed| completion_kind(indexed.occurrence.kind));
                CompletionItem {
                    label: name.clone(),
                    kind,
                    detail: Some("CEA symbol".into()),
                    sort_text: Some(format!("0_{name}")),
                    ..CompletionItem::default()
                }
            })
            .collect()
    }

    async fn native_definitions(
        &self,
        source_uri: &Url,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<Vec<Location>> {
        let index = self.symbols.read().await;
        let name = index.occurrence_at(source_uri, position)?.name.clone();
        let locations: Vec<_> = index
            .declarations_named(&name)
            .into_iter()
            .map(|indexed| Location::new(indexed.uri, indexed.occurrence.range))
            .collect();
        (!locations.is_empty()).then_some(locations)
    }

    async fn native_references(
        &self,
        source_uri: &Url,
        position: tower_lsp::lsp_types::Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let index = self.symbols.read().await;
        let name = index.occurrence_at(source_uri, position)?.name.clone();
        let locations = index
            .occurrences_named(&name)
            .into_iter()
            .filter(|indexed| {
                include_declaration
                    || !matches!(
                        indexed.occurrence.role,
                        OccurrenceRole::Declaration | OccurrenceRole::Definition
                    )
            })
            .map(|indexed| Location::new(indexed.uri, indexed.occurrence.range))
            .collect();
        Some(locations)
    }

    async fn native_highlights(
        &self,
        source_uri: &Url,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<Vec<DocumentHighlight>> {
        let index = self.symbols.read().await;
        let name = index.occurrence_at(source_uri, position)?.name.clone();
        Some(
            index
                .occurrences_named(&name)
                .into_iter()
                .filter(|indexed| indexed.uri == *source_uri)
                .map(|indexed| DocumentHighlight {
                    range: indexed.occurrence.range,
                    kind: None,
                })
                .collect(),
        )
    }

    async fn native_rename(
        &self,
        source_uri: &Url,
        position: tower_lsp::lsp_types::Position,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        if !valid_symbol_name(new_name) {
            return None;
        }
        let index = self.symbols.read().await;
        let name = index.occurrence_at(source_uri, position)?.name.clone();
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for indexed in index.occurrences_named(&name) {
            changes.entry(indexed.uri).or_default().push(TextEdit {
                range: indexed.occurrence.range,
                new_text: new_name.to_owned(),
            });
        }
        Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        })
    }

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

fn completion_kind(kind: CeaSymbolKind) -> CompletionItemKind {
    match kind {
        CeaSymbolKind::Definition => CompletionItemKind::CONSTANT,
        CeaSymbolKind::Label => CompletionItemKind::REFERENCE,
        CeaSymbolKind::Allocation | CeaSymbolKind::Registered => CompletionItemKind::VARIABLE,
    }
}

fn valid_symbol_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || "_?.$".contains(character))
        && characters
            .all(|character| character.is_ascii_alphanumeric() || "_.$?".contains(character))
}
