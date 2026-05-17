use std::path::{Path, PathBuf};

pub fn zed_settings_path(home: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Zed/settings.json")
    } else {
        home.join(".config/zed/settings.json")
    }
}

pub fn zed_config_dir(home: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Zed")
    } else {
        home.join(".config/zed")
    }
}

pub fn vscode_mcp_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "macos")]
        {
            return home.join("Library/Application Support/Code/User/mcp.json");
        }
        #[cfg(target_os = "linux")]
        {
            return home.join(".config/Code/User/mcp.json");
        }
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata).join("Code/User/mcp.json");
            }
        }
        #[allow(unreachable_code)]
        home.join(".config/Code/User/mcp.json")
    } else {
        PathBuf::from("/nonexistent")
    }
}

pub fn qoder_mcp_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("Qoder")
                .join("SharedClientCache")
                .join("mcp.json");
        }
    }
    home.join(".qoder").join("mcp.json")
}

#[cfg(target_os = "macos")]
pub fn qoder_mcp_paths(home: &Path) -> Vec<PathBuf> {
    let mut paths = vec![qoder_mcp_path(home)];
    paths.push(home.join("Library/Application Support/Qoder/User/mcp.json"));
    paths.push(home.join("Library/Application Support/Qoder/SharedClientCache/mcp.json"));
    paths
}

#[cfg(not(target_os = "macos"))]
pub fn qoder_mcp_paths(home: &Path) -> Vec<PathBuf> {
    vec![qoder_mcp_path(home)]
}

#[allow(unreachable_code)]
pub fn cline_mcp_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join(
                "Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            );
        }
        return PathBuf::from("/nonexistent");
    }

    let Some(home) = dirs::home_dir() else {
        return PathBuf::from("/nonexistent");
    };
    #[cfg(target_os = "macos")]
    {
        return home.join("Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json");
    }
    #[cfg(target_os = "linux")]
    {
        return home.join(".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json");
    }
    PathBuf::from("/nonexistent")
}

#[allow(unreachable_code)]
pub fn roo_mcp_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json");
        }
        return PathBuf::from("/nonexistent");
    }

    let Some(home) = dirs::home_dir() else {
        return PathBuf::from("/nonexistent");
    };
    #[cfg(target_os = "macos")]
    {
        return home.join("Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json");
    }
    #[cfg(target_os = "linux")]
    {
        return home.join(".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json");
    }
    PathBuf::from("/nonexistent")
}

pub fn qoder_settings_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("Qoder")
                .join("SharedClientCache")
                .join("mcp.json");
        }
    }
    home.join(".qoder/mcp.json")
}

pub fn qoder_all_mcp_paths(home: &Path) -> Vec<PathBuf> {
    let paths = vec![qoder_settings_path(home)];
    #[cfg(target_os = "macos")]
    let paths = {
        let mut paths = paths;
        paths.push(home.join("Library/Application Support/Qoder/User/mcp.json"));
        paths.push(home.join("Library/Application Support/Qoder/SharedClientCache/mcp.json"));
        paths
    };
    paths
}

pub fn qoderwork_mcp_path(home: &Path) -> PathBuf {
    home.join(".qoderwork/mcp.json")
}

pub fn claude_mcp_json_path(home: &Path) -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir).join(".claude.json");
        }
    }
    home.join(".claude.json")
}

pub fn claude_state_dir(home: &Path) -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    home.join(".claude")
}

pub fn claude_rules_dir(home: &Path) -> PathBuf {
    claude_state_dir(home).join("rules")
}

pub fn claude_config_display_prefix() -> String {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = dir.trim().to_string();
        if !dir.is_empty() {
            return dir;
        }
    }
    "~/.claude".to_string()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn qoder_mcp_paths_include_macos_user_and_shared_cache_locations() {
        let home = Path::new("/Users/tester");
        let paths = qoder_mcp_paths(home);

        assert_eq!(
            paths,
            vec![
                home.join(".qoder/mcp.json"),
                home.join("Library/Application Support/Qoder/User/mcp.json"),
                home.join("Library/Application Support/Qoder/SharedClientCache/mcp.json"),
            ]
        );
    }
}
