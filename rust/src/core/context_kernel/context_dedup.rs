//! Content deduplication for context delivered to language models.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

const DEFAULT_MAX_ENTRIES: usize = 1000;
const HASH_LENGTH: usize = 16;

/// Tracks the identity of content that was delivered to the LLM.
#[derive(Debug, Clone)]
pub struct ContentFingerprint {
    /// Blake3 content hash, truncated to 16 hexadecimal characters.
    pub hash: String,
    /// Approximate number of tokens in the delivered content.
    pub token_estimate: usize,
    /// Time at which this version of the content was delivered.
    pub delivered_at: Instant,
}

/// Whether content needs to be sent or was already delivered.
#[derive(Debug, Clone, PartialEq)]
pub enum DedupResult {
    /// Content is new or changed — must be sent.
    Fresh,
    /// Content is unchanged since last delivery — send a stub instead.
    Unchanged {
        /// Fingerprint of the content already in context.
        hash: String,
        /// Estimated tokens avoided by suppressing the duplicate.
        saved_tokens: usize,
    },
}

/// Bounded content deduplication tracker.
/// Remembers what was sent to the LLM to avoid resending unchanged content.
#[derive(Debug)]
pub struct ContextDedup {
    fingerprints: HashMap<String, ContentFingerprint>,
    max_entries: usize,
}

impl ContextDedup {
    /// Creates a tracker limited to `max_entries`; zero selects the default of 1000.
    pub fn new(max_entries: usize) -> Self {
        Self {
            fingerprints: HashMap::new(),
            max_entries: if max_entries == 0 {
                DEFAULT_MAX_ENTRIES
            } else {
                max_entries
            },
        }
    }

    /// Checks whether `content` for `path` changed and records fresh content.
    pub fn check_and_record(&mut self, path: &str, content: &str) -> DedupResult {
        let hash = content_hash(content);
        if let Some(fingerprint) = self.fingerprints.get(path)
            && fingerprint.hash == hash
        {
            return DedupResult::Unchanged {
                hash,
                saved_tokens: fingerprint.token_estimate,
            };
        }

        if !self.fingerprints.contains_key(path) && self.fingerprints.len() >= self.max_entries {
            self.evict_oldest();
        }
        self.fingerprints.insert(
            path.to_owned(),
            ContentFingerprint {
                hash,
                token_estimate: estimate_tokens(content),
                delivered_at: Instant::now(),
            },
        );
        DedupResult::Fresh
    }

    /// Forgets content previously recorded for `path`.
    pub fn invalidate(&mut self, path: &str) {
        self.fingerprints.remove(path);
    }

    /// Forgets every recorded content fingerprint.
    pub fn clear(&mut self) {
        self.fingerprints.clear();
    }

    /// Returns the number of tracked paths.
    pub fn len(&self) -> usize {
        self.fingerprints.len()
    }

    /// Returns whether the tracker contains no fingerprints.
    pub fn is_empty(&self) -> bool {
        self.fingerprints.is_empty()
    }

    /// Removes the least recently delivered fingerprint, if any.
    pub fn evict_oldest(&mut self) {
        let oldest = self
            .fingerprints
            .iter()
            .min_by_key(|(_, fingerprint)| fingerprint.delivered_at)
            .map(|(path, _)| path.clone());
        if let Some(path) = oldest {
            self.fingerprints.remove(&path);
        }
    }
}

/// Formats a compact reference to unchanged content already in context.
pub fn format_unchanged_stub(path: &str, hash: &str) -> String {
    let short_hash = &hash[..hash.len().min(8)];
    format!("→ {path} unchanged (ref:{short_hash}), already in context\n")
}

/// Replaces repeated Context Kernel blocks with compact reference stubs.
pub fn dedup_kernel_blocks(blocks: &str, seen_hashes: &mut HashSet<String>) -> String {
    let starts = kernel_block_starts(blocks);
    let Some(&first_start) = starts.first() else {
        return blocks.to_owned();
    };

    let mut output = String::with_capacity(blocks.len());
    output.push_str(&blocks[..first_start]);
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(blocks.len());
        let block = &blocks[start..end];
        let hash = content_hash(block.trim_end());
        if seen_hashes.insert(hash.clone()) {
            output.push_str(block);
        } else {
            output.push_str(&format_unchanged_stub("kernel context", &hash));
        }
    }
    output
}

/// Estimates token usage using an average of four UTF-8 bytes per token.
pub fn estimate_tokens(content: &str) -> usize {
    content.len() / 4
}

fn content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex()[..HASH_LENGTH].to_owned()
}

fn kernel_block_starts(blocks: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in blocks.split_inclusive('\n') {
        if matches!(
            line.trim_end_matches(['\r', '\n']),
            "## Context Kernel" | "--- kernel context ---"
        ) {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::{
        ContextDedup, DedupResult, content_hash, dedup_kernel_blocks, estimate_tokens,
        format_unchanged_stub,
    };
    use std::collections::HashSet;

    #[test]
    fn fresh_on_first_read() {
        let mut dedup = ContextDedup::new(1000);
        assert_eq!(
            dedup.check_and_record("src/lib.rs", "content"),
            DedupResult::Fresh
        );
    }

    #[test]
    fn unchanged_on_repeat_read() {
        let mut dedup = ContextDedup::new(1000);
        dedup.check_and_record("src/lib.rs", "eight888");
        assert_eq!(
            dedup.check_and_record("src/lib.rs", "eight888"),
            DedupResult::Unchanged {
                hash: content_hash("eight888"),
                saved_tokens: 2,
            }
        );
    }

    #[test]
    fn fresh_after_content_change() {
        let mut dedup = ContextDedup::new(1000);
        dedup.check_and_record("src/lib.rs", "before");
        assert_eq!(
            dedup.check_and_record("src/lib.rs", "after"),
            DedupResult::Fresh
        );
    }

    #[test]
    fn stub_format_is_short() {
        let stub = format_unchanged_stub("src/lib.rs", "0123456789abcdef");
        assert!(estimate_tokens(&stub) <= 20);
        assert!(stub.contains("ref:01234567"));
    }

    #[test]
    fn dedup_kernel_blocks_removes_duplicates() {
        let block = "## Context Kernel\nshared enrichment\n";
        let input = block.repeat(3);
        let mut seen = HashSet::new();
        let output = dedup_kernel_blocks(&input, &mut seen);
        assert_eq!(output.matches("shared enrichment").count(), 1);
        assert_eq!(output.matches("kernel context unchanged").count(), 2);
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn bounded_eviction() {
        let mut dedup = ContextDedup::new(1000);
        dedup.check_and_record("oldest", "first");
        for index in 0..1000 {
            dedup.check_and_record(&format!("path-{index}"), &format!("content-{index}"));
        }
        assert_eq!(dedup.len(), 1000);
        assert_eq!(
            dedup.check_and_record("oldest", "first"),
            DedupResult::Fresh
        );
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(content_hash("stable"), content_hash("stable"));
        assert_eq!(content_hash("stable").len(), 16);
    }

    #[test]
    fn invalidate_and_clear_forget_entries() {
        let mut dedup = ContextDedup::new(10);
        dedup.check_and_record("a", "one");
        dedup.invalidate("a");
        assert_eq!(dedup.check_and_record("a", "one"), DedupResult::Fresh);
        dedup.clear();
        assert!(dedup.is_empty());
    }
}
