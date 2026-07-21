//! Bounded in-memory cache for identical model responses.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Maximum number of responses retained by a response cache.
pub const MAX_ENTRIES: usize = 512;
/// Default lifetime for a cached response.
pub const DEFAULT_TTL: Duration = Duration::from_secs(5 * 60);

/// Stable cache key derived from response-defining request fields.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResponseCacheKey {
    /// First 64 bits of the digest of the key components.
    pub hash: u64,
}

impl ResponseCacheKey {
    /// Hashes model, prompt hash, temperature, and maximum output tokens.
    pub fn new(
        model: impl AsRef<str>,
        prompt_hash: u64,
        temperature: f32,
        max_tokens: u64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        let model = model.as_ref();
        hasher.update(&(model.len() as u64).to_be_bytes());
        hasher.update(model.as_bytes());
        hasher.update(&prompt_hash.to_be_bytes());
        hasher.update(&temperature.to_bits().to_be_bytes());
        hasher.update(&max_tokens.to_be_bytes());

        let digest = hasher.finalize();
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&digest.as_bytes()[..8]);
        Self {
            hash: u64::from_be_bytes(bytes),
        }
    }
}

/// A response stored in the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedResponse {
    pub body: Vec<u8>,
    pub tokens: u64,
    pub created_at: Instant,
    pub ttl: Duration,
}

#[derive(Debug, Default)]
struct CacheState {
    entries: VecDeque<(ResponseCacheKey, CachedResponse)>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

/// Thread-safe bounded LRU response cache.
#[derive(Debug)]
pub struct ResponseCache {
    capacity: usize,
    ttl: Duration,
    state: Mutex<CacheState>,
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new(MAX_ENTRIES, DEFAULT_TTL)
    }
}

impl ResponseCache {
    /// Creates a cache with the requested capacity and defaulting zero TTLs.
    ///
    /// Capacity is clamped to the inclusive range 1..=512.
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity: capacity.clamp(1, MAX_ENTRIES),
            ttl,
            state: Mutex::new(CacheState::default()),
        }
    }

    /// Creates a cache with the requested TTL and the maximum capacity.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self::new(MAX_ENTRIES, ttl)
    }

    /// Looks up a response, refreshing its LRU position on a live hit.
    pub fn get(&self, key: &ResponseCacheKey) -> Option<CachedResponse> {
        let mut state = self.lock_state();
        let now = Instant::now();
        let position = state
            .entries
            .iter()
            .position(|(entry_key, _)| entry_key == key);

        let Some(position) = position else {
            state.misses += 1;
            return None;
        };

        if is_expired(&state.entries[position].1, now) {
            state.entries.remove(position);
            state.misses += 1;
            return None;
        }

        let entry = state.entries.remove(position)?;
        state.entries.push_back(entry.clone());
        state.hits += 1;
        Some(entry.1)
    }

    /// Inserts or replaces a response, evicting the least recently used entry
    /// when the bounded capacity is reached.
    pub fn put(&self, key: ResponseCacheKey, mut response: CachedResponse) {
        let mut state = self.lock_state();
        remove_expired(&mut state.entries, Instant::now());

        if response.ttl.is_zero() {
            response.ttl = self.ttl;
        }

        if let Some(position) = state
            .entries
            .iter()
            .position(|(entry_key, _)| *entry_key == key)
        {
            state.entries.remove(position);
        } else if state.entries.len() >= self.capacity {
            state.entries.pop_front();
            state.evictions += 1;
        }

        state.entries.push_back((key, response));
    }

    /// Returns cumulative hit, miss, and eviction counters.
    pub fn stats(&self) -> CacheStats {
        let state = self.lock_state();
        let total = state.hits + state.misses;
        CacheStats {
            hits: state.hits,
            misses: state.misses,
            evictions: state.evictions,
            hit_rate: if total == 0 {
                0.0
            } else {
                state.hits as f64 / total as f64
            },
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, CacheState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn is_expired(response: &CachedResponse, now: Instant) -> bool {
    now.checked_duration_since(response.created_at)
        .unwrap_or_default()
        >= response.ttl
}

fn remove_expired(entries: &mut VecDeque<(ResponseCacheKey, CachedResponse)>, now: Instant) {
    entries.retain(|(_, response)| !is_expired(response, now));
}

/// Snapshot of cache activity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_key(model: &str) -> ResponseCacheKey {
        ResponseCacheKey::new(model, 42, 0.2, 128)
    }

    fn response(body: &[u8], created_at: Instant, ttl: Duration) -> CachedResponse {
        CachedResponse {
            body: body.to_vec(),
            tokens: body.len() as u64,
            created_at,
            ttl,
        }
    }

    #[test]
    fn key_hash_changes_when_request_fields_change() {
        let base = cache_key("model-a");
        assert_ne!(base, cache_key("model-b"));
        assert_ne!(base, ResponseCacheKey::new("model-a", 43, 0.2, 128));
        assert_ne!(base, ResponseCacheKey::new("model-a", 42, 0.3, 128));
        assert_ne!(base, ResponseCacheKey::new("model-a", 42, 0.2, 129));
    }

    #[test]
    fn cache_hit_and_miss_update_stats() {
        let cache = ResponseCache::new(4, Duration::from_secs(60));
        let key = cache_key("model-a");
        cache.put(key, response(b"answer", Instant::now(), Duration::ZERO));

        assert_eq!(cache.get(&key).unwrap().body, b"answer");
        assert!(cache.get(&cache_key("model-b")).is_none());

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evictions, 0);
        assert!((stats.hit_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn expired_entries_count_as_misses() {
        let cache = ResponseCache::with_ttl(Duration::from_secs(60));
        let key = cache_key("model-a");
        cache.put(
            key,
            response(
                b"old",
                Instant::now() - Duration::from_secs(61),
                Duration::from_secs(60),
            ),
        );

        assert!(cache.get(&key).is_none());
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evictions, 0);
    }

    #[test]
    fn lru_eviction_removes_least_recently_used_entry() {
        let cache = ResponseCache::new(2, Duration::from_secs(60));
        let first = cache_key("first");
        let second = cache_key("second");
        let third = cache_key("third");

        cache.put(first, response(b"1", Instant::now(), Duration::ZERO));
        cache.put(second, response(b"2", Instant::now(), Duration::ZERO));
        assert!(cache.get(&first).is_some());
        cache.put(third, response(b"3", Instant::now(), Duration::ZERO));

        assert!(cache.get(&second).is_none());
        assert!(cache.get(&first).is_some());
        assert!(cache.get(&third).is_some());

        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn capacity_is_hard_capped() {
        let cache = ResponseCache::new(MAX_ENTRIES + 1, Duration::from_secs(60));
        for index in 0..=MAX_ENTRIES {
            cache.put(
                ResponseCacheKey { hash: index as u64 },
                response(b"x", Instant::now(), Duration::ZERO),
            );
        }

        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn default_policy_uses_five_minute_ttl() {
        let cache = ResponseCache::default();
        let key = cache_key("default");
        cache.put(
            key,
            response(
                b"answer",
                Instant::now() - Duration::from_secs(301),
                Duration::ZERO,
            ),
        );

        assert!(cache.get(&key).is_none());
    }
}
