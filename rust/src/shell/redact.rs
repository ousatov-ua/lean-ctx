macro_rules! static_regex {
    ($pattern:expr) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| {
            regex::Regex::new($pattern).expect(concat!("BUG: invalid static regex: ", $pattern))
        })
    }};
}

fn mask_sensitive_data(input: &str) -> String {
    let patterns: Vec<(&str, &regex::Regex)> = vec![
        (
            "Bearer token",
            static_regex!(r"(?i)(bearer\s+)[a-zA-Z0-9\-_\.]{8,}"),
        ),
        (
            "Authorization header",
            static_regex!(r"(?i)(authorization:\s*(?:basic|bearer|token)\s+)[^\s\r\n]+"),
        ),
        (
            "API key param",
            static_regex!(
                r#"(?i)((?:api[_-]?key|apikey|access[_-]?key|secret[_-]?key|token|password|passwd|pwd|secret)\s*[=:]\s*)[^\s\r\n,;&"']+"#
            ),
        ),
        ("AWS key", static_regex!(r"(AKIA[0-9A-Z]{12,})")),
        (
            "Private key block",
            static_regex!(
                r"(?s)(-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----).+?(-----END\s+(?:RSA\s+)?PRIVATE\s+KEY-----)"
            ),
        ),
        (
            "GitHub token",
            static_regex!(r"(gh[pousr]_)[a-zA-Z0-9]{20,}"),
        ),
        (
            "Generic long hex/base64 secret",
            static_regex!(
                r#"(?i)(?:key|token|secret|password|credential|auth)\s*[=:]\s*['"]?([a-zA-Z0-9+/=\-_]{32,})['"]?"#
            ),
        ),
    ];

    let mut result = input.to_string();
    for (label, re) in &patterns {
        result = re
            .replace_all(&result, |caps: &regex::Captures| {
                if let Some(prefix) = caps.get(1) {
                    format!("{}[REDACTED:{}]", prefix.as_str(), label)
                } else {
                    format!("[REDACTED:{label}]")
                }
            })
            .to_string();
    }
    result
}

pub fn save_tee(command: &str, output: &str) -> Option<String> {
    let tee_dir = crate::core::paths::state_dir().ok()?.join("tee");
    std::fs::create_dir_all(&tee_dir).ok()?;

    cleanup_old_tee_logs(&tee_dir);

    let cmd_slug: String = command
        .chars()
        .take(40)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Content-addressed path (#498): the same command always maps to the same
    // file, so repeated tool outputs stay byte-identical (provider prompt
    // caches reward stable text). Re-runs overwrite — newest output wins;
    // the 24h TTL cleanup works on mtime, not the filename.
    let cmd_hash = blake3::hash(command.as_bytes()).to_hex();
    let filename = format!("{cmd_slug}_{}.log", &cmd_hash.as_str()[..8]);
    let path = tee_dir.join(&filename);

    let masked = mask_sensitive_data(output);
    let (redacted, _) = crate::core::secret_detection::scan_and_redact_from_config(&masked);
    std::fs::write(&path, redacted).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Some(path.to_string_lossy().to_string())
}

fn cleanup_old_tee_logs(tee_dir: &std::path::Path) {
    let cutoff = std::time::SystemTime::now().checked_sub(std::time::Duration::from_hours(24));
    let Some(cutoff) = cutoff else { return };

    if let Ok(entries) = std::fs::read_dir(tee_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Determinism contract (#498): the tee path must be content-addressed —
    /// the same command always maps to the same file so repeated tool outputs
    /// stay byte-identical for provider prompt caching.
    #[test]
    fn tee_path_is_content_addressed() {
        let first = save_tee("cargo test --lib", "output run 1").expect("tee saved");
        let second = save_tee("cargo test --lib", "output run 2").expect("tee saved");
        assert_eq!(first, second, "same command must map to the same tee path");

        let other = save_tee("cargo build", "output").expect("tee saved");
        assert_ne!(first, other, "different commands get different tee paths");

        // Latest output wins on overwrite.
        let content = std::fs::read_to_string(&second).unwrap();
        assert!(content.contains("run 2"));

        for p in [first, other] {
            let _ = std::fs::remove_file(p);
        }
    }
}
