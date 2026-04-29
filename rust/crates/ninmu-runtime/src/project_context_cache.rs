use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::prompt::ProjectContext;

/// TTL for the on-disk project context cache. 30 seconds is long enough to
/// amortize git subprocess calls across multiple turns / process restarts,
/// short enough that a `git commit` is reflected promptly.
const DISK_CACHE_TTL: Duration = Duration::from_secs(30);

/// On-disk entry for a discovered project context.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiskCacheEntry {
    /// Unix timestamp (seconds) when this entry was written.
    cached_at_unix_secs: u64,
    /// mtime of `.git/index` at the time of caching.
    git_index_mtime: u64,
    /// mtime of `.git/HEAD` at the time of caching.
    git_head_mtime: u64,
    /// The cached project context.
    context: ProjectContext,
}

/// Return the directory used for the project-context disk cache.
fn cache_dir() -> PathBuf {
    std::env::var("NINMU_CACHE_HOME")
        .map_or_else(
            |_| {
                std::env::var("XDG_CACHE_HOME")
                    .map_or_else(
                        |_| {
                            std::env::var("HOME")
                                .map_or_else(|_| std::env::temp_dir(), PathBuf::from)
                                .join(".cache")
                        },
                        PathBuf::from,
                    )
                    .join("ninmu")
            },
            PathBuf::from,
        )
        .join("project-context")
}

/// Derive a stable file name from the workspace path.
fn cache_file_name(cwd: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut s = DefaultHasher::new();
    cwd.canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf())
        .hash(&mut s);
    format!("{:016x}.json", s.finish())
}

/// Read the mtime (seconds since epoch) of a file, or `0` if unavailable.
fn file_mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Load project context from the on-disk cache if it is still valid.
///
/// Validity requires:
/// 1. The cache file exists and is readable JSON.
/// 2. The entry is younger than `DISK_CACHE_TTL`.
/// 3. The mtime of `.git/index` has not changed since caching.
/// 4. The mtime of `.git/HEAD` has not changed since caching.
///
/// If any check fails, `None` is returned and the caller should recompute.
pub fn load(cwd: &Path, current_date: &str) -> Option<ProjectContext> {
    let cache_path = cache_dir().join(cache_file_name(cwd));
    let bytes = fs::read(&cache_path).ok()?;
    let entry: DiskCacheEntry = serde_json::from_slice(&bytes).ok()?;

    // TTL check
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(entry.cached_at_unix_secs) > DISK_CACHE_TTL.as_secs() {
        return None;
    }

    // mtime checks — if either changed, the git state is stale
    let git_dir = cwd.join(".git");
    let index_mtime = file_mtime_secs(&git_dir.join("index"));
    let head_mtime = file_mtime_secs(&git_dir.join("HEAD"));
    if index_mtime != entry.git_index_mtime || head_mtime != entry.git_head_mtime {
        return None;
    }

    // Date mismatch — the cached entry is from a different day
    if entry.context.current_date != current_date {
        return None;
    }

    Some(entry.context)
}

/// Save a computed `ProjectContext` to the on-disk cache.
///
/// Writes are atomic (temp file + rename) to avoid partial reads by
/// concurrent processes.
pub fn save(cwd: &Path, context: &ProjectContext) -> io::Result<()> {
    let dir = cache_dir();
    fs::create_dir_all(&dir)?;

    let git_dir = cwd.join(".git");
    let entry = DiskCacheEntry {
        cached_at_unix_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        git_index_mtime: file_mtime_secs(&git_dir.join("index")),
        git_head_mtime: file_mtime_secs(&git_dir.join("HEAD")),
        context: context.clone(),
    };

    let cache_path = dir.join(cache_file_name(cwd));
    let temp_path = cache_path.with_extension("tmp");

    let mut file = fs::File::create(&temp_path)?;
    file.write_all(serde_json::to_vec_pretty(&entry)?.as_slice())?;
    file.sync_all()?;
    drop(file);

    fs::rename(temp_path, cache_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn dummy_context(cwd: &Path, date: &str) -> ProjectContext {
        ProjectContext {
            cwd: cwd.to_path_buf(),
            current_date: date.to_string(),
            git_status: Some("M file.rs".to_string()),
            git_diff: Some("diff --git".to_string()),
            git_context: None,
            instruction_files: vec![],
        }
    }

    #[test]
    fn cache_hit_when_mtines_match_and_ttl_not_expired() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(cwd.join(".git")).unwrap();
        fs::write(cwd.join(".git/index"), "index").unwrap();
        fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let ctx = dummy_context(&cwd, "2026-04-29");
        save(&cwd, &ctx).unwrap();

        let loaded = load(&cwd, "2026-04-29");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().cwd, ctx.cwd);
    }

    #[test]
    fn cache_miss_when_git_index_mtime_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(cwd.join(".git")).unwrap();
        fs::write(cwd.join(".git/index"), "index").unwrap();
        fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let ctx = dummy_context(&cwd, "2026-04-29");
        save(&cwd, &ctx).unwrap();

        // Simulate a git operation that touches the index.
        // On macOS/APFS mtime has 1-second resolution, so we must sleep
        // long enough for the mtime to actually change.
        std::thread::sleep(Duration::from_secs(1));
        fs::write(cwd.join(".git/index"), "new-index").unwrap();

        let loaded = load(&cwd, "2026-04-29");
        assert!(loaded.is_none());
    }

    #[test]
    fn cache_miss_when_ttl_expired() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(cwd.join(".git")).unwrap();
        fs::write(cwd.join(".git/index"), "index").unwrap();
        fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        // Manually write an expired entry
        let entry = DiskCacheEntry {
            cached_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(60),
            git_index_mtime: file_mtime_secs(&cwd.join(".git/index")),
            git_head_mtime: file_mtime_secs(&cwd.join(".git/HEAD")),
            context: dummy_context(&cwd, "2026-04-29"),
        };

        let dir = cache_dir();
        fs::create_dir_all(&dir).unwrap();
        let cache_path = dir.join(cache_file_name(&cwd));
        fs::write(&cache_path, serde_json::to_vec_pretty(&entry).unwrap()).unwrap();

        let loaded = load(&cwd, "2026-04-29");
        assert!(loaded.is_none());
    }

    #[test]
    fn cache_miss_when_date_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("repo");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(cwd.join(".git")).unwrap();
        fs::write(cwd.join(".git/index"), "index").unwrap();
        fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let ctx = dummy_context(&cwd, "2026-04-29");
        save(&cwd, &ctx).unwrap();

        let loaded = load(&cwd, "2026-04-30");
        assert!(loaded.is_none());
    }
}
