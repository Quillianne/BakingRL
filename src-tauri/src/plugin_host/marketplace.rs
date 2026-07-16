use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

const MARKETPLACE_BASE_URL: &str = "https://quillianne.github.io/BakingRLMarketplace";
const MARKETPLACE_CATALOGUE_FILE: &str = "marketplace.json";
const MARKETPLACE_SIGNATURE_FILE: &str = "marketplace.sig";
const MARKETPLACE_SCHEMA: &str = "bakingrl.marketplace/2";
const MARKETPLACE_SIGNATURE_SCHEMA: &str = "bakingrl.marketplace-signature/2";
const MARKETPLACE_STATE_SCHEMA: &str = "bakingrl.marketplace-state/1";
const MARKETPLACE_CACHE_SCHEMA: &str = "bakingrl.marketplace-cache/1";
const MARKETPLACE_CACHE_FILE: &str = "verified-catalogue.json";
const CLOCK_TOLERANCE: Duration = Duration::hours(24);
const MAX_CATALOGUE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 64 * 1024;

const TRUSTED_ROOTS: &[TrustedMarketplaceRoot<'static>] = &[TrustedMarketplaceRoot {
    key_id: "marketplace-root-1",
    public_key: "gWpy0Yiz0Jn6nCUK38WOqV9WQByCIXveRrG94zbLjeo=",
}];

#[derive(Debug, Clone, Copy)]
struct TrustedMarketplaceRoot<'a> {
    key_id: &'a str,
    public_key: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceCatalogue {
    pub schema: String,
    pub sequence: u64,
    pub generated_at: String,
    pub expires_at: String,
    pub sections: MarketplaceSections,
    pub developers: Vec<MarketplaceDeveloper>,
    pub packages: Vec<MarketplacePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceSections {
    pub recommended: Vec<String>,
    pub new: Vec<String>,
    pub first_run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceDeveloper {
    pub id: String,
    pub name: String,
    pub kind: MarketplaceDeveloperKind,
    pub verification: MarketplaceDeveloperVerification,
    pub signing_keys: Vec<MarketplaceSigningKey>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceDeveloperKind {
    Individual,
    Organization,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceDeveloperVerification {
    Unverified,
    Verified,
    Official,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceSigningKey {
    pub id: String,
    pub algorithm: String,
    pub public_key: String,
    pub status: MarketplaceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation: Option<MarketplaceRevocation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceStatus {
    Active,
    Yanked,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceRevocation {
    pub reason: String,
    pub revoked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplacePackage {
    pub schema: String,
    pub id: String,
    pub developer_id: String,
    pub status: MarketplaceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation: Option<MarketplaceRevocation>,
    pub repo: String,
    pub listing: MarketplaceListing,
    pub versions: Vec<MarketplacePackageVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceListing {
    pub source_url: String,
    pub snapshot_sha256: String,
    pub snapshot: MarketplaceListingSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceListingSnapshot {
    pub schema: String,
    pub package_id: String,
    pub display_name: String,
    pub short_description: String,
    pub long_description: String,
    pub tags: Vec<String>,
    pub repo: String,
    pub media: MarketplaceMedia,
    pub links: MarketplaceLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceMedia {
    pub icon: Option<MarketplaceMediaAsset>,
    pub banner: Option<MarketplaceMediaAsset>,
    pub screenshots: Vec<MarketplaceScreenshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceMediaAsset {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceScreenshot {
    pub url: String,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceLinks {
    pub docs: String,
    pub support: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplacePackageVersion {
    pub version: String,
    pub status: MarketplaceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation: Option<MarketplaceRevocation>,
    pub channel: MarketplaceChannel,
    pub runtime_api: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_bakingrl_version: Option<String>,
    pub runtime: MarketplaceRuntime,
    pub dependencies: Vec<MarketplaceDependency>,
    pub permissions: MarketplacePermissions,
    pub native_capabilities: MarketplaceNativeCapabilities,
    pub artifacts: Vec<MarketplaceArtifact>,
    pub reviewed_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceChannel {
    Stable,
    Beta,
    Nightly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceRuntime {
    pub node: bool,
    pub sidecars: Vec<MarketplaceRuntimeSidecar>,
    pub webviews: Vec<MarketplaceRuntimeWebview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceRuntimeSidecar {
    pub id: String,
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceRuntimeWebview {
    pub id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceDependency {
    pub package_id: String,
    pub version: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplacePermissions {
    pub bus: MarketplaceBusPermissions,
    pub registry: MarketplaceReadWritePermissions,
    pub network: MarketplaceNetworkPermissions,
    pub storage: MarketplaceReadWritePermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceBusPermissions {
    pub read: Vec<String>,
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceReadWritePermissions {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceNetworkPermissions {
    pub http: Vec<MarketplaceNetworkEndpoint>,
    pub websocket: Vec<MarketplaceNetworkEndpoint>,
    pub listen: Vec<MarketplaceListenEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceNetworkEndpoint {
    pub scheme: String,
    pub host: String,
    pub ports: MarketplacePorts,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceListenEndpoint {
    pub transport: String,
    pub host: String,
    pub ports: MarketplacePorts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MarketplacePorts {
    Any(String),
    Ports(Vec<u16>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceNativeCapabilities {
    pub node: Option<MarketplaceNativeNode>,
    pub sidecars: Vec<MarketplaceNativeItem>,
    pub surfaces: Vec<MarketplaceNativeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceNativeNode {
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceNativeItem {
    pub id: String,
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketplaceArtifact {
    pub platform: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub signing_key_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSnapshot {
    pub catalogue: MarketplaceCatalogue,
    pub source: MarketplaceSnapshotSource,
    pub expired: bool,
    pub installable: bool,
    pub first_run_pending: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceSnapshotSource {
    Network,
    Cache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceSignatureEnvelope {
    schema: String,
    algorithm: String,
    key_id: String,
    signed_file: String,
    sha256: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceLocalState {
    schema: String,
    max_sequence: u64,
    first_run_completed: bool,
    trusted_publishers: Vec<TrustedPublisher>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceCacheFile {
    schema: String,
    catalogue_base64: String,
    signature_base64: String,
}

impl Default for MarketplaceLocalState {
    fn default() -> Self {
        Self {
            schema: MARKETPLACE_STATE_SCHEMA.to_string(),
            max_sequence: 0,
            first_run_completed: false,
            trusted_publishers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustedPublisher {
    developer_id: String,
    key_fingerprint: String,
}

#[derive(Debug)]
struct VerifiedCatalogue {
    catalogue: MarketplaceCatalogue,
    expired: bool,
}

pub struct MarketplaceService {
    cache_dir: PathBuf,
    state_path: PathBuf,
    client: reqwest::Client,
    state: Mutex<MarketplaceLocalState>,
}

impl MarketplaceService {
    pub fn new(app_data: &Path) -> Result<Self, String> {
        let root = app_data.join("marketplace");
        let cache_dir = root.join("cache");
        fs::create_dir_all(&cache_dir)
            .map_err(|error| format!("Unable to create Marketplace cache: {error}"))?;
        let state_path = root.join("state.json");
        let state = read_local_state(&state_path)?;
        let client = reqwest::Client::builder()
            .user_agent(format!("BakingRL/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("Unable to create Marketplace HTTP client: {error}"))?;
        Ok(Self {
            cache_dir,
            state_path,
            client,
            state: Mutex::new(state),
        })
    }

    pub async fn snapshot(&self, refresh: bool) -> Result<MarketplaceSnapshot, String> {
        if refresh {
            match self.fetch_network().await {
                Ok(snapshot) => return Ok(snapshot),
                Err(network_error) => {
                    return match self.load_cache(true) {
                        Ok(mut snapshot) => {
                            snapshot.warning = Some(network_error);
                            Ok(snapshot)
                        }
                        Err(cache_error) => Err(format!(
                            "{network_error} No verified Marketplace cache could be used: {cache_error}"
                        )),
                    };
                }
            }
        }
        self.load_cache(true)
    }

    pub fn complete_first_run(&self) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.first_run_completed = true;
        write_local_state(&self.state_path, &state)
    }

    async fn fetch_network(&self) -> Result<MarketplaceSnapshot, String> {
        let catalogue_url = format!("{MARKETPLACE_BASE_URL}/{MARKETPLACE_CATALOGUE_FILE}");
        let signature_url = format!("{MARKETPLACE_BASE_URL}/{MARKETPLACE_SIGNATURE_FILE}");
        let (catalogue_bytes, signature_bytes) = tokio::try_join!(
            fetch_limited(&self.client, &catalogue_url, MAX_CATALOGUE_BYTES),
            fetch_limited(&self.client, &signature_url, MAX_SIGNATURE_BYTES),
        )?;
        let floor = self.state.lock().unwrap().max_sequence;
        let verified = verify_catalogue_bytes(
            &catalogue_bytes,
            &signature_bytes,
            TRUSTED_ROOTS,
            floor,
            false,
            OffsetDateTime::now_utc(),
        )?;

        let cache = MarketplaceCacheFile {
            schema: MARKETPLACE_CACHE_SCHEMA.to_string(),
            catalogue_base64: BASE64_STANDARD.encode(&catalogue_bytes),
            signature_base64: BASE64_STANDARD.encode(&signature_bytes),
        };
        let cache_bytes = serde_json::to_vec(&cache)
            .map_err(|error| format!("Unable to serialize Marketplace cache: {error}"))?;
        atomic_write(&self.cache_dir.join(MARKETPLACE_CACHE_FILE), &cache_bytes)?;
        self.accept_sequence(verified.catalogue.sequence)?;
        Ok(self.to_snapshot(verified, MarketplaceSnapshotSource::Network, None))
    }

    fn load_cache(&self, allow_expired: bool) -> Result<MarketplaceSnapshot, String> {
        let cache_path = self.cache_dir.join(MARKETPLACE_CACHE_FILE);
        let cache_bytes = read_atomic_file(&cache_path)
            .map_err(|error| format!("No verified Marketplace cache is available: {error}"))?;
        let cache: MarketplaceCacheFile = serde_json::from_slice(&cache_bytes)
            .map_err(|error| format!("Marketplace cache is invalid: {error}"))?;
        if cache.schema != MARKETPLACE_CACHE_SCHEMA {
            return Err(format!(
                "Unsupported Marketplace cache schema '{}'.",
                cache.schema
            ));
        }
        let catalogue_bytes =
            decode_cached_bytes(&cache.catalogue_base64, MAX_CATALOGUE_BYTES, "catalogue")?;
        let signature_bytes =
            decode_cached_bytes(&cache.signature_base64, MAX_SIGNATURE_BYTES, "signature")?;
        let floor = self.state.lock().unwrap().max_sequence;
        let verified = verify_catalogue_bytes(
            &catalogue_bytes,
            &signature_bytes,
            TRUSTED_ROOTS,
            floor,
            allow_expired,
            OffsetDateTime::now_utc(),
        )?;
        self.accept_sequence(verified.catalogue.sequence)?;
        Ok(self.to_snapshot(verified, MarketplaceSnapshotSource::Cache, None))
    }

    fn accept_sequence(&self, sequence: u64) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if sequence > state.max_sequence {
            state.max_sequence = sequence;
            write_local_state(&self.state_path, &state)?;
        }
        Ok(())
    }

    fn to_snapshot(
        &self,
        verified: VerifiedCatalogue,
        source: MarketplaceSnapshotSource,
        warning: Option<String>,
    ) -> MarketplaceSnapshot {
        let state = self.state.lock().unwrap();
        let first_run_pending =
            !state.first_run_completed && !verified.catalogue.sections.first_run.is_empty();
        MarketplaceSnapshot {
            installable: !verified.expired,
            expired: verified.expired,
            catalogue: verified.catalogue,
            source,
            first_run_pending,
            warning,
        }
    }
}

async fn fetch_limited(
    client: &reqwest::Client,
    url: &str,
    maximum: u64,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Unable to fetch Marketplace data: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Marketplace request failed with HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err("Marketplace response exceeds its size limit".to_string());
    }
    let mut contents = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Unable to read Marketplace data: {error}"))?;
        let next_len = contents
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| "Marketplace response size overflow".to_string())?;
        if next_len as u64 > maximum {
            return Err("Marketplace response exceeds its size limit".to_string());
        }
        contents.extend_from_slice(&chunk);
    }
    Ok(contents)
}

fn verify_catalogue_bytes(
    catalogue_bytes: &[u8],
    signature_bytes: &[u8],
    roots: &[TrustedMarketplaceRoot<'_>],
    minimum_sequence: u64,
    allow_expired: bool,
    now: OffsetDateTime,
) -> Result<VerifiedCatalogue, String> {
    let envelope: MarketplaceSignatureEnvelope = serde_json::from_slice(signature_bytes)
        .map_err(|error| format!("Marketplace signature is invalid JSON: {error}"))?;
    if envelope.schema != MARKETPLACE_SIGNATURE_SCHEMA
        || envelope.algorithm != "ed25519"
        || envelope.signed_file != MARKETPLACE_CATALOGUE_FILE
    {
        return Err("Marketplace signature envelope is unsupported".to_string());
    }
    let root = roots
        .iter()
        .find(|root| root.key_id == envelope.key_id)
        .ok_or_else(|| format!("Marketplace root '{}' is not trusted", envelope.key_id))?;
    let digest = hex::encode(Sha256::digest(catalogue_bytes));
    if envelope.sha256 != digest {
        return Err("Marketplace catalogue digest does not match its signature".to_string());
    }
    let public_key = decode_canonical_base64::<32>(root.public_key, "Marketplace root key")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("Marketplace root key is invalid: {error}"))?;
    let signature = decode_canonical_base64::<64>(&envelope.signature, "Marketplace signature")?;
    let signature = Signature::from_bytes(&signature);
    verifying_key
        .verify(catalogue_bytes, &signature)
        .map_err(|_| "Marketplace catalogue signature verification failed".to_string())?;

    let catalogue: MarketplaceCatalogue = serde_json::from_slice(catalogue_bytes)
        .map_err(|error| format!("Marketplace catalogue is invalid JSON: {error}"))?;
    validate_catalogue(&catalogue)?;
    if catalogue.sequence < minimum_sequence {
        return Err(format!(
            "Marketplace sequence {} is below the accepted sequence {}",
            catalogue.sequence, minimum_sequence
        ));
    }
    let generated_at = parse_timestamp(&catalogue.generated_at, "generatedAt")?;
    let expires_at = parse_timestamp(&catalogue.expires_at, "expiresAt")?;
    if expires_at <= generated_at {
        return Err("Marketplace expiresAt must be after generatedAt".to_string());
    }
    if generated_at > now + CLOCK_TOLERANCE {
        return Err("Marketplace catalogue was generated too far in the future".to_string());
    }
    let expired = expires_at < now;
    if expired && !allow_expired && expires_at < now - CLOCK_TOLERANCE {
        return Err("Marketplace catalogue is expired beyond clock tolerance".to_string());
    }
    Ok(VerifiedCatalogue { catalogue, expired })
}

fn validate_catalogue(catalogue: &MarketplaceCatalogue) -> Result<(), String> {
    if catalogue.schema != MARKETPLACE_SCHEMA {
        return Err(format!(
            "Unsupported Marketplace schema '{}'.",
            catalogue.schema
        ));
    }
    if catalogue.sequence == 0 {
        return Err("Marketplace sequence must be greater than zero".to_string());
    }

    let mut developer_ids = HashSet::new();
    let mut public_keys = HashSet::new();
    let mut keys_by_developer = HashMap::new();
    for developer in &catalogue.developers {
        require_identifier(&developer.id, "developer id")?;
        require_non_empty(&developer.name, "developer name")?;
        if !developer_ids.insert(developer.id.as_str()) {
            return Err(format!(
                "Duplicate Marketplace developer '{}'.",
                developer.id
            ));
        }
        if developer.signing_keys.is_empty() {
            return Err(format!("Developer '{}' has no signing keys", developer.id));
        }
        let mut key_ids = HashSet::new();
        for key in &developer.signing_keys {
            require_identifier(&key.id, "developer signing key id")?;
            if key.algorithm != "ed25519" {
                return Err(format!("Signing key '{}' is not Ed25519", key.id));
            }
            decode_canonical_base64::<32>(&key.public_key, "developer signing key")?;
            if !key_ids.insert(key.id.as_str()) {
                return Err(format!("Duplicate signing key '{}'.", key.id));
            }
            if !public_keys.insert(key.public_key.as_str()) {
                return Err("A Marketplace public key is registered more than once".to_string());
            }
            validate_revocable(key.status, key.revocation.as_ref(), "signing key")?;
        }
        keys_by_developer.insert(developer.id.as_str(), &developer.signing_keys);
    }

    let mut package_ids = HashSet::new();
    for package in &catalogue.packages {
        if package.schema != "bakingrl.marketplace-package/2" {
            return Err(format!(
                "Package '{}' has an unsupported schema",
                package.id
            ));
        }
        require_package_id(&package.id)?;
        if !package_ids.insert(package.id.as_str()) {
            return Err(format!("Duplicate Marketplace package '{}'.", package.id));
        }
        let signing_keys = keys_by_developer
            .get(package.developer_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Package '{}' references unknown developer '{}'",
                    package.id, package.developer_id
                )
            })?;
        validate_revocable(package.status, package.revocation.as_ref(), "package")?;
        require_https_url(&package.repo, "package repository")?;
        validate_listing(package)?;
        if package.versions.is_empty() {
            return Err(format!("Package '{}' has no versions", package.id));
        }
        let mut versions = HashSet::new();
        for version in &package.versions {
            Version::parse(&version.version).map_err(|error| {
                format!(
                    "Package '{}@{}' has invalid version: {error}",
                    package.id, version.version
                )
            })?;
            if !versions.insert(version.version.as_str()) {
                return Err(format!(
                    "Package '{}' contains duplicate version '{}'",
                    package.id, version.version
                ));
            }
            validate_revocable(
                version.status,
                version.revocation.as_ref(),
                "package version",
            )?;
            let runtime_api = Version::parse(&version.runtime_api).map_err(|error| {
                format!(
                    "Package '{}@{}' has invalid Runtime API: {error}",
                    package.id, version.version
                )
            })?;
            if version.status == MarketplaceStatus::Active
                && (runtime_api.major != 2 || runtime_api.minor != 3 || !runtime_api.pre.is_empty())
            {
                return Err(format!(
                    "Active package '{}@{}' must target Runtime API 2.3.x",
                    package.id, version.version
                ));
            }
            if let Some(minimum) = &version.min_bakingrl_version {
                Version::parse(minimum).map_err(|error| {
                    format!(
                        "Package '{}@{}' has invalid minimum host version: {error}",
                        package.id, version.version
                    )
                })?;
            }
            parse_timestamp(&version.reviewed_at, "reviewedAt")?;
            validate_dependencies(package, version)?;
            validate_permissions(&version.permissions)?;
            validate_artifacts(package, version, signing_keys)?;
        }
        if package.status == MarketplaceStatus::Yanked
            && package
                .versions
                .iter()
                .any(|version| version.status == MarketplaceStatus::Active)
        {
            return Err(format!(
                "Yanked package '{}' contains an active version",
                package.id
            ));
        }
        if package.status == MarketplaceStatus::Revoked
            && package
                .versions
                .iter()
                .any(|version| version.status != MarketplaceStatus::Revoked)
        {
            return Err(format!(
                "Revoked package '{}' contains a non-revoked version",
                package.id
            ));
        }
    }

    for package in &catalogue.packages {
        for version in &package.versions {
            for dependency in &version.dependencies {
                if !package_ids.contains(dependency.package_id.as_str()) {
                    return Err(format!(
                        "Package '{}@{}' references unknown dependency '{}'",
                        package.id, version.version, dependency.package_id
                    ));
                }
            }
        }
    }
    validate_sections(&catalogue.sections, &catalogue.packages)
}

fn validate_listing(package: &MarketplacePackage) -> Result<(), String> {
    require_https_url(&package.listing.source_url, "listing source")?;
    require_sha256(&package.listing.snapshot_sha256, "listing snapshot hash")?;
    let snapshot = &package.listing.snapshot;
    if snapshot.schema != "bakingrl.marketplace-listing/2" || snapshot.package_id != package.id {
        return Err(format!(
            "Package '{}' has an invalid listing snapshot",
            package.id
        ));
    }
    for value in [
        &snapshot.display_name,
        &snapshot.short_description,
        &snapshot.long_description,
    ] {
        require_non_empty(value, "listing text")?;
    }
    require_https_url(&snapshot.repo, "listing repository")?;
    require_https_url(&snapshot.links.docs, "listing documentation")?;
    require_https_url(&snapshot.links.support, "listing support")?;
    if let Some(icon) = &snapshot.media.icon {
        validate_media(icon)?;
    }
    if let Some(banner) = &snapshot.media.banner {
        validate_media(banner)?;
    }
    for screenshot in &snapshot.media.screenshots {
        require_https_url(&screenshot.url, "listing screenshot")?;
        require_sha256(&screenshot.sha256, "listing screenshot hash")?;
    }
    let canonical = canonical_json(
        &serde_json::to_value(snapshot)
            .map_err(|error| format!("Unable to serialize Marketplace listing: {error}"))?,
    );
    let actual = hex::encode(Sha256::digest(canonical.as_bytes()));
    if actual != package.listing.snapshot_sha256 {
        return Err(format!(
            "Package '{}' listing snapshot hash does not match",
            package.id
        ));
    }
    Ok(())
}

fn validate_media(media: &MarketplaceMediaAsset) -> Result<(), String> {
    require_https_url(&media.url, "listing media")?;
    require_sha256(&media.sha256, "listing media hash")
}

fn validate_dependencies(
    package: &MarketplacePackage,
    version: &MarketplacePackageVersion,
) -> Result<(), String> {
    let mut ids = HashSet::new();
    for dependency in &version.dependencies {
        require_package_id(&dependency.package_id)?;
        if dependency.package_id == package.id {
            return Err(format!("Package '{}' depends on itself", package.id));
        }
        if !ids.insert(dependency.package_id.as_str()) {
            return Err(format!(
                "Package '{}' contains a duplicate dependency",
                package.id
            ));
        }
        semver::VersionReq::parse(&dependency.version).map_err(|error| {
            format!(
                "Package '{}@{}' dependency '{}' has invalid requirement: {error}",
                package.id, version.version, dependency.package_id
            )
        })?;
    }
    Ok(())
}

fn validate_permissions(permissions: &MarketplacePermissions) -> Result<(), String> {
    for pattern in permissions
        .bus
        .read
        .iter()
        .chain(permissions.bus.publish.iter())
        .chain(permissions.registry.read.iter())
        .chain(permissions.registry.write.iter())
    {
        validate_pattern(pattern, false)?;
    }
    for pattern in permissions
        .storage
        .read
        .iter()
        .chain(permissions.storage.write.iter())
    {
        validate_pattern(pattern, true)?;
    }
    for endpoint in permissions
        .network
        .http
        .iter()
        .chain(permissions.network.websocket.iter())
    {
        require_non_empty(&endpoint.host, "network permission host")?;
        validate_ports(&endpoint.ports)?;
        if !matches!(endpoint.scheme.as_str(), "http" | "https" | "ws" | "wss") {
            return Err("Marketplace network permission has an invalid scheme".to_string());
        }
        for prefix in &endpoint.path_prefixes {
            if !prefix.starts_with('/') || prefix.contains(['\\', '?', '#']) {
                return Err("Marketplace network path prefix is invalid".to_string());
            }
        }
    }
    for endpoint in &permissions.network.listen {
        require_non_empty(&endpoint.host, "listen permission host")?;
        validate_ports(&endpoint.ports)?;
        if !matches!(
            endpoint.transport.as_str(),
            "http" | "https" | "ws" | "wss" | "tcp"
        ) {
            return Err("Marketplace listen permission has an invalid transport".to_string());
        }
    }
    Ok(())
}

fn validate_pattern(value: &str, storage: bool) -> Result<(), String> {
    require_non_empty(value, "permission pattern")?;
    let stars = value.bytes().filter(|byte| *byte == b'*').count();
    if stars > 1 || (stars == 1 && !value.ends_with('*')) {
        return Err(format!("Invalid Marketplace permission pattern '{value}'"));
    }
    if storage
        && value != "*"
        && (value.starts_with('/') || value.contains("\\") || value.contains("://"))
    {
        return Err(format!("Invalid Marketplace storage permission '{value}'"));
    }
    if storage && value.split('/').any(|part| part == "..") {
        return Err(format!("Invalid Marketplace storage permission '{value}'"));
    }
    Ok(())
}

fn validate_ports(ports: &MarketplacePorts) -> Result<(), String> {
    match ports {
        MarketplacePorts::Any(value) if value == "*" => Ok(()),
        MarketplacePorts::Any(_) => Err("Marketplace ports must be '*' or an array".to_string()),
        MarketplacePorts::Ports(values) if values.is_empty() || values.contains(&0) => {
            Err("Marketplace port array is invalid".to_string())
        }
        MarketplacePorts::Ports(_) => Ok(()),
    }
}

fn validate_artifacts(
    package: &MarketplacePackage,
    version: &MarketplacePackageVersion,
    signing_keys: &[MarketplaceSigningKey],
) -> Result<(), String> {
    if version.artifacts.is_empty() {
        return Err(format!(
            "Package '{}@{}' has no artifacts",
            package.id, version.version
        ));
    }
    let mut platforms = HashSet::new();
    for artifact in &version.artifacts {
        if !matches!(
            artifact.platform.as_str(),
            "any" | "darwin-arm64" | "darwin-x64" | "linux-arm64" | "linux-x64" | "windows-x64"
        ) {
            return Err(format!(
                "Package '{}@{}' has an unsupported artifact platform",
                package.id, version.version
            ));
        }
        if !platforms.insert(artifact.platform.as_str()) {
            return Err(format!(
                "Package '{}@{}' has duplicate artifact platforms",
                package.id, version.version
            ));
        }
        require_https_url(&artifact.bundle_url, "artifact URL")?;
        require_sha256(&artifact.bundle_sha256, "artifact hash")?;
        let key = signing_keys
            .iter()
            .find(|key| key.id == artifact.signing_key_id)
            .ok_or_else(|| {
                format!(
                    "Artifact for '{}@{}' references an unknown signing key",
                    package.id, version.version
                )
            })?;
        if version.status == MarketplaceStatus::Active && key.status != MarketplaceStatus::Active {
            return Err(format!(
                "Active package '{}@{}' uses an inactive signing key",
                package.id, version.version
            ));
        }
    }
    Ok(())
}

fn validate_sections(
    sections: &MarketplaceSections,
    packages: &[MarketplacePackage],
) -> Result<(), String> {
    for (name, values) in [
        ("recommended", &sections.recommended),
        ("new", &sections.new),
        ("firstRun", &sections.first_run),
    ] {
        let mut unique = HashSet::new();
        for package_id in values {
            if !unique.insert(package_id.as_str()) {
                return Err(format!("Marketplace section '{name}' contains duplicates"));
            }
            let package = packages
                .iter()
                .find(|package| package.id == *package_id)
                .ok_or_else(|| {
                    format!(
                        "Marketplace section '{name}' references unknown package '{package_id}'"
                    )
                })?;
            if package.status == MarketplaceStatus::Revoked {
                return Err(format!(
                    "Marketplace section '{name}' references revoked package '{package_id}'"
                ));
            }
            if name == "firstRun"
                && (package.status != MarketplaceStatus::Active
                    || !package.versions.iter().any(|version| {
                        version.status == MarketplaceStatus::Active
                            && version.channel == MarketplaceChannel::Stable
                    }))
            {
                return Err(format!(
                    "Marketplace firstRun package '{package_id}' is not installable"
                ));
            }
        }
    }
    Ok(())
}

fn validate_revocable(
    status: MarketplaceStatus,
    revocation: Option<&MarketplaceRevocation>,
    label: &str,
) -> Result<(), String> {
    match (status, revocation) {
        (MarketplaceStatus::Revoked, Some(revocation)) => {
            require_non_empty(&revocation.reason, "revocation reason")?;
            parse_timestamp(&revocation.revoked_at, "revokedAt")?;
            Ok(())
        }
        (MarketplaceStatus::Revoked, None) => {
            Err(format!("Revoked {label} has no revocation record"))
        }
        (_, Some(_)) => Err(format!("Non-revoked {label} contains a revocation record")),
        (_, None) => Ok(()),
    }
}

fn parse_timestamp(value: &str, label: &str) -> Result<OffsetDateTime, String> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| format!("Marketplace {label} is invalid: {error}"))
}

fn require_https_url(value: &str, label: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| format!("Marketplace {label} is invalid: {error}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!(
            "Marketplace {label} must be an HTTPS URL without credentials"
        ));
    }
    Ok(())
}

fn require_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "Marketplace {label} must be a lowercase SHA-256 digest"
        ));
    }
    Ok(())
}

fn require_identifier(value: &str, label: &str) -> Result<(), String> {
    require_non_empty(value, label)?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(format!("Marketplace {label} is invalid"));
    }
    Ok(())
}

fn require_package_id(value: &str) -> Result<(), String> {
    require_identifier(value, "package id")?;
    if !value.contains(['.', '-']) {
        return Err(format!("Marketplace package id '{value}' is invalid"));
    }
    Ok(())
}

fn require_non_empty(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Marketplace {label} cannot be empty"));
    }
    Ok(())
}

fn decode_canonical_base64<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let decoded = BASE64_STANDARD
        .decode(value)
        .map_err(|error| format!("{label} is invalid base64: {error}"))?;
    if BASE64_STANDARD.encode(&decoded) != value {
        return Err(format!("{label} is not canonical base64"));
    }
    decoded
        .try_into()
        .map_err(|_| format!("{label} must contain {N} bytes"))
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => serde_json::to_string(value).unwrap(),
        serde_json::Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        serde_json::Value::Object(values) => {
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical_json(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}

fn read_local_state(path: &Path) -> Result<MarketplaceLocalState, String> {
    if !path.exists() && !atomic_backup_path(path)?.exists() {
        return Ok(MarketplaceLocalState::default());
    }
    let raw = read_atomic_file(path)
        .map_err(|error| format!("Unable to read Marketplace state: {error}"))?;
    let state: MarketplaceLocalState = serde_json::from_slice(&raw)
        .map_err(|error| format!("Marketplace state is invalid: {error}"))?;
    if state.schema != MARKETPLACE_STATE_SCHEMA {
        return Err(format!(
            "Unsupported Marketplace state schema '{}'.",
            state.schema
        ));
    }
    Ok(state)
}

fn write_local_state(path: &Path, state: &MarketplaceLocalState) -> Result<(), String> {
    let raw = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("Unable to serialize Marketplace state: {error}"))?;
    atomic_write(path, &raw)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Marketplace file has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create Marketplace directory: {error}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Marketplace file name is invalid".to_string())?;
    let temporary = parent.join(format!(".{file_name}.{stamp}.tmp"));
    recover_atomic_file(path)?;
    let backup = atomic_backup_path(path)?;
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("Unable to clean Marketplace backup: {error}"))?;
    }
    let mut file = File::create(&temporary)
        .map_err(|error| format!("Unable to create Marketplace temporary file: {error}"))?;
    file.write_all(contents)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Unable to write Marketplace temporary file: {error}"))?;
    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup)
            .map_err(|error| format!("Unable to back up Marketplace file: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if had_previous {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("Unable to activate Marketplace file: {error}"));
    }
    if had_previous {
        fs::remove_file(&backup)
            .map_err(|error| format!("Unable to clean Marketplace backup: {error}"))?;
    }
    Ok(())
}

fn read_atomic_file(path: &Path) -> Result<Vec<u8>, String> {
    recover_atomic_file(path)?;
    fs::read(path).map_err(|error| format!("Unable to read Marketplace file: {error}"))
}

fn recover_atomic_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let backup = atomic_backup_path(path)?;
    if backup.exists() {
        fs::rename(&backup, path)
            .map_err(|error| format!("Unable to recover Marketplace file: {error}"))?;
    }
    Ok(())
}

fn atomic_backup_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Marketplace file has no parent directory".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Marketplace file name is invalid".to_string())?;
    Ok(parent.join(format!(".{file_name}.backup")))
}

fn decode_cached_bytes(value: &str, maximum: u64, label: &str) -> Result<Vec<u8>, String> {
    let decoded = BASE64_STANDARD
        .decode(value)
        .map_err(|error| format!("Marketplace cached {label} is invalid base64: {error}"))?;
    if decoded.len() as u64 > maximum || BASE64_STANDARD.encode(&decoded) != value {
        return Err(format!("Marketplace cached {label} is invalid"));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const GENERATED_AT: &str = "2026-07-16T12:00:00Z";
    const EXPIRES_AT: &str = "2026-07-23T12:00:00Z";

    fn sample_catalogue() -> MarketplaceCatalogue {
        let mut snapshot = MarketplaceListingSnapshot {
            schema: "bakingrl.marketplace-listing/2".to_string(),
            package_id: "com.example.visual".to_string(),
            display_name: "Example Visual".to_string(),
            short_description: "A compact example visual.".to_string(),
            long_description: "A signed fixture used to verify the Marketplace client.".to_string(),
            tags: vec!["visual".to_string()],
            repo: "https://github.com/example/visual".to_string(),
            media: MarketplaceMedia {
                icon: None,
                banner: None,
                screenshots: Vec::new(),
            },
            links: MarketplaceLinks {
                docs: "https://github.com/example/visual#readme".to_string(),
                support: "https://github.com/example/visual/issues".to_string(),
            },
        };
        let listing_value = serde_json::to_value(&snapshot).unwrap();
        let listing_hash = hex::encode(Sha256::digest(canonical_json(&listing_value)));

        MarketplaceCatalogue {
            schema: MARKETPLACE_SCHEMA.to_string(),
            sequence: 42,
            generated_at: GENERATED_AT.to_string(),
            expires_at: EXPIRES_AT.to_string(),
            sections: MarketplaceSections {
                recommended: vec![snapshot.package_id.clone()],
                new: vec![snapshot.package_id.clone()],
                first_run: vec![snapshot.package_id.clone()],
            },
            developers: vec![MarketplaceDeveloper {
                id: "example".to_string(),
                name: "Example Publisher".to_string(),
                kind: MarketplaceDeveloperKind::Organization,
                verification: MarketplaceDeveloperVerification::Verified,
                signing_keys: vec![MarketplaceSigningKey {
                    id: "example-release-1".to_string(),
                    algorithm: "ed25519".to_string(),
                    public_key: BASE64_STANDARD.encode([3u8; 32]),
                    status: MarketplaceStatus::Active,
                    revocation: None,
                }],
            }],
            packages: vec![MarketplacePackage {
                schema: "bakingrl.marketplace-package/2".to_string(),
                id: snapshot.package_id.clone(),
                developer_id: "example".to_string(),
                status: MarketplaceStatus::Active,
                revocation: None,
                repo: snapshot.repo.clone(),
                listing: MarketplaceListing {
                    source_url:
                        "https://raw.githubusercontent.com/example/visual/main/listing.json"
                            .to_string(),
                    snapshot_sha256: listing_hash,
                    snapshot: {
                        snapshot.tags.sort();
                        snapshot
                    },
                },
                versions: vec![MarketplacePackageVersion {
                    version: "1.0.0".to_string(),
                    status: MarketplaceStatus::Active,
                    revocation: None,
                    channel: MarketplaceChannel::Stable,
                    runtime_api: "2.3.0".to_string(),
                    min_bakingrl_version: None,
                    runtime: MarketplaceRuntime {
                        node: false,
                        sidecars: Vec::new(),
                        webviews: Vec::new(),
                    },
                    dependencies: Vec::new(),
                    permissions: empty_permissions(),
                    native_capabilities: MarketplaceNativeCapabilities {
                        node: None,
                        sidecars: Vec::new(),
                        surfaces: Vec::new(),
                    },
                    artifacts: vec![MarketplaceArtifact {
                        platform: "any".to_string(),
                        bundle_url: "https://github.com/example/visual/releases/download/v1.0.0/com.example.visual-1.0.0.brlp".to_string(),
                        bundle_sha256: "a".repeat(64),
                        signing_key_id: "example-release-1".to_string(),
                    }],
                    reviewed_at: GENERATED_AT.to_string(),
                }],
            }],
        }
    }

    fn empty_permissions() -> MarketplacePermissions {
        MarketplacePermissions {
            bus: MarketplaceBusPermissions {
                read: Vec::new(),
                publish: Vec::new(),
            },
            registry: MarketplaceReadWritePermissions {
                read: Vec::new(),
                write: Vec::new(),
            },
            network: MarketplaceNetworkPermissions {
                http: Vec::new(),
                websocket: Vec::new(),
                listen: Vec::new(),
            },
            storage: MarketplaceReadWritePermissions {
                read: Vec::new(),
                write: Vec::new(),
            },
        }
    }

    fn signed_catalogue(catalogue: &MarketplaceCatalogue) -> (Vec<u8>, Vec<u8>, String) {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let raw = serde_json::to_vec_pretty(catalogue).unwrap();
        let public_key = BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());
        let envelope = MarketplaceSignatureEnvelope {
            schema: MARKETPLACE_SIGNATURE_SCHEMA.to_string(),
            algorithm: "ed25519".to_string(),
            key_id: "test-root-1".to_string(),
            signed_file: MARKETPLACE_CATALOGUE_FILE.to_string(),
            sha256: hex::encode(Sha256::digest(&raw)),
            signature: BASE64_STANDARD.encode(signing_key.sign(&raw).to_bytes()),
        };
        (raw, serde_json::to_vec(&envelope).unwrap(), public_key)
    }

    fn test_now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-07-17T12:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn verifies_exact_catalogue_bytes() {
        let (raw, signature, public_key) = signed_catalogue(&sample_catalogue());
        let roots = [TrustedMarketplaceRoot {
            key_id: "test-root-1",
            public_key: &public_key,
        }];

        let verified =
            verify_catalogue_bytes(&raw, &signature, &roots, 0, false, test_now()).unwrap();
        assert_eq!(verified.catalogue.sequence, 42);
        assert!(!verified.expired);

        let mut changed = raw;
        changed.push(b'\n');
        let error =
            verify_catalogue_bytes(&changed, &signature, &roots, 0, false, test_now()).unwrap_err();
        assert!(error.contains("digest"));
    }

    #[test]
    fn rejects_unknown_catalogue_fields_after_valid_signature() {
        let mut value = serde_json::to_value(sample_catalogue()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), serde_json::json!(true));
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let raw = serde_json::to_vec(&value).unwrap();
        let public_key = BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());
        let envelope = MarketplaceSignatureEnvelope {
            schema: MARKETPLACE_SIGNATURE_SCHEMA.to_string(),
            algorithm: "ed25519".to_string(),
            key_id: "test-root-1".to_string(),
            signed_file: MARKETPLACE_CATALOGUE_FILE.to_string(),
            sha256: hex::encode(Sha256::digest(&raw)),
            signature: BASE64_STANDARD.encode(signing_key.sign(&raw).to_bytes()),
        };
        let signature = serde_json::to_vec(&envelope).unwrap();
        let roots = [TrustedMarketplaceRoot {
            key_id: "test-root-1",
            public_key: &public_key,
        }];

        let error =
            verify_catalogue_bytes(&raw, &signature, &roots, 0, false, test_now()).unwrap_err();
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn enforces_sequence_floor_and_expired_cache_policy() {
        let (raw, signature, public_key) = signed_catalogue(&sample_catalogue());
        let roots = [TrustedMarketplaceRoot {
            key_id: "test-root-1",
            public_key: &public_key,
        }];

        let rollback =
            verify_catalogue_bytes(&raw, &signature, &roots, 43, false, test_now()).unwrap_err();
        assert!(rollback.contains("below the accepted sequence"));

        let expired_now = OffsetDateTime::parse("2026-07-25T12:00:00Z", &Rfc3339).unwrap();
        let expired =
            verify_catalogue_bytes(&raw, &signature, &roots, 0, true, expired_now).unwrap();
        assert!(expired.expired);
        let install_error =
            verify_catalogue_bytes(&raw, &signature, &roots, 0, false, expired_now).unwrap_err();
        assert!(install_error.contains("expired"));
    }

    #[test]
    fn recovers_interrupted_marketplace_state_write() {
        let directory = tempfile::tempdir().unwrap();
        let state_path = directory.path().join("state.json");
        let backup_path = atomic_backup_path(&state_path).unwrap();
        let expected = MarketplaceLocalState {
            schema: MARKETPLACE_STATE_SCHEMA.to_string(),
            max_sequence: 99,
            first_run_completed: true,
            trusted_publishers: Vec::new(),
        };
        fs::write(&backup_path, serde_json::to_vec(&expected).unwrap()).unwrap();

        let recovered = read_local_state(&state_path).unwrap();
        assert_eq!(recovered.max_sequence, 99);
        assert!(recovered.first_run_completed);
        assert!(state_path.exists());
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    #[ignore = "requires the published Marketplace endpoint"]
    async fn verifies_published_marketplace_endpoint() {
        let directory = tempfile::tempdir().unwrap();
        let service = MarketplaceService::new(directory.path()).unwrap();
        let snapshot = service.snapshot(true).await.unwrap();

        assert_eq!(snapshot.catalogue.schema, MARKETPLACE_SCHEMA);
        assert!(snapshot.catalogue.sequence > 0);
        assert!(matches!(
            snapshot.source,
            MarketplaceSnapshotSource::Network
        ));

        let cached = service.snapshot(false).await.unwrap();
        assert_eq!(cached.catalogue.sequence, snapshot.catalogue.sequence);
        assert!(matches!(cached.source, MarketplaceSnapshotSource::Cache));
    }
}
