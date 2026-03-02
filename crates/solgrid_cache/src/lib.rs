//! Content-hash-based file cache for solgrid.
//!
//! Caches lint/format results per file so unchanged files can be skipped.
//! The cache is invalidated when the solgrid version or config hash changes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The cache store persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStore {
    /// solgrid version that created this cache.
    pub version: String,
    /// SHA-256 hash of the serialized config.
    pub config_hash: String,
    /// Per-file cache entries, keyed by file path.
    pub entries: HashMap<String, CacheEntry>,
}

/// A single cached file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// SHA-256 hash of the file content.
    pub content_hash: String,
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
    pub fn load(cache_dir: &Path, config_hash: &str) -> Self {
        let cache_path = cache_dir.join("solgrid.cache.json");
        let version = env!("CARGO_PKG_VERSION").to_string();

        let store = if cache_path.exists() {
            match std::fs::read_to_string(&cache_path) {
                Ok(content) => match serde_json::from_str::<CacheStore>(&content) {
                    Ok(store) if store.version == version && store.config_hash == config_hash => {
                        store
                    }
                    _ => CacheStore {
                        version: version.clone(),
                        config_hash: config_hash.to_string(),
                        entries: HashMap::new(),
                    },
                },
                Err(_) => CacheStore {
                    version: version.clone(),
                    config_hash: config_hash.to_string(),
                    entries: HashMap::new(),
                },
            }
        } else {
            CacheStore {
                version,
                config_hash: config_hash.to_string(),
                entries: HashMap::new(),
            }
        };

        Cache {
            store,
            cache_path,
            dirty: false,
        }
    }

    /// Check if a file is cached and unchanged.
    /// Returns Some(entry) if the file's content hash matches the cache.
    pub fn check(&self, path: &str, content: &str) -> Option<&CacheEntry> {
        let entry = self.store.entries.get(path)?;
        let hash = sha256_hex(content);
        if entry.content_hash == hash {
            Some(entry)
        } else {
            None
        }
    }

    /// Update the cache entry for a file.
    pub fn update(
        &mut self,
        path: &str,
        content: &str,
        diagnostic_count: usize,
        is_formatted: bool,
    ) {
        let hash = sha256_hex(content);
        self.store.entries.insert(
            path.to_string(),
            CacheEntry {
                content_hash: hash,
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
        let cache = Cache::load(&dir, "config_hash_1");
        assert!(cache.check("test.sol", "content").is_none());
    }

    #[test]
    fn test_cache_check_hit() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_hit");
        let _ = fs::remove_dir_all(&dir);
        let mut cache = Cache::load(&dir, "config_hash_2");
        cache.update("test.sol", "content", 0, true);
        let entry = cache.check("test.sol", "content");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().diagnostic_count, 0);
        assert!(entry.unwrap().is_formatted);
    }

    #[test]
    fn test_cache_check_stale() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_stale");
        let _ = fs::remove_dir_all(&dir);
        let mut cache = Cache::load(&dir, "config_hash_3");
        cache.update("test.sol", "old content", 0, true);
        assert!(cache.check("test.sol", "new content").is_none());
    }

    #[test]
    fn test_cache_save_and_reload() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_reload");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = Cache::load(&dir, "config_hash_4");
        cache.update("test.sol", "content", 3, false);
        cache.save().unwrap();

        let cache2 = Cache::load(&dir, "config_hash_4");
        let entry = cache2.check("test.sol", "content");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().diagnostic_count, 3);
    }

    #[test]
    fn test_cache_invalidated_on_config_change() {
        let dir = std::env::temp_dir().join("solgrid_test_cache_invalidate");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = Cache::load(&dir, "config_hash_5");
        cache.update("test.sol", "content", 0, true);
        cache.save().unwrap();

        // Different config hash should invalidate
        let cache2 = Cache::load(&dir, "different_config_hash");
        assert!(cache2.check("test.sol", "content").is_none());
    }
}
