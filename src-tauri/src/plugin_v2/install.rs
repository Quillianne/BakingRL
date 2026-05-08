use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use reqwest::Url;
use tokio::io::AsyncWriteExt;

use super::bundle::{extract_bundle, inspect_bundle, BundleInspection};

const MAX_BUNDLE_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallReceipt {
    pub package_id: String,
    pub version: String,
    pub source: String,
    pub bundle_sha256: String,
    pub installed_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DeepLinkInstallRequest {
    pub url: String,
    pub sha256: Option<String>,
}

pub fn inspect_bundle_file(path: &Path) -> Result<BundleInspection, String> {
    inspect_bundle(path)
}

pub fn parse_install_deep_link(value: &str) -> Result<DeepLinkInstallRequest, String> {
    let parsed = Url::parse(value).map_err(|e| format!("Invalid deep link URL: {e}"))?;
    if parsed.scheme() != "bakingrl" {
        return Err("Deep link scheme must be bakingrl".to_string());
    }
    if parsed.host_str() != Some("install") || !matches!(parsed.path(), "" | "/") {
        return Err("Deep link action must be install".to_string());
    }

    let mut bundle_url: Option<String> = None;
    let mut sha256: Option<String> = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "url" => {
                if bundle_url.replace(value.into_owned()).is_some() {
                    return Err("Deep link cannot contain multiple url parameters".to_string());
                }
            }
            "sha256" => {
                let value = value.into_owned();
                validate_sha256_hex(&value)?;
                if sha256.replace(value).is_some() {
                    return Err("Deep link cannot contain multiple sha256 parameters".to_string());
                }
            }
            other => return Err(format!("Unsupported deep link parameter '{other}'")),
        }
    }

    let bundle_url = bundle_url.ok_or_else(|| "Deep link is missing url parameter".to_string())?;
    let parsed_bundle_url =
        Url::parse(&bundle_url).map_err(|e| format!("Invalid plugin bundle URL: {e}"))?;
    if parsed_bundle_url.scheme() != "https" {
        return Err("Deep link plugin bundle URL must use HTTPS".to_string());
    }
    if parsed_bundle_url.host_str().is_none() {
        return Err("Deep link plugin bundle URL must include a host".to_string());
    }

    Ok(DeepLinkInstallRequest {
        url: bundle_url,
        sha256,
    })
}

pub async fn download_bundle_to_file(url: &str, target: &Path) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("Invalid plugin URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("Plugin bundle URL must use HTTPS".to_string());
    }
    let response = reqwest::get(parsed)
        .await
        .map_err(|e| format!("Unable to download plugin bundle: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Plugin bundle download failed with HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BUNDLE_DOWNLOAD_BYTES)
    {
        return Err("Downloaded plugin bundle is too large".to_string());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Unable to create download directory: {e}"))?;
    }
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("Unable to write downloaded bundle: {e}"))?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Unable to read plugin bundle response: {e}"))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "Downloaded plugin bundle size overflow".to_string())?;
        if downloaded > MAX_BUNDLE_DOWNLOAD_BYTES {
            return Err("Downloaded plugin bundle is too large".to_string());
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Unable to write downloaded bundle: {e}"))?;
    }
    file.flush()
        .await
        .map_err(|e| format!("Unable to write downloaded bundle: {e}"))
}

pub fn install_bundle_from_file(
    bundle_path: &Path,
    packages_dir: &Path,
    staging_root: &Path,
    source: String,
) -> Result<InstallReceipt, String> {
    let inspection = inspect_bundle(bundle_path)?;
    let package_id = inspection.manifest.id.clone();
    let version = inspection.manifest.version.clone();
    let staging_dir = staging_root.join(format!("{}-{}", sanitize_id(&package_id), unique_stamp()));
    fs::create_dir_all(packages_dir)
        .map_err(|e| format!("Unable to create package directory: {e}"))?;
    let installed_dir = safe_install_dir(packages_dir, &package_id)?;

    extract_bundle(bundle_path, &staging_dir)?;

    if installed_dir.exists() {
        ensure_existing_install_dir_is_safe(packages_dir, &installed_dir)?;
        fs::remove_dir_all(&installed_dir)
            .map_err(|e| format!("Unable to replace previous package: {e}"))?;
    }
    fs::rename(&staging_dir, &installed_dir)
        .map_err(|e| format!("Unable to activate installed package: {e}"))?;

    let receipt = InstallReceipt {
        package_id,
        version,
        source,
        bundle_sha256: inspection.sha256,
        installed_at_ms: unique_stamp(),
    };
    let raw = serde_json::to_string_pretty(&receipt)
        .map_err(|e| format!("Unable to serialize install receipt: {e}"))?;
    fs::write(installed_dir.join("bakingrl.install.json"), raw)
        .map_err(|e| format!("Unable to write install receipt: {e}"))?;
    Ok(receipt)
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_' || *ch == '.')
        .collect()
}

fn safe_install_dir(packages_dir: &Path, package_id: &str) -> Result<std::path::PathBuf, String> {
    let packages_dir = fs::canonicalize(packages_dir)
        .map_err(|e| format!("Unable to resolve package directory: {e}"))?;
    let candidate = packages_dir.join(package_id);
    if candidate.parent() != Some(packages_dir.as_path()) {
        return Err(format!(
            "Package id '{package_id}' escapes the package directory"
        ));
    }
    Ok(candidate)
}

fn ensure_existing_install_dir_is_safe(
    packages_dir: &Path,
    installed_dir: &Path,
) -> Result<(), String> {
    let packages_dir = fs::canonicalize(packages_dir)
        .map_err(|e| format!("Unable to resolve package directory: {e}"))?;
    let resolved = fs::canonicalize(installed_dir)
        .map_err(|e| format!("Unable to resolve installed package directory: {e}"))?;
    if resolved.parent() != Some(packages_dir.as_path()) {
        return Err("Installed package path escapes the package directory".to_string());
    }
    Ok(())
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn validate_sha256_hex(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Deep link sha256 must be a 64-character hexadecimal string".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_install_deep_link() {
        let request = parse_install_deep_link(
            "bakingrl://install?url=https%3A%2F%2Fexample.com%2Fplugin.brlp&sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();

        assert_eq!(request.url, "https://example.com/plugin.brlp");
        assert_eq!(
            request.sha256.as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn rejects_deep_link_file_urls() {
        let error =
            parse_install_deep_link("bakingrl://install?url=file%3A%2F%2F%2Ftmp%2Fplugin.brlp")
                .unwrap_err();

        assert!(error.contains("must use HTTPS"));
    }

    #[test]
    fn rejects_unknown_deep_link_parameters() {
        let error = parse_install_deep_link(
            "bakingrl://install?url=https%3A%2F%2Fexample.com%2Fplugin.brlp&path=/tmp/plugin.brlp",
        )
        .unwrap_err();

        assert!(error.contains("Unsupported deep link parameter"));
    }

    #[test]
    fn rejects_invalid_deep_link_hash() {
        let error = parse_install_deep_link(
            "bakingrl://install?url=https%3A%2F%2Fexample.com%2Fplugin.brlp&sha256=bad",
        )
        .unwrap_err();

        assert!(error.contains("64-character"));
    }
}
