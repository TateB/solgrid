//! Content-hash-based file cache for solgrid.
//!
//! Caches lint/format results per file so unchanged files can be skipped.
//! The cache is invalidated when the solgrid version or config hash changes.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The cache store persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStore {
    /// solgrid version that created this cache.
    pub version: String,
    /// Legacy top-level config hash retained for backward-compatible deserialization.
    #[serde(default)]
    pub config_hash: String,
    /// Per-file cache entries, keyed by file path.
    pub entries: HashMap<String, CacheEntry>,
}

/// A single cached file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// SHA-256 hash of the file content.
    pub content_hash: String,
    /// SHA-256 hash of the effective config for this file.
    #[serde(default)]
    pub config_hash: String,
    /// Number of diagnostics produced.
    pub diagnostic_count: usize,
    /// Whether the file was already formatted.
    pub is_formatted: bool,
}

/// In-memory cache handle.
pub struct Cache {
    store: CacheStore,
    cache_path: PathBuf,
    dirty: bool,
}

impl Cache {
    /// Load cache from disk, or create a new empty cache.
    /// Returns a fresh cache if the file doesn't exist or is invalid.
    pub fn load(cache_dir: &Path) -> Self {
        let cache_path = cache_dir.join("solgrid.cache.json");
        let version = env!("CARGO_PKG_VERSION").to_string();

        let store = if cache_path.exists() {
            match std::fs::read_to_string(&cache_path) {
                Ok(content) => match serde_json::from_str::<CacheStore>(&content) {
                    Ok(store) if store.version == version => store,
                    _ => CacheStore {
                        version: version.clone(),
                        config_hash: String::new(),
                        entries: HashMap::new(),
                    },
                },
                Err(_) => CacheStore {
                    version: version.clone(),
                    config_hash: String::new(),
                    entries: HashMap::new(),
                },
            }
        } else {
            CacheStore {
                version,
                config_hash: String::new(),
                entries: HashMap::new(),
            }
        };

        Cache {
            store,
            cache_path,
            dirty: false,
        }
    }

    /// Check if a file is cached and unchanged using precomputed hashes.
    pub fn check_hashed(
        &self,
        path: &str,
        content_hash: &str,
        config_hash: &str,
    ) -> Option<&CacheEntry> {
        let entry = self.store.entries.get(path)?;
        if entry.content_hash == content_hash && entry.config_hash == config_hash {
            Some(entry)
        } else {
            None
        }
    }

    /// Update the cache entry for a file using precomputed hashes.
    pub fn update_hashed(
        &mut self,
        path: &str,
        content_hash: String,
        config_hash: String,
        diagnostic_count: usize,
        is_formatted: bool,
    ) {
        self.store.entries.insert(
            path.to_string(),
            CacheEntry {
                content_hash,
                config_hash,
                diagnostic_count,
                is_formatted,
            },
        );
        self.dirty = true;
    }

    /// Save cache to disk if it has been modified.
    pub fn save(&self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }

        // Ensure parent directory exists
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create cache dir: {e}"))?;
        }

        let json = serde_json::to_string(&self.store)
            .map_err(|e| format!("failed to serialize cache: {e}"))?;
        std::fs::write(&self.cache_path, json)
            .map_err(|e| format!("failed to write cache: {e}"))?;

        Ok(())
    }
}

/// Compute a SHA-256 hex digest of a string.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex_encode(&result)
}

/// Compute a deterministic hash for an effective config.
pub fn config_hash(config: &solgrid_config::Config) -> String {
    let value = serde_json::to_value(config).expect("config should serialize to JSON");
    let canonical = canonical_json(&value);
    sha256_hex(&canonical)
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).expect("JSON string should serialize"),
        Value::Array(values) => {
            let mut out = String::from("[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(value));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));

            let mut out = String::from("{");
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("JSON key should serialize"));
                out.push(':');
                out.push_str(&canonical_json(value));
            }
            out.push('}');
            out
        }
    }
}

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex("hello");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_cache_check_miss() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_miss");
        let _ = fs::remove_dir_all(&dir);
        let cache = Cache::load(&dir);
        let content_hash = sha256_hex("content");
        assert!(cache
            .check_hashed("test.sol", &content_hash, "config_hash_1")
            .is_none());
    }

    #[test]
    fn test_cache_check_hit() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_hit");
        let _ = fs::remove_dir_all(&dir);
        let mut cache = Cache::load(&dir);
        cache.update_hashed(
            "test.sol",
            sha256_hex("content"),
            "config_hash_2".to_string(),
            0,
            true,
        );
        let entry = cache.check_hashed("test.sol", &sha256_hex("content"), "config_hash_2");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().diagnostic_count, 0);
        assert!(entry.unwrap().is_formatted);
    }

    #[test]
    fn test_cache_check_stale() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_stale");
        let _ = fs::remove_dir_all(&dir);
        let mut cache = Cache::load(&dir);
        cache.update_hashed(
            "test.sol",
            sha256_hex("old content"),
            "config_hash_3".to_string(),
            0,
            true,
        );
        assert!(cache
            .check_hashed("test.sol", &sha256_hex("new content"), "config_hash_3")
            .is_none());
    }

    #[test]
    fn test_cache_save_and_reload() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_reload");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = Cache::load(&dir);
        cache.update_hashed(
            "test.sol",
            sha256_hex("content"),
            "config_hash_4".to_string(),
            3,
            false,
        );
        cache.save().unwrap();

        let cache2 = Cache::load(&dir);
        let entry = cache2.check_hashed("test.sol", &sha256_hex("content"), "config_hash_4");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().diagnostic_count, 3);
    }

    #[test]
    fn test_cache_invalidated_on_config_change() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_invalidate");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = Cache::load(&dir);
        cache.update_hashed(
            "test.sol",
            sha256_hex("content"),
            "config_hash_5".to_string(),
            0,
            true,
        );
        cache.save().unwrap();

        // Different config hash should invalidate
        let cache2 = Cache::load(&dir);
        assert!(cache2
            .check_hashed("test.sol", &sha256_hex("content"), "different_config_hash")
            .is_none());
    }

    /// Finding #10: config hash uses `format!("{:?}", config)` which relies on
    /// HashMap's Debug output.  HashMap iteration order is non-deterministic
    /// across runs (random hash seed), so the same config can produce different
    /// hashes.  This test demonstrates the instability.
    #[test]
    fn test_config_debug_hash_is_stable_across_identical_configs() {
        use solgrid_config::Config;

        // Build two identical configs by inserting rules in different order.
        let mut config_a = Config::default();
        config_a
            .lint
            .rules
            .insert("security/tx-origin".into(), solgrid_config::RuleLevel::Warn);
        config_a.lint.rules.insert(
            "best-practices/no-console".into(),
            solgrid_config::RuleLevel::Off,
        );
        config_a.lint.rules.insert(
            "naming/func-name-mixedcase".into(),
            solgrid_config::RuleLevel::Error,
        );

        let mut config_b = Config::default();
        config_b.lint.rules.insert(
            "naming/func-name-mixedcase".into(),
            solgrid_config::RuleLevel::Error,
        );
        config_b
            .lint
            .rules
            .insert("security/tx-origin".into(), solgrid_config::RuleLevel::Warn);
        config_b.lint.rules.insert(
            "best-practices/no-console".into(),
            solgrid_config::RuleLevel::Off,
        );

        let hash_a = config_hash(&config_a);
        let hash_b = config_hash(&config_b);

        assert_eq!(
            hash_a, hash_b,
            "config hashes should be identical for configs with the same rules"
        );
    }
}
