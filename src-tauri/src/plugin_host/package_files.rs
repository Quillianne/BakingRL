use std::fs;
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

const MAX_GATEWAY_PACKAGE_FILE_BYTES: u64 = 25 * 1024 * 1024;

pub(super) fn parse_export_ref(value: &str) -> Result<(&str, &str), String> {
    let Some((package_id, export_name)) = value.split_once('/') else {
        return Err(format!(
            "Export ref '{value}' must use '<package-id>/<export>'."
        ));
    };
    if package_id.trim().is_empty() || export_name.trim().is_empty() {
        return Err(format!(
            "Export ref '{value}' must use '<package-id>/<export>'."
        ));
    }
    Ok((package_id, export_name))
}

pub(super) fn safe_package_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    let mut path = PathBuf::new();
    for component in Path::new(relative_path).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "Package file '{relative_path}' escapes the package root."
                ));
            }
        }
    }
    if path.as_os_str().is_empty() {
        return Err("Package file path cannot be empty.".to_string());
    }
    Ok(path)
}

pub(super) fn safe_installed_package_dir(
    packages_dir: &Path,
    package_id: &str,
) -> Result<PathBuf, String> {
    if package_id == "."
        || package_id == ".."
        || package_id.contains('/')
        || package_id.contains('\\')
        || package_id.trim().is_empty()
    {
        return Err(format!(
            "Package id '{package_id}' is not a safe package directory name."
        ));
    }
    let packages_dir = packages_dir
        .canonicalize()
        .map_err(|e| format!("Unable to resolve package directory: {e}"))?;
    let target = packages_dir.join(package_id);
    if target.parent() != Some(packages_dir.as_path()) {
        return Err(format!(
            "Package id '{package_id}' escapes the package directory."
        ));
    }
    Ok(target)
}

pub(super) fn read_binary_package_file(
    package_root: &Path,
    relative_path: &Path,
) -> Result<Vec<u8>, String> {
    let root = package_root
        .canonicalize()
        .map_err(|e| format!("Unable to resolve package root: {e}"))?;
    let path = root.join(relative_path);
    let resolved = path.canonicalize().map_err(|e| {
        format!(
            "Unable to resolve package file '{}': {e}",
            relative_path.display()
        )
    })?;
    if !resolved.starts_with(&root) {
        return Err(format!(
            "Package file '{}' escapes the package root.",
            relative_path.display()
        ));
    }
    let metadata = resolved.metadata().map_err(|e| {
        format!(
            "Unable to inspect package file '{}': {e}",
            relative_path.display()
        )
    })?;
    if metadata.len() > MAX_GATEWAY_PACKAGE_FILE_BYTES {
        return Err(format!(
            "Package file '{}' exceeds the read limit.",
            relative_path.display()
        ));
    }
    fs::read(resolved).map_err(|e| {
        format!(
            "Unable to read package file '{}': {e}",
            relative_path.display()
        )
    })
}

pub(super) fn read_package_file(
    package_root: &Path,
    relative_path: &str,
) -> Result<String, String> {
    let root = package_root
        .canonicalize()
        .map_err(|e| format!("Unable to resolve package root: {e}"))?;
    let path = root.join(relative_path);
    let resolved = path
        .canonicalize()
        .map_err(|e| format!("Unable to resolve package file '{relative_path}': {e}"))?;
    if !resolved.starts_with(&root) {
        return Err(format!(
            "Package file '{relative_path}' escapes the package root."
        ));
    }
    let metadata = resolved
        .metadata()
        .map_err(|e| format!("Unable to inspect package file '{relative_path}': {e}"))?;
    if metadata.len() > MAX_GATEWAY_PACKAGE_FILE_BYTES {
        return Err(format!(
            "Package file '{relative_path}' exceeds the read limit."
        ));
    }
    fs::read_to_string(resolved)
        .map_err(|e| format!("Unable to read package file '{relative_path}': {e}"))
}

pub(super) fn read_json_package_file(
    package_root: &Path,
    relative_path: &str,
) -> Result<serde_json::Value, String> {
    let raw = read_package_file(package_root, relative_path)?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("Package JSON file '{relative_path}' is invalid: {e}"))
}

pub(super) fn find_first_bundle(root: &Path) -> Result<PathBuf, String> {
    let mut bundles = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != ".git" && name != "node_modules"
        })
        .flatten()
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "brlp")
        {
            bundles.push(entry.path().to_path_buf());
        }
    }
    bundles.sort();
    bundles
        .into_iter()
        .next()
        .ok_or_else(|| "Git repository does not contain a .brlp bundle".to_string())
}

pub(super) fn format_command_error(command: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("{command} failed")
    } else {
        format!("{command} failed: {stderr}")
    }
}

pub(super) fn is_remote_package_source(source: &str) -> bool {
    source.starts_with("url:")
        || source.starts_with("deeplink:")
        || source.starts_with("git:")
        || source.starts_with("marketplace:")
}
