use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub const PLUGIN_STORAGE_QUOTA_BYTES: u64 = 256 * 1024 * 1024;
const TEMP_FILE_PREFIX: &str = ".bakingrl-write-";

static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);
static STORAGE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

#[derive(Clone)]
pub struct PluginStorage {
    root: PathBuf,
    quota_bytes: u64,
    lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for PluginStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginStorage")
            .field("root", &self.root)
            .field("quota_bytes", &self.quota_bytes)
            .finish()
    }
}

impl PluginStorage {
    pub fn new(root: PathBuf) -> Self {
        Self::with_quota(root, PLUGIN_STORAGE_QUOTA_BYTES)
    }

    fn with_quota(root: PathBuf, quota_bytes: u64) -> Self {
        let lock = storage_lock(&root);
        Self {
            root,
            quota_bytes,
            lock,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read_text(&self, path: &str) -> Result<String, String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes)
            .map_err(|error| format!("Storage file '{path}' is not valid UTF-8: {error}"))
    }

    pub fn read_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let bytes = self.read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Storage file '{path}' is not valid JSON: {error}"))
    }

    pub fn write_text(&self, path: &str, contents: &str) -> Result<(), String> {
        self.write(path, contents.as_bytes())
    }

    pub fn write_json(&self, path: &str, value: &serde_json::Value) -> Result<(), String> {
        let contents = serde_json::to_vec(value)
            .map_err(|error| format!("Unable to serialize storage JSON: {error}"))?;
        self.write(path, &contents)
    }

    pub fn list(&self, prefix: Option<&str>) -> Result<Vec<String>, String> {
        let normalized = normalize_storage_path(prefix.unwrap_or(""), true)?;
        let _guard = self.lock.lock().map_err(|_| storage_lock_error())?;
        ensure_storage_root(&self.root)?;
        let start = if normalized.is_empty() {
            self.root.clone()
        } else {
            resolve_storage_path(&self.root, &normalized, false)?
        };
        let metadata = match fs::symlink_metadata(&start) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "Unable to inspect storage prefix '{}': {error}",
                    start.display()
                ))
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Storage prefix '{}' cannot be a symlink.",
                start.display()
            ));
        }
        let mut files = Vec::new();
        if metadata.is_file() {
            files.push(normalized);
        } else if metadata.is_dir() {
            collect_storage_files(&self.root, &start, &mut files)?;
        } else {
            return Err(format!(
                "Storage prefix '{}' is not a regular file or directory.",
                start.display()
            ));
        }
        files.sort();
        Ok(files)
    }

    pub fn delete(&self, path: &str) -> Result<bool, String> {
        let normalized = normalize_storage_path(path, false)?;
        let _guard = self.lock.lock().map_err(|_| storage_lock_error())?;
        ensure_storage_root(&self.root)?;
        let target = resolve_storage_path(&self.root, &normalized, false)?;
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "Unable to inspect storage file '{}': {error}",
                    target.display()
                ))
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Storage path '{normalized}' is not a regular file."
            ));
        }
        fs::remove_file(&target).map_err(|error| {
            format!(
                "Unable to delete storage file '{}': {error}",
                target.display()
            )
        })?;
        Ok(true)
    }

    pub fn usage(&self) -> Result<StorageUsage, String> {
        let _guard = self.lock.lock().map_err(|_| storage_lock_error())?;
        ensure_storage_root(&self.root)?;
        Ok(StorageUsage {
            used_bytes: storage_used_bytes(&self.root)?,
            quota_bytes: self.quota_bytes,
        })
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let normalized = normalize_storage_path(path, false)?;
        let _guard = self.lock.lock().map_err(|_| storage_lock_error())?;
        ensure_storage_root(&self.root)?;
        let target = resolve_storage_path(&self.root, &normalized, false)?;
        let metadata = fs::symlink_metadata(&target).map_err(|error| {
            format!(
                "Unable to inspect storage file '{}': {error}",
                target.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Storage path '{normalized}' is not a regular file."
            ));
        }
        let mut file = File::open(&target).map_err(|error| {
            format!(
                "Unable to open storage file '{}': {error}",
                target.display()
            )
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|error| {
            format!(
                "Unable to read storage file '{}': {error}",
                target.display()
            )
        })?;
        Ok(bytes)
    }

    fn write(&self, path: &str, contents: &[u8]) -> Result<(), String> {
        let normalized = normalize_storage_path(path, false)?;
        let _guard = self.lock.lock().map_err(|_| storage_lock_error())?;
        ensure_storage_root(&self.root)?;
        let target = resolve_storage_path(&self.root, &normalized, true)?;
        let existing_bytes = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(format!(
                    "Storage path '{normalized}' is not a regular file."
                ))
            }
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => {
                return Err(format!(
                    "Unable to inspect storage file '{}': {error}",
                    target.display()
                ))
            }
        };
        let used_bytes = storage_used_bytes(&self.root)?;
        let next_used_bytes = used_bytes
            .saturating_sub(existing_bytes)
            .checked_add(contents.len() as u64)
            .ok_or_else(|| "Plugin storage usage overflowed u64.".to_string())?;
        if next_used_bytes > self.quota_bytes {
            return Err(format!(
                "Plugin storage quota exceeded: {next_used_bytes} bytes requested, {} bytes allowed.",
                self.quota_bytes
            ));
        }
        atomic_write(&target, contents)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub used_bytes: u64,
    pub quota_bytes: u64,
}

pub fn normalize_storage_path(path: &str, allow_empty: bool) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return if allow_empty {
            Ok(String::new())
        } else {
            Err("Storage path cannot be empty.".to_string())
        };
    }
    if trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
        || trimmed
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':' && trimmed.as_bytes()[0].is_ascii_alphabetic())
    {
        return Err(format!(
            "Storage path '{path}' must be a relative '/' path."
        ));
    }
    let mut normalized = Vec::new();
    for component in trimmed.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(format!("Storage path '{path}' cannot contain '..'."));
            }
            component => normalized.push(component),
        }
    }
    if normalized.is_empty() && !allow_empty {
        return Err(format!("Storage path '{path}' must name a file."));
    }
    Ok(normalized.join("/"))
}

fn storage_lock(root: &Path) -> Arc<Mutex<()>> {
    let locks = STORAGE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(lock) = locks.get(root).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(root.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn storage_lock_error() -> String {
    "Plugin storage lock is poisoned.".to_string()
}

fn ensure_storage_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| {
        format!(
            "Unable to create plugin storage directory '{}': {error}",
            root.display()
        )
    })?;
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        format!(
            "Unable to inspect plugin storage directory '{}': {error}",
            root.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Plugin storage root '{}' must be a real directory.",
            root.display()
        ));
    }
    Ok(())
}

fn resolve_storage_path(
    root: &Path,
    normalized: &str,
    create_parents: bool,
) -> Result<PathBuf, String> {
    let components = normalized.split('/').collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Storage path '{normalized}' traverses symlink '{}'.",
                    current.display()
                ))
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "Storage path '{normalized}' traverses non-directory '{}'.",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create_parents => {
                fs::create_dir(&current).map_err(|error| {
                    format!(
                        "Unable to create storage directory '{}': {error}",
                        current.display()
                    )
                })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Unable to inspect storage directory '{}': {error}",
                    current.display()
                ))
            }
        }
    }
    if let Some(file_name) = components.last() {
        current.push(file_name);
    }
    if let Ok(metadata) = fs::symlink_metadata(&current) {
        if metadata.file_type().is_symlink() {
            return Err(format!("Storage path '{normalized}' cannot be a symlink."));
        }
    }
    Ok(current)
}

fn collect_storage_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "Unable to list storage directory '{}': {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Unable to list storage directory '{}': {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Unable to inspect storage path '{}': {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Storage path '{}' cannot be a symlink.",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_storage_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| format!("Storage path '{}' escapes its root.", path.display()))?;
            files.push(path_to_slash(relative)?);
        } else {
            return Err(format!(
                "Storage path '{}' is not a regular file or directory.",
                path.display()
            ));
        }
    }
    Ok(())
}

fn storage_used_bytes(root: &Path) -> Result<u64, String> {
    let mut files = Vec::new();
    collect_storage_files(root, root, &mut files)?;
    files.into_iter().try_fold(0u64, |total, relative| {
        let length = fs::metadata(root.join(&relative))
            .map_err(|error| format!("Unable to measure storage file '{relative}': {error}"))?
            .len();
        total
            .checked_add(length)
            .ok_or_else(|| "Plugin storage usage overflowed u64.".to_string())
    })
}

fn path_to_slash(path: &Path) -> Result<String, String> {
    let mut components = Vec::new();
    for component in path.components() {
        let value = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| format!("Storage path '{}' is not valid UTF-8.", path.display()))?;
        components.push(value);
    }
    Ok(components.join("/"))
}

fn atomic_write(target: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("Storage target '{}' has no parent.", target.display()))?;
    let temp = parent.join(format!(
        "{TEMP_FILE_PREFIX}{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| {
                format!(
                    "Unable to create temporary storage file '{}': {error}",
                    temp.display()
                )
            })?;
        file.write_all(contents).map_err(|error| {
            format!(
                "Unable to write temporary storage file '{}': {error}",
                temp.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "Unable to flush temporary storage file '{}': {error}",
                temp.display()
            )
        })?;
        drop(file);
        replace_file(&temp, target)?;
        sync_parent_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| {
        format!(
            "Unable to atomically replace storage file '{}': {error}",
            target.display()
        )
    })
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let existing = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            replacement.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(format!(
            "Unable to atomically replace storage file '{}': {}",
            target.display(),
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<(), String> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "Unable to flush storage directory '{}': {error}",
                parent.display()
            )
        })
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn normalizes_relative_storage_paths_and_rejects_escape_forms() {
        assert_eq!(
            normalize_storage_path("state//./match.json", false).unwrap(),
            "state/match.json"
        );
        for invalid in [
            "",
            "/absolute",
            "../escape",
            "a/../escape",
            "a\\b",
            "C:/escape",
        ] {
            assert!(normalize_storage_path(invalid, false).is_err(), "{invalid}");
        }
    }

    #[test]
    fn reads_writes_lists_json_and_deletes_files() {
        let temp = TempDir::new().unwrap();
        let storage = PluginStorage::new(temp.path().join("plugin"));

        storage.write_text("logs/latest.txt", "ready").unwrap();
        storage
            .write_json("state/value.json", &serde_json::json!({ "value": 42 }))
            .unwrap();

        assert_eq!(storage.read_text("logs/latest.txt").unwrap(), "ready");
        assert_eq!(storage.read_json("state/value.json").unwrap()["value"], 42);
        assert_eq!(
            storage.list(None).unwrap(),
            vec!["logs/latest.txt", "state/value.json"]
        );
        assert_eq!(
            storage.list(Some("state")).unwrap(),
            vec!["state/value.json"]
        );
        assert!(storage.delete("logs/latest.txt").unwrap());
        assert!(!storage.delete("logs/latest.txt").unwrap());
    }

    #[test]
    fn replaces_files_atomically_without_counting_old_bytes_twice() {
        let temp = TempDir::new().unwrap();
        let storage = PluginStorage::with_quota(temp.path().join("plugin"), 8);
        storage.write_text("value.txt", "12345678").unwrap();
        storage.write_text("value.txt", "abcdefgh").unwrap();
        assert_eq!(storage.read_text("value.txt").unwrap(), "abcdefgh");
        assert_eq!(storage.usage().unwrap().used_bytes, 8);
        assert!(storage.write_text("other.txt", "x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_storage_paths() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let root = temp.path().join("plugin");
        fs::create_dir_all(&root).unwrap();
        symlink(temp.path(), root.join("linked")).unwrap();
        let storage = PluginStorage::new(root);
        assert!(storage.write_text("linked/escape.txt", "bad").is_err());
        assert!(storage.list(None).is_err());
    }
}
