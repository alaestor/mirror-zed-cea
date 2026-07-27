use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tower_lsp::{
    lsp_types::{Diagnostic, Url},
    Client,
};

#[derive(Clone)]
pub struct DiagnosticPublisher {
    inner: Arc<Inner>,
}

struct Inner {
    client: Client,
    cea: RwLock<HashMap<Url, Vec<Diagnostic>>>,
    lua: RwLock<HashMap<Url, Vec<Diagnostic>>>,
}

impl DiagnosticPublisher {
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(Inner {
                client,
                cea: RwLock::new(HashMap::new()),
                lua: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub async fn set_cea(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.inner
            .cea
            .write()
            .await
            .insert(uri.clone(), diagnostics);
        self.publish(uri).await;
    }

    pub async fn set_lua(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.inner
            .lua
            .write()
            .await
            .insert(uri.clone(), diagnostics);
        self.publish(uri).await;
    }

    pub async fn clear(&self, uri: Url) {
        self.inner.cea.write().await.remove(&uri);
        self.inner.lua.write().await.remove(&uri);
        self.inner
            .client
            .publish_diagnostics(uri, Vec::new(), None)
            .await;
    }

    async fn publish(&self, uri: Url) {
        let mut diagnostics = self
            .inner
            .cea
            .read()
            .await
            .get(&uri)
            .cloned()
            .unwrap_or_default();
        diagnostics.extend(
            self.inner
                .lua
                .read()
                .await
                .get(&uri)
                .cloned()
                .unwrap_or_default(),
        );
        self.inner
            .client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}
