use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::manifest::PluginPackageManifest;

const MANIFEST_FILE: &str = "bakingrl.plugin.json";
const HASHES_FILE: &str = "manifest.hashes.json";
const SIGNATURE_FILE: &str = "signature.ed25519";
const MAX_FILE_SIZE: u64 = 25 * 1024 * 1024;
const MAX_UNCOMPRESSED_SIZE: u64 = 150 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BundleHashes {
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundleInspection {
    pub manifest: PluginPackageManifest,
    pub hashes_present: bool,
    pub signature_present: bool,
    pub signature_verified: bool,
    pub signature_public_key: Option<String>,
    pub file_count: usize,
    pub uncompressed_size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundleSignatureInspection {
    pub present: bool,
    pub verified: bool,
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleSignatureFile {
    algorithm: String,
    public_key: String,
    signature: String,
    signed_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchivePathKind {
    File,
    Directory,
}

#[derive(Debug)]
struct ArchivePathRecord {
    canonical_path: String,
    kind: ArchivePathKind,
    explicit_entry: bool,
    source_entry: String,
}

#[derive(Debug, Default)]
struct PortableArchivePaths {
    paths: BTreeMap<String, ArchivePathRecord>,
}

impl PortableArchivePaths {
    fn register(&mut self, entry_name: &str, is_dir: bool) -> Result<(), String> {
        let components = portable_path_components(entry_name)?;
        validate_reserved_root_entry(&components, is_dir)?;

        let mut canonical_path = String::new();
        for (index, component) in components.iter().enumerate() {
            if !canonical_path.is_empty() {
                canonical_path.push('/');
            }
            canonical_path.push_str(component);

            let is_leaf = index + 1 == components.len();
            let kind = if is_leaf && !is_dir {
                ArchivePathKind::File
            } else {
                ArchivePathKind::Directory
            };
            let portable_key = canonical_path.to_uppercase();

            match self.paths.get_mut(&portable_key) {
                Some(existing) => {
                    if existing.canonical_path != canonical_path {
                        return Err(format!(
                            "Bundle entries '{}' and '{}' collide after Unicode uppercase normalization ('{}' vs '{}').",
                            existing.source_entry,
                            entry_name,
                            existing.canonical_path,
                            canonical_path
                        ));
                    }
                    if existing.kind != kind {
                        return Err(format!(
                            "Bundle entries '{}' and '{}' conflict because '{}' is both a file and a directory.",
                            existing.source_entry, entry_name, canonical_path
                        ));
                    }
                    if is_leaf && existing.explicit_entry {
                        return Err(format!("Bundle contains duplicate entry '{entry_name}'."));
                    }
                    if is_leaf {
                        existing.explicit_entry = true;
                        existing.source_entry = entry_name.to_string();
                    }
                }
                None => {
                    self.paths.insert(
                        portable_key,
                        ArchivePathRecord {
                            canonical_path: canonical_path.clone(),
                            kind,
                            explicit_entry: is_leaf,
                            source_entry: entry_name.to_string(),
                        },
                    );
                }
            }
        }

        Ok(())
    }
}

pub fn inspect_bundle(path: &Path) -> Result<BundleInspection, String> {
    let sha256 = sha256_file(path)?;
    let file = File::open(path).map_err(|e| format!("Unable to open bundle: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid .brlp archive: {e}"))?;
    inspect_archive(&mut archive, sha256)
}

pub fn extract_bundle(path: &Path, target_dir: &Path) -> Result<BundleInspection, String> {
    let inspection = inspect_bundle(path)?;
    if target_dir.exists() {
        fs::remove_dir_all(target_dir)
            .map_err(|e| format!("Unable to clean staging directory: {e}"))?;
    }
    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Unable to create staging directory: {e}"))?;

    let file = File::open(path).map_err(|e| format!("Unable to open bundle: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid .brlp archive: {e}"))?;
    let hashes = read_hashes(&mut archive)?;
    let mut archive_paths = PortableArchivePaths::default();

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Unable to read archive entry: {e}"))?;
        let entry_name = file.name().to_string();
        archive_paths.register(&entry_name, file.is_dir())?;
        if file.is_dir() {
            fs::create_dir_all(target_dir.join(&entry_name))
                .map_err(|e| format!("Unable to create bundle directory: {e}"))?;
            continue;
        }
        if is_symlink(file.unix_mode()) {
            return Err(format!("Bundle entry '{entry_name}' is a symlink"));
        }
        if file.size() > MAX_FILE_SIZE {
            return Err(format!(
                "Bundle entry '{entry_name}' exceeds file size limit"
            ));
        }
        let output_path = target_dir.join(&entry_name);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Unable to create bundle parent directory: {e}"))?;
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|e| format!("Unable to extract bundle entry '{entry_name}': {e}"))?;
        verify_hash(&entry_name, &contents, &hashes)?;
        fs::write(output_path, contents)
            .map_err(|e| format!("Unable to write bundle entry '{entry_name}': {e}"))?;
    }

    make_declared_sidecars_executable(target_dir, &inspection.manifest)?;

    Ok(inspection)
}

#[cfg(unix)]
fn make_declared_sidecars_executable(
    target_dir: &Path,
    manifest: &PluginPackageManifest,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let Some(runtime) = manifest.runtime_v4() else {
        return Ok(());
    };
    for sidecar in &runtime.sidecars {
        if !sidecar_supports_current_platform(sidecar) {
            continue;
        }
        let relative_path = validate_archive_path(&sidecar.bin)?;
        let binary_path = target_dir.join(relative_path);
        let metadata = fs::metadata(&binary_path).map_err(|error| {
            format!(
                "Unable to inspect declared sidecar '{}': {error}",
                sidecar.bin
            )
        })?;
        if !metadata.is_file() {
            return Err(format!(
                "Declared sidecar '{}' is not a regular file after extraction.",
                sidecar.bin
            ));
        }
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(&binary_path, permissions).map_err(|error| {
            format!(
                "Unable to make declared sidecar '{}' executable: {error}",
                sidecar.bin
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_declared_sidecars_executable(
    _target_dir: &Path,
    _manifest: &PluginPackageManifest,
) -> Result<(), String> {
    Ok(())
}

fn inspect_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sha256: String,
) -> Result<BundleInspection, String> {
    let mut file_count = 0usize;
    let mut uncompressed_size = 0u64;
    let mut manifest: Option<PluginPackageManifest> = None;
    let mut hashes_present = false;
    let mut signature_present = false;
    let mut hashes_raw: Option<Vec<u8>> = None;
    let mut signature_raw: Option<String> = None;
    let mut entries = BTreeSet::new();
    let mut archive_paths = PortableArchivePaths::default();

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Unable to inspect bundle entry: {e}"))?;
        let entry_name = file.name().to_string();
        archive_paths.register(&entry_name, file.is_dir())?;
        if is_symlink(file.unix_mode()) {
            return Err(format!("Bundle entry '{entry_name}' is a symlink"));
        }
        uncompressed_size = uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| "Bundle size overflow".to_string())?;
        if uncompressed_size > MAX_UNCOMPRESSED_SIZE {
            return Err("Bundle exceeds uncompressed size limit".to_string());
        }
        if !file.is_dir() {
            file_count += 1;
            entries.insert(entry_name.clone());
        }
        if entry_name == MANIFEST_FILE {
            let mut raw = String::new();
            file.read_to_string(&mut raw)
                .map_err(|e| format!("Unable to read bundle manifest: {e}"))?;
            let parsed = PluginPackageManifest::parse(&raw)
                .map_err(|e| format!("Bundle manifest is invalid: {e}"))?;
            manifest = Some(parsed);
        } else if entry_name == HASHES_FILE {
            hashes_present = true;
            let mut raw = Vec::new();
            file.read_to_end(&mut raw)
                .map_err(|e| format!("Unable to read {HASHES_FILE}: {e}"))?;
            hashes_raw = Some(raw);
        } else if entry_name == SIGNATURE_FILE {
            signature_present = true;
            let mut raw = String::new();
            file.read_to_string(&mut raw)
                .map_err(|e| format!("Unable to read {SIGNATURE_FILE}: {e}"))?;
            signature_raw = Some(raw);
        }
    }
    let signature = verify_bundle_signature(signature_raw.as_deref(), hashes_raw.as_deref())?;
    let manifest = manifest.ok_or_else(|| format!("Bundle is missing {MANIFEST_FILE}"))?;
    validate_manifest_declared_files(&manifest, &entries)?;

    Ok(BundleInspection {
        manifest,
        hashes_present,
        signature_present,
        signature_verified: signature.verified,
        signature_public_key: signature.public_key,
        file_count,
        uncompressed_size,
        sha256,
    })
}

fn validate_manifest_declared_files(
    manifest: &PluginPackageManifest,
    entries: &BTreeSet<String>,
) -> Result<(), String> {
    if let Some(runtime) = manifest.runtime_v4() {
        if let Some(node) = &runtime.node {
            require_bundle_entry(entries, "runtime.node.entry", &node.entry)?;
        }
        for sidecar in &runtime.sidecars {
            if sidecar_supports_current_platform(sidecar) {
                require_bundle_entry(entries, "runtime.sidecars.bin", &sidecar.bin)?;
            }
        }
    }

    let contributes = manifest.contributes_v4();
    if let Some(settings_schema) = manifest.settings_schema() {
        require_bundle_entry(entries, "contributes.settings.schema", settings_schema)?;
    }
    for service in &contributes.services {
        if let Some(schema) = &service.schema {
            require_bundle_entry(entries, "contributes.services.schema", schema)?;
        }
    }
    for extension_point in &contributes.extension_points {
        if let Some(schema) = &extension_point.schema {
            require_bundle_entry(entries, "contributes.extensionPoints.schema", schema)?;
        }
    }
    for contribution in &contributes.contributions {
        if let Some(data_schema) = &contribution.data_schema {
            require_bundle_entry(entries, "contributes.contributions.dataSchema", data_schema)?;
        }
    }
    for resource in &contributes.resources {
        if let Some(path) = &resource.path {
            require_bundle_entry(entries, "contributes.resources.path", path)?;
        }
        for path in &resource.paths {
            require_bundle_entry(entries, "contributes.resources.paths", path)?;
        }
    }
    for webview in &contributes.webviews {
        require_bundle_entry(entries, "contributes.webviews.entry", &webview.entry)?;
    }

    Ok(())
}

fn require_bundle_entry(
    entries: &BTreeSet<String>,
    field: &str,
    relative_path: &str,
) -> Result<(), String> {
    validate_archive_path(relative_path)?;
    if entries.contains(relative_path) {
        return Ok(());
    }
    Err(format!(
        "Bundle manifest declares {field} '{relative_path}', but the file is missing from the bundle."
    ))
}

fn sidecar_supports_current_platform(sidecar: &super::manifest::PluginRuntimeSidecarV4) -> bool {
    sidecar.platforms.is_empty()
        || sidecar
            .platforms
            .iter()
            .any(|candidate| candidate == current_bundle_platform())
}

fn current_bundle_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("windows", "x86_64") => "windows-x64",
        _ => "unknown",
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Unable to open bundle: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("Unable to read bundle: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn read_hashes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Option<BundleHashes>, String> {
    match archive.by_name(HASHES_FILE) {
        Ok(mut file) => {
            let mut raw = String::new();
            file.read_to_string(&mut raw)
                .map_err(|e| format!("Unable to read {HASHES_FILE}: {e}"))?;
            let hashes: BundleHashes = serde_json::from_str(&raw)
                .map_err(|e| format!("{HASHES_FILE} is invalid JSON: {e}"))?;
            Ok(Some(hashes))
        }
        Err(_) => Ok(None),
    }
}

fn verify_hash(
    entry_name: &str,
    contents: &[u8],
    hashes: &Option<BundleHashes>,
) -> Result<(), String> {
    let Some(hashes) = hashes else {
        return Ok(());
    };
    if entry_name == HASHES_FILE || entry_name == SIGNATURE_FILE {
        return Ok(());
    }
    let expected = hashes
        .files
        .get(entry_name)
        .ok_or_else(|| format!("{HASHES_FILE} does not contain hash for '{entry_name}'"))?;
    let actual = hex::encode(Sha256::digest(contents));
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(format!("Hash mismatch for bundle entry '{entry_name}'"));
    }
    Ok(())
}

fn verify_bundle_signature(
    signature_raw: Option<&str>,
    hashes_raw: Option<&[u8]>,
) -> Result<BundleSignatureInspection, String> {
    let Some(signature_raw) = signature_raw else {
        return Ok(BundleSignatureInspection {
            present: false,
            verified: false,
            public_key: None,
        });
    };
    let hashes_raw =
        hashes_raw.ok_or_else(|| format!("{SIGNATURE_FILE} requires {HASHES_FILE}"))?;
    let signature: BundleSignatureFile = serde_json::from_str(signature_raw)
        .map_err(|e| format!("{SIGNATURE_FILE} is invalid JSON: {e}"))?;
    let public_key = signature.public_key.clone();
    if !signature.algorithm.eq_ignore_ascii_case("ed25519") {
        return Err(format!("{SIGNATURE_FILE} uses unsupported algorithm"));
    }
    if signature.signed_file.as_deref().unwrap_or(HASHES_FILE) != HASHES_FILE {
        return Err(format!("{SIGNATURE_FILE} must sign {HASHES_FILE}"));
    }
    let public_key_bytes = BASE64_STANDARD
        .decode(&signature.public_key)
        .map_err(|e| format!("{SIGNATURE_FILE} publicKey is invalid base64: {e}"))?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| format!("{SIGNATURE_FILE} publicKey must contain 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|e| format!("{SIGNATURE_FILE} publicKey is invalid: {e}"))?;
    let signature_bytes = BASE64_STANDARD
        .decode(&signature.signature)
        .map_err(|e| format!("{SIGNATURE_FILE} signature is invalid base64: {e}"))?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| format!("{SIGNATURE_FILE} signature must contain 64 bytes"))?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|e| format!("{SIGNATURE_FILE} signature is invalid: {e}"))?;
    verifying_key
        .verify(hashes_raw, &signature)
        .map_err(|_| format!("{SIGNATURE_FILE} verification failed"))?;

    Ok(BundleSignatureInspection {
        present: true,
        verified: true,
        public_key: Some(public_key),
    })
}

pub fn validate_archive_path(value: &str) -> Result<PathBuf, String> {
    portable_path_components(value)?;
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(format!(
            "Bundle entry '{value}' must stay inside the plugin"
        ));
    }
    Ok(path.to_path_buf())
}

fn portable_path_components(value: &str) -> Result<Vec<&str>, String> {
    if value.trim().is_empty() {
        return Err("Bundle entry path cannot be empty".to_string());
    }
    if value.contains('\\') {
        return Err(format!(
            "Bundle entry '{value}' must use '/' as its path separator"
        ));
    }

    let path_without_directory_suffix = value.strip_suffix('/').unwrap_or(value);
    if path_without_directory_suffix.is_empty() {
        return Err(format!(
            "Bundle entry '{value}' must stay inside the plugin"
        ));
    }

    let components: Vec<_> = path_without_directory_suffix.split('/').collect();
    for component in &components {
        if component.is_empty() || *component == "." || *component == ".." {
            return Err(format!(
                "Bundle entry '{value}' must use non-empty normalized path segments"
            ));
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(format!(
                "Bundle entry '{value}' contains a path segment ending with a dot or space"
            ));
        }
        if component.chars().any(|character| {
            character <= '\u{1f}' || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        }) {
            return Err(format!(
                "Bundle entry '{value}' contains characters that are invalid in Windows paths"
            ));
        }
        if is_windows_reserved_name(component) {
            return Err(format!(
                "Bundle entry '{value}' uses Windows-reserved path segment '{component}'"
            ));
        }
    }

    Ok(components)
}

fn validate_reserved_root_entry(components: &[&str], is_dir: bool) -> Result<(), String> {
    if components.len() != 1 {
        return Ok(());
    }
    let entry_name = components[0];
    for reserved_name in [MANIFEST_FILE, HASHES_FILE, SIGNATURE_FILE] {
        if entry_name.eq_ignore_ascii_case(reserved_name) {
            if entry_name != reserved_name {
                return Err(format!(
                    "Reserved bundle entry '{entry_name}' must use exact canonical case '{reserved_name}'."
                ));
            }
            if is_dir {
                return Err(format!(
                    "Reserved bundle entry '{reserved_name}' must be a file."
                ));
            }
        }
    }
    Ok(())
}

fn is_windows_reserved_name(component: &str) -> bool {
    let basename = component
        .split_once('.')
        .map_or(component, |(basename, _)| basename)
        .to_ascii_uppercase();
    matches!(basename.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || basename
            .strip_prefix("COM")
            .or_else(|| basename.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
}

fn is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::FileOptions;

    fn valid_manifest() -> String {
        serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.bundle",
            "name": "Bundle",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension-host.js"
                },
                "sidecars": []
            },
            "contributes": {},
        })
        .to_string()
    }

    fn write_bundle(path: &Path, entries: &[(&str, &str)]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: FileOptions<'_, ()> = FileOptions::default();
        for (name, contents) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn valid_bundle_entries(manifest: &str) -> Vec<(&str, &str)> {
        vec![
            (MANIFEST_FILE, manifest),
            (
                "dist/extension-host.js",
                "export default { activate() {} };",
            ),
        ]
    }

    fn signed_entries() -> Vec<(String, String)> {
        let manifest = valid_manifest();
        let runtime = "export default { activate() {} };".to_string();
        let hashes = serde_json::to_string_pretty(&serde_json::json!({
            "files": {
                MANIFEST_FILE: hex::encode(Sha256::digest(manifest.as_bytes())),
                "dist/extension-host.js": hex::encode(Sha256::digest(runtime.as_bytes()))
            }
        }))
        .unwrap();
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let signature = signing_key.sign(hashes.as_bytes());
        let signature = serde_json::to_string_pretty(&serde_json::json!({
            "algorithm": "ed25519",
            "publicKey": BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes()),
            "signature": BASE64_STANDARD.encode(signature.to_bytes()),
            "signedFile": HASHES_FILE
        }))
        .unwrap();
        vec![
            (MANIFEST_FILE.to_string(), manifest),
            ("dist/extension-host.js".to_string(), runtime),
            (HASHES_FILE.to_string(), hashes),
            (SIGNATURE_FILE.to_string(), signature),
        ]
    }

    #[test]
    fn inspects_valid_bundle() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("valid.brlp");
        let manifest = valid_manifest();
        write_bundle(&bundle_path, &valid_bundle_entries(&manifest));

        let inspection = inspect_bundle(&bundle_path).unwrap();
        assert_eq!(inspection.manifest.id(), "com.example.bundle");
        assert_eq!(inspection.file_count, 2);
    }

    #[test]
    fn rejects_legacy_manifest_in_bundle() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("legacy.brlp");
        let manifest = serde_json::json!({
            "schema": "bakingrl.plugin/legacy",
            "id": "com.example.bundle-legacy",
            "name": "Bundle legacy",
            "version": "1.0.0",
            "contributes": {}
        })
        .to_string();
        write_bundle(
            &bundle_path,
            &[
                (MANIFEST_FILE, &manifest),
                (
                    "dist/extension-host.js",
                    "export default { activate() {} };",
                ),
            ],
        );

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("manifest field 'schema'"));
    }

    #[test]
    fn verifies_signed_bundle() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("signed.brlp");
        let entries = signed_entries();
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, contents)| (name.as_str(), contents.as_str()))
            .collect();
        write_bundle(&bundle_path, &entries);

        let inspection = inspect_bundle(&bundle_path).unwrap();
        assert!(inspection.hashes_present);
        assert!(inspection.signature_present);
        assert!(inspection.signature_verified);
        assert!(inspection.signature_public_key.is_some());
    }

    #[test]
    fn rejects_bad_signature() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("bad-signature.brlp");
        let mut entries = signed_entries();
        let (_, signature) = entries
            .iter_mut()
            .find(|(name, _)| name == SIGNATURE_FILE)
            .unwrap();
        *signature = signature.replace("ed25519", "ed25519-bad");
        let entries: Vec<_> = entries
            .iter()
            .map(|(name, contents)| (name.as_str(), contents.as_str()))
            .collect();
        write_bundle(&bundle_path, &entries);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("unsupported algorithm"));
    }

    #[test]
    fn rejects_path_traversal_bundle_entry() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("bad.brlp");
        write_bundle(
            &bundle_path,
            &[(MANIFEST_FILE, &valid_manifest()), ("../outside.js", "bad")],
        );

        assert!(inspect_bundle(&bundle_path).is_err());
    }

    #[test]
    fn rejects_case_insensitive_file_collisions() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("case-collision.brlp");
        let manifest = valid_manifest();
        let mut entries = valid_bundle_entries(&manifest);
        entries.extend([("assets/Index.js", "first"), ("assets/index.js", "second")]);
        write_bundle(&bundle_path, &entries);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("Unicode uppercase normalization"));
        assert!(error.contains("assets/Index.js"));
        assert!(error.contains("assets/index.js"));
    }

    #[test]
    fn rejects_case_insensitive_implicit_directory_collisions() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("directory-case-collision.brlp");
        let manifest = valid_manifest();
        let mut entries = valid_bundle_entries(&manifest);
        entries.extend([
            ("Assets/first.txt", "first"),
            ("assets/second.txt", "second"),
        ]);
        write_bundle(&bundle_path, &entries);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("Unicode uppercase normalization"));
        assert!(error.contains("Assets"));
        assert!(error.contains("assets"));
    }

    #[test]
    fn rejects_unicode_uppercase_file_collisions() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("unicode-case-collision.brlp");
        let manifest = valid_manifest();
        let mut entries = valid_bundle_entries(&manifest);
        entries.extend([("assets/É.js", "first"), ("assets/é.js", "second")]);
        write_bundle(&bundle_path, &entries);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("Unicode uppercase normalization"));
        assert!(error.contains("assets/É.js"));
        assert!(error.contains("assets/é.js"));
    }

    #[test]
    fn rejects_windows_unicode_sigma_file_collisions() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("unicode-sigma-collision.brlp");
        let manifest = valid_manifest();
        let mut entries = valid_bundle_entries(&manifest);
        entries.extend([("assets/σ.js", "first"), ("assets/ς.js", "second")]);
        write_bundle(&bundle_path, &entries);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("Unicode uppercase normalization"));
        assert!(error.contains("assets/σ.js"));
        assert!(error.contains("assets/ς.js"));
    }

    #[test]
    fn rejects_file_directory_prefix_conflicts_regardless_of_entry_order() {
        let dir = tempdir().unwrap();
        let manifest = valid_manifest();
        let conflict_orders = [
            [("assets", "file"), ("assets/icon.png", "icon")],
            [("assets/icon.png", "icon"), ("assets", "file")],
        ];

        for (index, conflict_entries) in conflict_orders.iter().enumerate() {
            let bundle_path = dir.path().join(format!("file-directory-{index}.brlp"));
            let mut entries = valid_bundle_entries(&manifest);
            entries.extend(conflict_entries.iter().copied());
            write_bundle(&bundle_path, &entries);

            let error = inspect_bundle(&bundle_path).unwrap_err();
            assert!(error.contains("both a file and a directory"));
            assert!(error.contains("assets"));
        }
    }

    #[test]
    fn rejects_noncanonical_reserved_root_entry_names() {
        let dir = tempdir().unwrap();
        let manifest = valid_manifest();
        for (index, (entry_name, canonical_name)) in [
            ("BakingRL.Plugin.Json", MANIFEST_FILE),
            ("Manifest.Hashes.Json", HASHES_FILE),
            ("Signature.Ed25519", SIGNATURE_FILE),
        ]
        .iter()
        .enumerate()
        {
            let bundle_path = dir.path().join(format!("reserved-case-{index}.brlp"));
            let mut entries = valid_bundle_entries(&manifest);
            entries.push((entry_name, "{}"));
            write_bundle(&bundle_path, &entries);

            let error = inspect_bundle(&bundle_path).unwrap_err();
            assert!(error.contains("must use exact canonical case"));
            assert!(error.contains(canonical_name));
        }
    }

    #[test]
    fn rejects_nonportable_windows_archive_paths() {
        let dir = tempdir().unwrap();
        let manifest = valid_manifest();
        for (index, entry_name) in [
            "assets\\index.js",
            "assets/index.js.",
            "assets/index.js ",
            "assets//index.js",
            "C:/index.js",
            "assets/CON.txt",
        ]
        .iter()
        .enumerate()
        {
            let bundle_path = dir.path().join(format!("windows-path-{index}.brlp"));
            let mut entries = valid_bundle_entries(&manifest);
            entries.push((entry_name, "invalid"));
            write_bundle(&bundle_path, &entries);

            let error = inspect_bundle(&bundle_path).unwrap_err();
            assert!(
                error.contains("Bundle entry"),
                "unexpected validation error for {entry_name:?}: {error}"
            );
        }
    }

    #[test]
    fn rejects_windows_reserved_names_with_superscript_digits() {
        let dir = tempdir().unwrap();
        let manifest = valid_manifest();
        for (index, entry_name) in [
            "assets/COM¹.txt",
            "assets/COM².txt",
            "assets/COM³.txt",
            "assets/LPT¹.txt",
            "assets/LPT².txt",
            "assets/LPT³.txt",
        ]
        .iter()
        .enumerate()
        {
            let bundle_path = dir.path().join(format!("windows-superscript-{index}.brlp"));
            let mut entries = valid_bundle_entries(&manifest);
            entries.push((entry_name, "invalid"));
            write_bundle(&bundle_path, &entries);

            let error = inspect_bundle(&bundle_path).unwrap_err();
            assert!(error.contains("Windows-reserved path segment"));
            assert!(error.contains(entry_name));
        }
    }

    #[test]
    fn rejects_bundle_missing_declared_manifest_file() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("missing-declared-file.brlp");
        let manifest = valid_manifest();
        write_bundle(&bundle_path, &[(MANIFEST_FILE, &manifest)]);

        let error = inspect_bundle(&bundle_path).unwrap_err();
        assert!(error.contains("runtime.node.entry"));
        assert!(error.contains("dist/extension-host.js"));
    }

    #[test]
    fn inspect_bundle_only_requires_sidecars_for_current_platform() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("foreign-sidecar.brlp");
        let manifest = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.sidecar",
            "name": "Sidecar",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "foreign",
                        "bin": "bin/foreign-sidecar",
                        "platforms": ["definitely-not-this-platform"],
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {}
        })
        .to_string();
        write_bundle(&bundle_path, &[(MANIFEST_FILE, &manifest)]);

        inspect_bundle(&bundle_path).unwrap();

        let current_bundle_path = dir.path().join("current-sidecar.brlp");
        let manifest = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.sidecar",
            "name": "Sidecar",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "current",
                        "bin": "bin/current-sidecar",
                        "platforms": [current_bundle_platform()],
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {}
        })
        .to_string();
        write_bundle(&current_bundle_path, &[(MANIFEST_FILE, &manifest)]);

        let error = inspect_bundle(&current_bundle_path).unwrap_err();
        assert!(error.contains("runtime.sidecars.bin"));
        assert!(error.contains("bin/current-sidecar"));
    }

    #[test]
    fn extracts_valid_bundle_to_target() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("valid.brlp");
        let target = dir.path().join("target");
        let manifest = valid_manifest();
        write_bundle(&bundle_path, &valid_bundle_entries(&manifest));

        extract_bundle(&bundle_path, &target).unwrap();
        assert!(target.join(MANIFEST_FILE).exists());
        assert!(target.join("dist/extension-host.js").exists());
    }

    #[cfg(unix)]
    #[test]
    fn extraction_marks_current_platform_sidecars_executable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("sidecar.brlp");
        let target = dir.path().join("target");
        let manifest = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.sidecar-extraction",
            "name": "Sidecar extraction",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "worker",
                        "bin": "bin/worker",
                        "platforms": [current_bundle_platform()],
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {}
        })
        .to_string();
        write_bundle(
            &bundle_path,
            &[(MANIFEST_FILE, &manifest), ("bin/worker", "sidecar bytes")],
        );

        extract_bundle(&bundle_path, &target).unwrap();

        let sidecar_mode = fs::metadata(target.join("bin/worker"))
            .unwrap()
            .permissions()
            .mode();
        let manifest_mode = fs::metadata(target.join(MANIFEST_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(sidecar_mode & 0o111, 0o111);
        assert_eq!(manifest_mode & 0o111, 0);
    }
}
