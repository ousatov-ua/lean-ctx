//! Read-dedup control — whether the PostToolUse hook replaces a native Read's
//! *result* with the compact re-read stub (GL #1140, follow-up to GH #637).
//!
//! On guard hosts (Claude Code / CodeBuddy) `read_redirect = auto` keeps the
//! PreToolUse path-swap off so the native read-before-write guard stays intact —
//! at the cost of the Read dedup savings. `PostToolUse.updatedToolOutput` restores
//! them guard-safely: the native Read has already run on the *real* path (guard
//! satisfied, first read byte-identical), and only the model-visible result of a
//! **re-read of an unchanged file** is replaced by the stub.

use serde::{Deserialize, Serialize};

use super::Config;

/// Controls the PostToolUse native-Read re-read dedup.
///
/// - `Auto`: (Default) dedup only on hosts with a read-before-write guard
///   (Claude Code / CodeBuddy) — exactly where the PreToolUse redirect is off and
///   the savings would otherwise be lost. Elsewhere the PreToolUse redirect
///   already dedups re-reads, so the PostToolUse hook stays passive.
/// - `On`: dedup wherever the hook fires.
/// - `Off`: never replace a Read result.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReadDedup {
    #[default]
    Auto,
    On,
    Off,
}

impl ReadDedup {
    /// Parse `LEAN_CTX_READ_DEDUP`. Accepts the canonical `auto|on|off` plus the
    /// usual boolean spellings, mirroring `LEAN_CTX_READ_REDIRECT`.
    pub fn from_env() -> Option<Self> {
        std::env::var("LEAN_CTX_READ_DEDUP").ok().and_then(|v| {
            match v.trim().to_lowercase().as_str() {
                "auto" => Some(Self::Auto),
                "on" | "1" | "true" | "yes" => Some(Self::On),
                "off" | "0" | "false" | "no" => Some(Self::Off),
                _ => None,
            }
        })
    }

    /// Env override (`LEAN_CTX_READ_DEDUP`) wins over the on-disk config value.
    pub fn effective(config: &Config) -> Self {
        Self::from_env().unwrap_or(config.read_dedup)
    }

    /// Whether the PostToolUse read-dedup may replace a re-read result in the
    /// current process/host. `Auto` restricts it to guard hosts, where the
    /// PreToolUse redirect is disabled and re-reads would otherwise flow at
    /// full size (#637 / GL #1140).
    pub fn read_dedup_enabled(config: &Config) -> bool {
        match Self::effective(config) {
            Self::On => true,
            Self::Off => false,
            Self::Auto => super::read_redirect::host_has_read_before_write_guard(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auto() {
        assert_eq!(ReadDedup::default(), ReadDedup::Auto);
    }

    #[test]
    fn serde_roundtrip_kebab() {
        #[derive(Deserialize)]
        struct Wrapper {
            read_dedup: ReadDedup,
        }
        for (raw, want) in [
            ("auto", ReadDedup::Auto),
            ("on", ReadDedup::On),
            ("off", ReadDedup::Off),
        ] {
            let w: Wrapper = toml::from_str(&format!("read_dedup = \"{raw}\"")).expect("parse");
            assert_eq!(w.read_dedup, want, "{raw}");
        }
    }

    #[test]
    fn from_env_parses_canonical_and_boolean_spellings() {
        let _lock = crate::core::data_dir::test_env_lock();

        crate::test_env::set_var("LEAN_CTX_READ_DEDUP", "auto");
        assert_eq!(ReadDedup::from_env(), Some(ReadDedup::Auto));
        crate::test_env::set_var("LEAN_CTX_READ_DEDUP", "ON");
        assert_eq!(ReadDedup::from_env(), Some(ReadDedup::On));
        crate::test_env::set_var("LEAN_CTX_READ_DEDUP", " off ");
        assert_eq!(ReadDedup::from_env(), Some(ReadDedup::Off));
        crate::test_env::set_var("LEAN_CTX_READ_DEDUP", "0");
        assert_eq!(ReadDedup::from_env(), Some(ReadDedup::Off));
        crate::test_env::set_var("LEAN_CTX_READ_DEDUP", "nonsense");
        assert_eq!(ReadDedup::from_env(), None);
        crate::test_env::remove_var("LEAN_CTX_READ_DEDUP");
        assert_eq!(ReadDedup::from_env(), None);
    }

    #[test]
    fn auto_enables_only_on_guard_hosts() {
        // Inverse of read_redirect's auto: dedup where the redirect is off.
        let _lock = crate::core::data_dir::test_env_lock();
        crate::test_env::remove_var("LEAN_CTX_READ_DEDUP");
        crate::test_env::remove_var("CLAUDE_PROJECT_DIR");
        crate::test_env::remove_var("CLAUDECODE");
        crate::test_env::remove_var("CODEBUDDY");

        let cfg = Config::default(); // Auto
        assert!(
            !ReadDedup::read_dedup_enabled(&cfg),
            "auto must stay passive off guard hosts (PreToolUse redirect dedups there)"
        );

        crate::test_env::set_var("CLAUDE_PROJECT_DIR", "/repo");
        assert!(
            ReadDedup::read_dedup_enabled(&cfg),
            "auto must dedup under Claude Code hooks (CLAUDE_PROJECT_DIR)"
        );
        crate::test_env::remove_var("CLAUDE_PROJECT_DIR");

        crate::test_env::set_var("CODEBUDDY", "1");
        assert!(
            ReadDedup::read_dedup_enabled(&cfg),
            "auto must dedup under CodeBuddy (shared guard contract)"
        );
        crate::test_env::remove_var("CODEBUDDY");
    }

    #[test]
    fn on_and_off_are_absolute() {
        let _lock = crate::core::data_dir::test_env_lock();
        crate::test_env::remove_var("LEAN_CTX_READ_DEDUP");
        crate::test_env::remove_var("CLAUDE_PROJECT_DIR");
        crate::test_env::remove_var("CLAUDECODE");
        crate::test_env::remove_var("CODEBUDDY");

        let cfg_on = Config {
            read_dedup: ReadDedup::On,
            ..Config::default()
        };
        assert!(ReadDedup::read_dedup_enabled(&cfg_on));

        let cfg_off = Config {
            read_dedup: ReadDedup::Off,
            ..Config::default()
        };
        crate::test_env::set_var("CLAUDE_PROJECT_DIR", "/repo");
        assert!(!ReadDedup::read_dedup_enabled(&cfg_off));
        crate::test_env::remove_var("CLAUDE_PROJECT_DIR");
    }
}
