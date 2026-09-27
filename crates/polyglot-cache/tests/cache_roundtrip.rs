//! Integration tests for the incremental compilation cache
//! (`polyglot-cache`): key generation, store/lookup, on-disk manifest
//! persistence and retention cleanup.

use polyglot_cache::{CacheEntry, CompilationCache};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// A unique scratch directory per test (never shared between runs).
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "polyglot_cache_tests_{}_{}_{}",
        std::process::id(),
        tag,
        nanos
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn entry(key: &str, timestamp: u64) -> CacheEntry {
    CacheEntry {
        key: key.to_string(),
        object_path: PathBuf::from(format!("/tmp/fake_objects/{key}.o")),
        mlir_module: format!("module @{key} {{ }}"),
        timestamp,
    }
}

#[tokio::test]
async fn put_then_get_roundtrips() {
    let dir = temp_dir("roundtrip");
    let mut cache = CompilationCache::new(dir.clone())
        .await
        .expect("cache created");

    let stored = entry("k1", 100);
    cache.put("k1".to_string(), stored).await.expect("put");

    let got = cache.get("k1").expect("entry present after put");
    assert_eq!(got.key, "k1");
    assert_eq!(got.object_path, PathBuf::from("/tmp/fake_objects/k1.o"));
    assert_eq!(got.mlir_module, "module @k1 { }");
    assert_eq!(got.timestamp, 100);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn get_misses_unknown_key() {
    let dir = temp_dir("miss");
    let mut cache = CompilationCache::new(dir.clone())
        .await
        .expect("cache created");
    cache
        .put("present".to_string(), entry("present", 1))
        .await
        .unwrap();

    assert!(cache.get("absent").is_none(), "unknown keys miss");
    assert!(cache.get("").is_none());
    assert!(
        cache.get(&"p".repeat(64)).is_none(),
        "a similar-but-different key misses"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn key_depends_on_every_input() {
    let args = vec!["-O2".to_string()];
    let base = CompilationCache::generate_key(
        "int main() {}",
        "clang",
        "1.0",
        &args,
        "x86_64-linux",
        "default",
    );

    // Deterministic: identical inputs produce the identical key.
    let again = CompilationCache::generate_key(
        "int main() {}",
        "clang",
        "1.0",
        &args,
        "x86_64-linux",
        "default",
    );
    assert_eq!(base, again);

    // Any single changed input changes the key.
    let variants = [
        CompilationCache::generate_key(
            "int main() { return 1; }",
            "clang",
            "1.0",
            &args,
            "x86_64-linux",
            "default",
        ),
        CompilationCache::generate_key(
            "int main() {}",
            "tcc",
            "1.0",
            &args,
            "x86_64-linux",
            "default",
        ),
        CompilationCache::generate_key(
            "int main() {}",
            "clang",
            "1.1",
            &args,
            "x86_64-linux",
            "default",
        ),
        CompilationCache::generate_key(
            "int main() {}",
            "clang",
            "1.0",
            &["-O0".to_string()],
            "x86_64-linux",
            "default",
        ),
        CompilationCache::generate_key(
            "int main() {}",
            "clang",
            "1.0",
            &args,
            "aarch64-linux",
            "default",
        ),
        CompilationCache::generate_key(
            "int main() {}",
            "clang",
            "1.0",
            &args,
            "x86_64-linux",
            "aggressive",
        ),
    ];
    for (i, v) in variants.iter().enumerate() {
        assert_ne!(&base, v, "variant {i} must differ from the base key");
    }
    // Keys are hex-encoded SHA-256 digests.
    assert_eq!(base.len(), 64);
    assert!(base.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn index_manifest_persists_across_reopen() {
    let dir = temp_dir("persist");
    {
        let mut cache = CompilationCache::new(dir.clone())
            .await
            .expect("cache created");
        cache
            .put("alpha".to_string(), entry("alpha", 1))
            .await
            .unwrap();
        cache
            .put("beta".to_string(), entry("beta", 2))
            .await
            .unwrap();
    } // dropped: index.json must already be on disk

    // The manifest exists and is valid JSON naming both keys.
    let manifest = std::fs::read_to_string(dir.join("index.json")).expect("index.json written");
    let parsed: serde_json::Value =
        serde_json::from_str(&manifest).expect("manifest is valid JSON");
    assert!(parsed.get("alpha").is_some(), "alpha in manifest: {parsed}");
    assert!(parsed.get("beta").is_some(), "beta in manifest: {parsed}");

    // A freshly opened cache sees the persisted entries.
    let reopened = CompilationCache::new(dir.clone()).await.expect("reopen");
    assert_eq!(
        reopened.get("alpha").unwrap().mlir_module,
        "module @alpha { }"
    );
    assert_eq!(reopened.get("beta").unwrap().timestamp, 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn clean_keeps_newest_entries() {
    let dir = temp_dir("clean");
    let mut cache = CompilationCache::new(dir.clone())
        .await
        .expect("cache created");
    cache
        .put("old".to_string(), entry("old", 100))
        .await
        .unwrap();
    cache
        .put("mid".to_string(), entry("mid", 200))
        .await
        .unwrap();
    cache
        .put("new".to_string(), entry("new", 300))
        .await
        .unwrap();

    cache.clean(2).await.expect("clean");

    assert!(cache.get("old").is_none(), "oldest entry evicted");
    assert!(cache.get("mid").is_some());
    assert!(cache.get("new").is_some());

    // No-op when already within the limit.
    cache.clean(10).await.expect("clean within limit");
    assert!(cache.get("mid").is_some() && cache.get("new").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn object_path_is_key_named_dot_o() {
    let dir = temp_dir("objpath");
    let cache = CompilationCache::new(dir.clone())
        .await
        .expect("cache created");
    let key = CompilationCache::generate_key("src", "clang", "1.0", &[], "x86_64-linux", "default");
    let path = cache.object_path(&key);
    assert_eq!(
        path.parent().unwrap(),
        dir,
        "objects live inside the cache dir"
    );
    assert_eq!(
        path.file_name().unwrap().to_str().unwrap(),
        format!("{key}.o")
    );

    let _ = std::fs::remove_dir_all(&dir);
}
