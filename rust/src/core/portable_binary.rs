pub fn resolve_portable_binary() -> String {
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = std::process::Command::new(which_cmd)
        .arg("lean-ctx")
        .stderr(std::process::Stdio::null())
        .output()
    {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !raw.is_empty() {
                let path = pick_best_binary_line(&raw);
                return sanitize_exe_path(&path);
            }
        }
    }
    let path = std::env::current_exe().map_or_else(
        |_| "lean-ctx".to_string(),
        |p| p.to_string_lossy().to_string(),
    );
    sanitize_exe_path(&path)
}

/// On Windows, `where lean-ctx` returns multiple lines (e.g. `lean-ctx` and
/// `lean-ctx.cmd`). Pick the `.cmd`/`.exe` variant if available, otherwise
/// the first line.
fn pick_best_binary_line(raw: &str) -> String {
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() <= 1 {
        return lines.first().unwrap_or(&"lean-ctx").to_string();
    }
    if cfg!(windows) {
        if let Some(cmd) = lines.iter().find(|l| {
            std::path::Path::new(*l).extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("exe")
            })
        }) {
            return cmd.to_string();
        }
    }
    lines[0].to_string()
}

fn sanitize_exe_path(path: &str) -> String {
    let cleaned = path.trim_end_matches(" (deleted)");
    if cfg!(windows) {
        super::pathutil::normalize_tool_path(cleaned)
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_returns_as_is() {
        assert_eq!(
            pick_best_binary_line("/usr/bin/lean-ctx"),
            "/usr/bin/lean-ctx"
        );
    }

    #[test]
    fn multiline_returns_first_line() {
        let raw = "/usr/bin/lean-ctx\n/usr/local/bin/lean-ctx";
        let result = pick_best_binary_line(raw);
        assert_eq!(result, "/usr/bin/lean-ctx");
    }

    #[test]
    fn empty_returns_fallback() {
        assert_eq!(pick_best_binary_line(""), "lean-ctx");
    }

    #[test]
    fn sanitize_removes_deleted_suffix() {
        assert_eq!(
            sanitize_exe_path("/usr/bin/lean-ctx (deleted)"),
            "/usr/bin/lean-ctx"
        );
    }

    #[test]
    fn whitespace_lines_are_filtered() {
        let raw = "  /usr/bin/lean-ctx  \n  \n  /usr/local/bin/lean-ctx  ";
        assert_eq!(pick_best_binary_line(raw), "/usr/bin/lean-ctx");
    }

    #[cfg(windows)]
    #[test]
    fn sanitize_normalizes_msys_path_on_windows() {
        assert_eq!(
            sanitize_exe_path("/c/Users/ABC/.local/bin/lean-ctx"),
            "C:/Users/ABC/.local/bin/lean-ctx"
        );
    }

    #[cfg(windows)]
    #[test]
    fn sanitize_keeps_native_windows_path() {
        assert_eq!(
            sanitize_exe_path(r"C:\Users\ABC\lean-ctx.exe"),
            "C:/Users/ABC/lean-ctx.exe"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn sanitize_unix_path_unchanged() {
        assert_eq!(
            sanitize_exe_path("/usr/local/bin/lean-ctx"),
            "/usr/local/bin/lean-ctx"
        );
    }

    #[test]
    fn resolve_portable_binary_is_absolute() {
        // #367: generated hook commands must use an absolute binary path, never
        // a bare `lean-ctx`, because agents run hooks under non-login shells
        // without the install dir on PATH. `which`/`current_exe()` both yield
        // an absolute path in any normal environment (incl. the test harness).
        let resolved = resolve_portable_binary();
        assert!(
            std::path::Path::new(&resolved).is_absolute(),
            "resolve_portable_binary must return an absolute path, got: {resolved}"
        );
    }
}
