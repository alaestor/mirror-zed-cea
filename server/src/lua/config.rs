use serde::Deserialize;
use serde_json::Value;

use crate::cea_api::{self, CheatEngineApiConfig};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaConfig {
    pub(super) path: Option<String>,
    pub(super) runtime_version: Option<String>,
    #[serde(default)]
    pub(super) runtime_path: Vec<String>,
    #[serde(default)]
    pub(super) workspace_library: Vec<String>,
    #[serde(skip)]
    pub(super) cheat_engine_api: CheatEngineApiConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CeaOptions {
    #[serde(default)]
    lua_language_server: LuaConfig,
    #[serde(default)]
    cheat_engine_api: CheatEngineApiConfig,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum InitializationOptions {
    Nested { cea: CeaOptions },
    Direct(CeaOptions),
}

impl LuaConfig {
    pub fn from_initialization_options(options: Option<Value>) -> Result<Self, String> {
        let Some(options) = options else {
            return Ok(Self::default());
        };
        let options = serde_json::from_value::<InitializationOptions>(options)
            .map_err(|error| format!("invalid CEA initialization options: {error}"))?;
        let options = match options {
            InitializationOptions::Direct(options) => options,
            InitializationOptions::Nested { cea } => cea,
        };
        if options.cheat_engine_api.enabled
            && !cea_api::supported_versions().contains(&options.cheat_engine_api.version.as_str())
        {
            return Err(format!(
                "unsupported Cheat Engine API version {:?}; supported versions: {}",
                options.cheat_engine_api.version,
                cea_api::supported_versions().join(", ")
            ));
        }
        Ok(Self {
            cheat_engine_api: options.cheat_engine_api,
            ..options.lua_language_server
        })
    }
}
