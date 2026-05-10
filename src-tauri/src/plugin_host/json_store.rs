use std::fs;
use std::path::Path;

pub(super) fn read_json_or_default<T>(path: &Path) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub(super) fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: serde::Serialize,
{
    let raw = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    fs::write(path, raw).map_err(|e| format!("Failed to write JSON: {e}"))
}
