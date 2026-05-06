use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::manifest::PluginPackageManifestV2;

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
    pub manifest: PluginPackageManifestV2,
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

pub fn inspect_bundle(path: &Path) -> Result<BundleInspection, String> {
    let bytes = fs::read(path).map_err(|e| format!("Unable to read bundle: {e}"))?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid .brlp archive: {e}"))?;
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

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Unable to read archive entry: {e}"))?;
        let entry_name = file.name().to_string();
        validate_archive_path(&entry_name)?;
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

    Ok(inspection)
}

fn inspect_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sha256: String,
) -> Result<BundleInspection, String> {
    let mut file_count = 0usize;
    let mut uncompressed_size = 0u64;
    let mut manifest: Option<PluginPackageManifestV2> = None;
    let mut hashes_present = false;
    let mut signature_present = false;
    let mut hashes_raw: Option<Vec<u8>> = None;
    let mut signature_raw: Option<String> = None;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Unable to inspect bundle entry: {e}"))?;
        let entry_name = file.name().to_string();
        validate_archive_path(&entry_name)?;
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
        }
        if entry_name == MANIFEST_FILE {
            let mut raw = String::new();
            file.read_to_string(&mut raw)
                .map_err(|e| format!("Unable to read bundle manifest: {e}"))?;
            let parsed: PluginPackageManifestV2 = serde_json::from_str(&raw)
                .map_err(|e| format!("Bundle manifest is invalid JSON: {e}"))?;
            parsed.validate()?;
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

    Ok(BundleInspection {
        manifest: manifest.ok_or_else(|| format!("Bundle is missing {MANIFEST_FILE}"))?,
        hashes_present,
        signature_present,
        signature_verified: signature.verified,
        signature_public_key: signature.public_key,
        file_count,
        uncompressed_size,
        sha256,
    })
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
    if value.trim().is_empty() {
        return Err("Bundle entry path cannot be empty".to_string());
    }
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
            "schema": "bakingrl.plugin/2",
            "id": "com.example.bundle",
            "name": "Bundle",
            "version": "1.0.0",
            "exports": {
                "visuals": {
                    "scoreboard": {
                        "entry": "dist/visuals/scoreboard.js",
                        "defaultSize": [600, 90]
                    }
                }
            }
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

    fn signed_entries() -> Vec<(String, String)> {
        let manifest = valid_manifest();
        let visual = "export default { mount() {} };".to_string();
        let hashes = serde_json::to_string_pretty(&serde_json::json!({
            "files": {
                MANIFEST_FILE: hex::encode(Sha256::digest(manifest.as_bytes())),
                "dist/visuals/scoreboard.js": hex::encode(Sha256::digest(visual.as_bytes()))
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
            ("dist/visuals/scoreboard.js".to_string(), visual),
            (HASHES_FILE.to_string(), hashes),
            (SIGNATURE_FILE.to_string(), signature),
        ]
    }

    #[test]
    fn inspects_valid_bundle() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("valid.brlp");
        write_bundle(
            &bundle_path,
            &[
                (MANIFEST_FILE, &valid_manifest()),
                (
                    "dist/visuals/scoreboard.js",
                    "export default { mount() {} };",
                ),
            ],
        );

        let inspection = inspect_bundle(&bundle_path).unwrap();
        assert_eq!(inspection.manifest.id, "com.example.bundle");
        assert_eq!(inspection.file_count, 2);
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
    fn extracts_valid_bundle_to_target() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("valid.brlp");
        let target = dir.path().join("target");
        write_bundle(
            &bundle_path,
            &[
                (MANIFEST_FILE, &valid_manifest()),
                (
                    "dist/visuals/scoreboard.js",
                    "export default { mount() {} };",
                ),
            ],
        );

        extract_bundle(&bundle_path, &target).unwrap();
        assert!(target.join(MANIFEST_FILE).exists());
        assert!(target.join("dist/visuals/scoreboard.js").exists());
    }
}
