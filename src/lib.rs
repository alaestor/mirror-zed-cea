use zed_extension_api as zed;

const LANGUAGE_SERVER_NAME: &str = "cea-language-server";

struct CeaExtension;

impl zed::Extension for CeaExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let command = worktree.which(LANGUAGE_SERVER_NAME).ok_or_else(|| {
            format!(
                "{LANGUAGE_SERVER_NAME} was not found on PATH for language server {language_server_id}"
            )
        })?;

        Ok(zed::Command {
            command,
            args: Vec::new(),
            env: worktree.shell_env(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<Option<zed::serde_json::Value>> {
        Ok(
            zed::settings::LspSettings::for_worktree(LANGUAGE_SERVER_NAME, worktree)?
                .initialization_options,
        )
    }
}

zed::register_extension!(CeaExtension);
