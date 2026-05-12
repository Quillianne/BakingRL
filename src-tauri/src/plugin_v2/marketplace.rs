use std::fs;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::permissions::EffectivePackagePermissionsV2;

pub const OFFICIAL_MARKETPLACE_URL: &str =
    "https://quillianne.github.io/BakingRLMarketplace/marketplace.json";

// Replace this public key before enabling the official hosted marketplace in a
// release build. During development, BAKINGRL_MARKETPLACE_PUBLIC_KEY can supply
// the trusted key without changing source.
const OFFICIAL_MARKETPLACE_PUBLIC_KEY: &str = "";
const MAX_MARKETPLACE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LISTING_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceIndex {
    pub schema: String,
    pub generated_at: String,
    #[serde(default)]
    pub sections: MarketplaceSections,
    #[serde(default)]
    pub developers: Vec<MarketplaceDeveloper>,
    #[serde(default)]
    pub packages: Vec<MarketplacePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceSections {
    #[serde(default)]
    pub recommended: Vec<String>,
    #[serde(default, rename = "new")]
    pub new_packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDeveloper {
    pub id: String,
    pub name: String,
    pub verified: bool,
    #[serde(default)]
    pub package_signing_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePackage {
    pub schema: String,
    pub id: String,
    pub developer_id: String,
    pub repo: String,
    pub listing_url: String,
    #[serde(default)]
    pub approved_versions: Vec<MarketplaceApprovedVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceApprovedVersion {
    pub version: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub signature_public_key: String,
    pub runtime_api: Option<String>,
    #[serde(default)]
    pub revoked: bool,
    pub review: MarketplaceReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceReview {
    pub status: String,
    pub reviewed_at: String,
    pub permissions: EffectivePackagePermissionsV2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSignature {
    pub schema: String,
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
    pub signed_file: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceListing {
    pub schema: String,
    pub package_id: String,
    pub display_name: String,
    pub short_description: String,
    pub long_description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub repo: String,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    #[serde(default)]
    pub screenshots: Vec<MarketplaceScreenshot>,
    #[serde(default)]
    pub links: MarketplaceListingLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceScreenshot {
    pub url: String,
    pub alt: Option<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceListingLinks {
    pub docs: Option<String>,
    pub support: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceCatalog {
    pub generated_at: String,
    pub sections: MarketplaceSections,
    pub packages: Vec<MarketplaceCatalogPackage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceCatalogPackage {
    pub id: String,
    pub developer_id: String,
    pub developer_name: Option<String>,
    pub repo: String,
    pub listing_url: String,
    pub listing: Option<MarketplaceListing>,
    pub listing_error: Option<String>,
    pub approved_versions: Vec<MarketplaceApprovedVersion>,
}

pub fn trusted_marketplace_public_keys() -> Vec<String> {
    let mut keys = Vec::new();
    if !OFFICIAL_MARKETPLACE_PUBLIC_KEY.trim().is_empty() {
        keys.push(OFFICIAL_MARKETPLACE_PUBLIC_KEY.trim().to_string());
    }
    if let Ok(value) = std::env::var("BAKINGRL_MARKETPLACE_PUBLIC_KEY") {
        keys.extend(
            value
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    keys
}

pub async fn fetch_verified_marketplace_index(index_url: &str) -> Result<MarketplaceIndex, String> {
    let signature_url = signature_url_for_index(index_url)?;
    let index_raw = download_limited(index_url, MAX_MARKETPLACE_BYTES).await?;
    let signature_raw = download_limited(&signature_url, MAX_LISTING_BYTES).await?;
    verify_marketplace_index(
        &index_raw,
        &signature_raw,
        &trusted_marketplace_public_keys(),
    )
}

pub fn read_cached_marketplace_index(path: &Path) -> Result<MarketplaceIndex, String> {
    let raw = fs::read(path).map_err(|e| format!("Unable to read marketplace cache: {e}"))?;
    let index: MarketplaceIndex =
        serde_json::from_slice(&raw).map_err(|e| format!("Marketplace cache is invalid: {e}"))?;
    validate_index_shape(&index)?;
    Ok(index)
}

pub fn write_marketplace_cache(path: &Path, index: &MarketplaceIndex) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Unable to create marketplace cache directory: {e}"))?;
    }
    let raw = serde_json::to_vec_pretty(index)
        .map_err(|e| format!("Unable to serialize marketplace cache: {e}"))?;
    fs::write(path, raw).map_err(|e| format!("Unable to write marketplace cache: {e}"))
}

pub async fn catalog_for_index(index: MarketplaceIndex) -> MarketplaceCatalog {
    let developers = index.developers;
    let mut packages = Vec::new();
    for package in index.packages {
        let developer_name = developers
            .iter()
            .find(|developer| developer.id == package.developer_id)
            .map(|developer| developer.name.clone());
        let (listing, listing_error) = match fetch_listing(&package.listing_url).await {
            Ok(listing) => (Some(listing), None),
            Err(error) => (None, Some(error)),
        };
        packages.push(MarketplaceCatalogPackage {
            developer_name,
            id: package.id,
            developer_id: package.developer_id,
            repo: package.repo,
            listing_url: package.listing_url,
            listing,
            listing_error,
            approved_versions: package.approved_versions,
        });
    }

    MarketplaceCatalog {
        generated_at: index.generated_at,
        sections: index.sections,
        packages,
    }
}

pub fn find_marketplace_version<'a>(
    index: &'a MarketplaceIndex,
    package_id: &str,
    version: &str,
) -> Result<(&'a MarketplacePackage, &'a MarketplaceApprovedVersion), String> {
    let package = index
        .packages
        .iter()
        .find(|package| package.id == package_id)
        .ok_or_else(|| format!("Marketplace package '{package_id}' was not found"))?;
    let version = package
        .approved_versions
        .iter()
        .find(|entry| entry.version == version)
        .ok_or_else(|| format!("Marketplace version '{package_id}@{version}' was not found"))?;
    if version.revoked {
        return Err(format!(
            "Marketplace version '{package_id}@{}' is revoked",
            version.version
        ));
    }
    if version.review.status != "approved" {
        return Err(format!(
            "Marketplace version '{package_id}@{}' is not approved",
            version.version
        ));
    }
    Ok((package, version))
}

pub fn developer_allows_key(
    index: &MarketplaceIndex,
    developer_id: &str,
    public_key: &str,
) -> bool {
    index
        .developers
        .iter()
        .find(|developer| developer.id == developer_id && developer.verified)
        .is_some_and(|developer| {
            developer
                .package_signing_keys
                .iter()
                .any(|key| key == public_key)
        })
}

pub fn verify_marketplace_index(
    index_raw: &[u8],
    signature_raw: &[u8],
    trusted_public_keys: &[String],
) -> Result<MarketplaceIndex, String> {
    if trusted_public_keys.is_empty() {
        return Err("No trusted marketplace signing key is configured.".to_string());
    }
    let signature: MarketplaceSignature = serde_json::from_slice(signature_raw)
        .map_err(|e| format!("Marketplace signature is invalid JSON: {e}"))?;
    if signature.schema != "bakingrl.marketplace-signature/1" {
        return Err("Marketplace signature uses an unsupported schema.".to_string());
    }
    if !signature.algorithm.eq_ignore_ascii_case("ed25519") {
        return Err("Marketplace signature uses an unsupported algorithm.".to_string());
    }
    if signature.signed_file != "marketplace.json" {
        return Err("Marketplace signature must sign marketplace.json.".to_string());
    }
    if !trusted_public_keys
        .iter()
        .any(|key| key == &signature.public_key)
    {
        return Err("Marketplace signature public key is not trusted.".to_string());
    }
    if let Some(expected_sha256) = &signature.sha256 {
        let actual_sha256 = hex::encode(Sha256::digest(index_raw));
        if !expected_sha256.eq_ignore_ascii_case(&actual_sha256) {
            return Err(
                "Marketplace index SHA-256 does not match its signature metadata.".to_string(),
            );
        }
    }
    let public_key_bytes = BASE64_STANDARD
        .decode(&signature.public_key)
        .map_err(|e| format!("Marketplace public key is invalid base64: {e}"))?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "Marketplace public key must contain 32 bytes".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|e| format!("Marketplace public key is invalid: {e}"))?;
    let signature_bytes = BASE64_STANDARD
        .decode(&signature.signature)
        .map_err(|e| format!("Marketplace signature is invalid base64: {e}"))?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| "Marketplace signature must contain 64 bytes".to_string())?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|e| format!("Marketplace signature is invalid: {e}"))?;
    verifying_key
        .verify(index_raw, &signature)
        .map_err(|_| "Marketplace signature verification failed".to_string())?;

    let index: MarketplaceIndex = serde_json::from_slice(index_raw)
        .map_err(|e| format!("Marketplace index is invalid JSON: {e}"))?;
    validate_index_shape(&index)?;
    Ok(index)
}

fn validate_index_shape(index: &MarketplaceIndex) -> Result<(), String> {
    if index.schema != "bakingrl.marketplace/1" {
        return Err("Marketplace index uses an unsupported schema.".to_string());
    }
    Ok(())
}

async fn fetch_listing(url: &str) -> Result<MarketplaceListing, String> {
    let raw = download_limited(url, MAX_LISTING_BYTES).await?;
    let listing: MarketplaceListing = serde_json::from_slice(&raw)
        .map_err(|e| format!("Marketplace listing is invalid JSON: {e}"))?;
    if listing.schema != "bakingrl.plugin-listing/1" {
        return Err("Marketplace listing uses an unsupported schema.".to_string());
    }
    Ok(listing)
}

async fn download_limited(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid marketplace URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("Marketplace URLs must use HTTPS.".to_string());
    }
    let response = reqwest::get(parsed)
        .await
        .map_err(|e| format!("Unable to download marketplace resource: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Marketplace resource download failed with HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err("Marketplace resource is too large.".to_string());
    }
    let mut downloaded = 0u64;
    let mut output = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Unable to read marketplace response: {e}"))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "Marketplace resource size overflow.".to_string())?;
        if downloaded > max_bytes {
            return Err("Marketplace resource is too large.".to_string());
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn signature_url_for_index(index_url: &str) -> Result<String, String> {
    let parsed =
        reqwest::Url::parse(index_url).map_err(|e| format!("Invalid marketplace URL: {e}"))?;
    if parsed.path().ends_with("marketplace.json") {
        Ok(index_url.trim_end_matches("marketplace.json").to_string() + "marketplace.sig")
    } else {
        Ok(format!("{index_url}.sig"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_signed_index() {
        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let public_key = BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());
        let index = br#"{
  "schema": "bakingrl.marketplace/1",
  "generatedAt": "2026-05-12T00:00:00Z",
  "sections": { "recommended": [], "new": [] },
  "developers": [],
  "packages": []
}"#;
        let signature = signing_key.sign(index);
        let signature = serde_json::to_vec(&serde_json::json!({
            "schema": "bakingrl.marketplace-signature/1",
            "algorithm": "ed25519",
            "publicKey": public_key,
            "signature": BASE64_STANDARD.encode(signature.to_bytes()),
            "signedFile": "marketplace.json"
        }))
        .unwrap();

        let verified = verify_marketplace_index(index, &signature, &[public_key]).unwrap();
        assert_eq!(verified.schema, "bakingrl.marketplace/1");
    }
}
