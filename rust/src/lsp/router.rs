use lsp_types::Uri;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use super::client::{file_path_to_uri, LspClient};
use super::config::{
    check_server_available, default_servers, language_for_extension, LspServerConfig,
};

static CLIENTS: std::sync::LazyLock<Mutex<HashMap<String, LspClient>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{rest}", home.display());
        }
    }
    path.to_string()
}

fn resolve_config_for_language(language: &str) -> LspServerConfig {
    let cfg = crate::core::config::Config::load();
    if let Some(custom_path) = cfg.lsp.get(language) {
        let expanded = expand_tilde(custom_path);
        return LspServerConfig {
            command: expanded,
            args: if language == "typescript" || language == "javascript" {
                vec!["--stdio".into()]
            } else if language == "go" {
                vec!["serve".into()]
            } else {
                vec![]
            },
        };
    }
    let servers = default_servers();
    servers.get(language).cloned().unwrap_or(LspServerConfig {
        command: format!("{language}-language-server"),
        args: vec![],
    })
}

pub fn with_client<F, R>(file_path: &str, project_root: &str, f: F) -> Result<R, String>
where
    F: FnOnce(&mut LspClient, &str) -> Result<R, String>,
{
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let language = language_for_extension(ext).ok_or_else(|| {
        format!(
            "No LSP server configured for extension '.{ext}'. Supported: rs, ts, tsx, js, py, go"
        )
    })?;

    let mut clients = CLIENTS.lock().map_err(|e| e.to_string())?;

    if !clients.contains_key(language) {
        let config = resolve_config_for_language(language);

        if super::config::find_binary_in_path(&config.command).is_none()
            && !Path::new(&config.command).is_file()
        {
            check_server_available(language)?;
        }

        let root_uri = file_path_to_uri(project_root)?;
        let client = LspClient::start(&config, &root_uri)?;
        clients.insert(language.to_string(), client);
    }

    let client = clients
        .get_mut(language)
        .ok_or_else(|| format!("LSP client for '{language}' not available"))?;

    f(client, language)
}

pub fn open_file(file_path: &str, project_root: &str) -> Result<Uri, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read '{file_path}': {e}"))?;

    let uri = file_path_to_uri(file_path)?;

    with_client(file_path, project_root, |client, language| {
        client.did_open(&uri, language, &content)?;
        Ok(uri.clone())
    })
}

pub fn shutdown_all() {
    if let Ok(mut clients) = CLIENTS.lock() {
        for (_, client) in clients.drain() {
            drop(client);
        }
    }
}
