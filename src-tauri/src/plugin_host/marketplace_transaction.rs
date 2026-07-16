use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::models::PackageStateFile;
use crate::plugin_package::bundle::{extract_bundle, inspect_bundle, BundleInspection};
use crate::plugin_package::install::{download_bundle_to_file, InstallReceipt};
use crate::plugin_package::manifest::{
    PluginBusPermissionsV4, PluginListenEndpointV4, PluginNetworkEndpointV4,
    PluginNetworkPermissionsV4, PluginNetworkPortsV4, PluginPermissionsV4,
    PluginRegistryPermissionsV4, PluginStoragePermissionsV4,
};

use super::marketplace::{
    atomic_write, MarketplaceArtifact, MarketplaceCatalogue, MarketplaceChannel,
    MarketplaceDeveloper, MarketplaceDeveloperKind, MarketplaceDeveloperVerification,
    MarketplaceNativeCapabilities, MarketplaceNetworkEndpoint, MarketplacePackage,
    MarketplacePackageVersion, MarketplacePermissions, MarketplacePorts, MarketplaceSigningKey,
    MarketplaceStatus,
};
use super::package_files::safe_installed_package_dir;

const INSTALL_JOURNAL_SCHEMA: &str = "bakingrl.marketplace-install-journal/1";
const PROVENANCE_SCHEMA: &str = "bakingrl.marketplace-provenance/1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInstallPlan {
    pub transaction_id: String,
    pub catalogue_sequence: u64,
    pub packages: Vec<MarketplaceInstallPlanPackage>,
    pub publishers: Vec<MarketplacePublisherConsent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInstallPlanPackage {
    pub package_id: String,
    pub display_name: String,
    pub version: String,
    pub operation: MarketplaceInstallOperation,
    pub requested: bool,
    pub developer_id: String,
    pub dependencies: Vec<String>,
    pub permissions: MarketplacePermissions,
    pub native_capabilities: MarketplaceNativeCapabilities,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceInstallOperation {
    Install,
    Update,
    Downgrade,
    Reinstall,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePublisherConsent {
    pub trust_id: String,
    pub developer_id: String,
    pub name: String,
    pub kind: MarketplaceDeveloperKind,
    pub verification: MarketplaceDeveloperVerification,
    pub signing_key_id: String,
    pub key_fingerprint: String,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInstallResult {
    pub transaction_id: String,
    pub receipts: Vec<InstallReceipt>,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedMarketplaceTransaction {
    pub id: String,
    pub catalogue_sequence: u64,
    pub packages: Vec<PreparedMarketplacePackage>,
    pub publishers: Vec<PreparedPublisher>,
    pub required_trust_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedMarketplacePackage {
    package: MarketplacePackage,
    version: MarketplacePackageVersion,
    artifact: MarketplaceArtifact,
    developer: MarketplaceDeveloper,
    signing_key: MarketplaceSigningKey,
    bundle_path: PathBuf,
    staged_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedPublisher {
    pub developer_id: String,
    pub key_fingerprint: String,
    trust_id: String,
}

#[derive(Debug, Clone)]
struct ResolvedMarketplacePackage {
    package: MarketplacePackage,
    version: MarketplacePackageVersion,
    artifact: MarketplaceArtifact,
    developer: MarketplaceDeveloper,
    signing_key: MarketplaceSigningKey,
    requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceInstallJournal {
    schema: String,
    transaction_id: String,
    package_ids: Vec<String>,
    previously_installed: Vec<String>,
    previous_state: PackageStateFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketplaceProvenance {
    schema: String,
    catalogue_sequence: u64,
    package_id: String,
    version: String,
    developer_id: String,
    signing_key_id: String,
    bundle_sha256: String,
    bundle_url: String,
}

pub(super) struct MarketplaceInstaller {
    root: PathBuf,
    packages_dir: PathBuf,
    state_path: PathBuf,
    prepared: Mutex<HashMap<String, PreparedMarketplaceTransaction>>,
}

impl MarketplaceInstaller {
    pub fn new(app_data: &Path, packages_dir: &Path, state_path: &Path) -> Result<Self, String> {
        let root = app_data.join("marketplace");
        fs::create_dir_all(root.join("transactions")).map_err(|error| {
            format!("Unable to create Marketplace transaction directory: {error}")
        })?;
        let installer = Self {
            root,
            packages_dir: packages_dir.to_path_buf(),
            state_path: state_path.to_path_buf(),
            prepared: Mutex::new(HashMap::new()),
        };
        installer.recover_incomplete_commit()?;
        installer.cleanup_stale_transactions()?;
        Ok(installer)
    }

    pub async fn prepare(
        &self,
        catalogue: MarketplaceCatalogue,
        requested_package_ids: Vec<String>,
        installed_versions: &HashMap<String, String>,
        trusted_publishers: &HashSet<String>,
        host_version: &str,
    ) -> Result<MarketplaceInstallPlan, String> {
        let requested = normalize_requested_packages(requested_package_ids)?;
        let host_version = Version::parse(host_version)
            .map_err(|error| format!("BakingRL version is invalid: {error}"))?;
        let resolved = resolve_install_graph(&catalogue, &requested, &host_version)?;
        let transaction_id = unique_transaction_id();
        let transaction_root = self.transaction_root(&transaction_id)?;
        fs::create_dir_all(transaction_root.join("downloads"))
            .and_then(|_| fs::create_dir_all(transaction_root.join("staged")))
            .map_err(|error| format!("Unable to create Marketplace transaction: {error}"))?;

        let prepared = self
            .prepare_packages(
                &transaction_id,
                &transaction_root,
                resolved,
                &requested,
                installed_versions,
                trusted_publishers,
                catalogue.sequence,
            )
            .await;
        let (transaction, plan) = match prepared {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_dir_all(&transaction_root);
                return Err(error);
            }
        };
        self.prepared
            .lock()
            .unwrap()
            .insert(transaction_id, transaction);
        Ok(plan)
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_packages(
        &self,
        transaction_id: &str,
        transaction_root: &Path,
        resolved: Vec<ResolvedMarketplacePackage>,
        requested: &HashSet<String>,
        installed_versions: &HashMap<String, String>,
        trusted_publishers: &HashSet<String>,
        catalogue_sequence: u64,
    ) -> Result<(PreparedMarketplaceTransaction, MarketplaceInstallPlan), String> {
        let mut packages = Vec::new();
        let mut plan_packages = Vec::new();
        let mut publishers_by_trust_id = HashMap::<String, MarketplacePublisherConsent>::new();
        let mut prepared_publishers = HashMap::<String, PreparedPublisher>::new();

        for resolved in resolved {
            let package_id = resolved.package.id.clone();
            let bundle_path = transaction_root
                .join("downloads")
                .join(format!("{package_id}-{}.brlp", resolved.version.version));
            download_bundle_to_file(&resolved.artifact.bundle_url, &bundle_path).await?;
            let inspection = inspect_bundle(&bundle_path)?;
            validate_bundle_concordance(&resolved, &inspection)?;

            let staged_path = transaction_root.join("staged").join(&package_id);
            extract_bundle(&bundle_path, &staged_path)?;
            let receipt = install_receipt(&resolved, catalogue_sequence);
            write_install_metadata(&staged_path, &resolved, catalogue_sequence, &receipt)?;

            let key_fingerprint = public_key_fingerprint(&resolved.signing_key.public_key)?;
            let trust_id = format!("{}:{key_fingerprint}", resolved.developer.id);
            let trusted = trusted_publishers.contains(&trust_id);
            publishers_by_trust_id
                .entry(trust_id.clone())
                .or_insert_with(|| MarketplacePublisherConsent {
                    trust_id: trust_id.clone(),
                    developer_id: resolved.developer.id.clone(),
                    name: resolved.developer.name.clone(),
                    kind: resolved.developer.kind,
                    verification: resolved.developer.verification,
                    signing_key_id: resolved.signing_key.id.clone(),
                    key_fingerprint: key_fingerprint.clone(),
                    trusted,
                });
            prepared_publishers
                .entry(trust_id.clone())
                .or_insert(PreparedPublisher {
                    developer_id: resolved.developer.id.clone(),
                    key_fingerprint,
                    trust_id,
                });

            plan_packages.push(MarketplaceInstallPlanPackage {
                package_id: package_id.clone(),
                display_name: resolved.package.listing.snapshot.display_name.clone(),
                version: resolved.version.version.clone(),
                operation: install_operation(
                    installed_versions.get(&package_id).map(String::as_str),
                    &resolved.version.version,
                )?,
                requested: requested.contains(&package_id),
                developer_id: resolved.developer.id.clone(),
                dependencies: resolved
                    .version
                    .dependencies
                    .iter()
                    .filter(|dependency| !dependency.optional)
                    .map(|dependency| dependency.package_id.clone())
                    .collect(),
                permissions: resolved.version.permissions.clone(),
                native_capabilities: resolved.version.native_capabilities.clone(),
            });
            packages.push(PreparedMarketplacePackage {
                package: resolved.package,
                version: resolved.version,
                artifact: resolved.artifact,
                developer: resolved.developer,
                signing_key: resolved.signing_key,
                bundle_path,
                staged_path,
            });
        }

        let mut publishers = publishers_by_trust_id.into_values().collect::<Vec<_>>();
        publishers.sort_by(|left, right| left.developer_id.cmp(&right.developer_id));
        let mut prepared_publishers = prepared_publishers.into_values().collect::<Vec<_>>();
        prepared_publishers.sort_by(|left, right| left.trust_id.cmp(&right.trust_id));
        let required_trust_ids = publishers
            .iter()
            .filter(|publisher| !publisher.trusted)
            .map(|publisher| publisher.trust_id.clone())
            .collect::<Vec<_>>();
        let transaction = PreparedMarketplaceTransaction {
            id: transaction_id.to_string(),
            catalogue_sequence,
            packages,
            publishers: prepared_publishers,
            required_trust_ids,
        };
        let plan = MarketplaceInstallPlan {
            transaction_id: transaction_id.to_string(),
            catalogue_sequence,
            packages: plan_packages,
            publishers,
        };
        Ok((transaction, plan))
    }

    pub fn transaction_for_commit(
        &self,
        transaction_id: &str,
        accepted_publishers: &[String],
    ) -> Result<PreparedMarketplaceTransaction, String> {
        let transaction = self
            .prepared
            .lock()
            .unwrap()
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| "Marketplace install transaction is missing or expired".to_string())?;
        let accepted = accepted_publishers.iter().collect::<HashSet<_>>();
        let missing = transaction
            .required_trust_ids
            .iter()
            .filter(|trust_id| !accepted.contains(trust_id))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "Publisher trust was not accepted for: {}",
                missing.join(", ")
            ));
        }
        Ok(transaction)
    }

    pub fn begin_commit(
        &self,
        transaction: &PreparedMarketplaceTransaction,
        previous_state: &PackageStateFile,
    ) -> Result<(), String> {
        if self.journal_path().exists() || self.journal_backup_path().exists() {
            return Err(
                "Another Marketplace installation must be recovered before continuing".to_string(),
            );
        }
        let transaction_root = self.transaction_root(&transaction.id)?;
        if !transaction_root.exists() {
            return Err("Marketplace install transaction files are missing".to_string());
        }
        let backup_root = transaction_root.join("backups");
        fs::create_dir_all(&backup_root)
            .map_err(|error| format!("Unable to create Marketplace backup directory: {error}"))?;

        let mut previously_installed = Vec::new();
        for package in &transaction.packages {
            let installed = safe_installed_package_dir(&self.packages_dir, &package.package.id)?;
            if installed.exists() {
                previously_installed.push(package.package.id.clone());
            }
        }
        let journal = MarketplaceInstallJournal {
            schema: INSTALL_JOURNAL_SCHEMA.to_string(),
            transaction_id: transaction.id.clone(),
            package_ids: transaction
                .packages
                .iter()
                .map(|package| package.package.id.clone())
                .collect(),
            previously_installed,
            previous_state: previous_state.clone(),
        };
        self.write_journal(&journal)
    }

    pub fn swap_packages(
        &self,
        transaction: &PreparedMarketplaceTransaction,
    ) -> Result<Vec<InstallReceipt>, String> {
        let journal = self.read_journal()?;
        let package_ids = transaction
            .packages
            .iter()
            .map(|package| package.package.id.clone())
            .collect::<Vec<_>>();
        if journal.transaction_id != transaction.id || journal.package_ids != package_ids {
            return Err("Marketplace install journal does not match the transaction".to_string());
        }
        let backup_root = self.transaction_root(&transaction.id)?.join("backups");

        let result = self.perform_swaps(transaction, &backup_root);
        if let Err(error) = result {
            let rollback_error = self.rollback_journal(&journal).err();
            if rollback_error.is_none() {
                self.prepared.lock().unwrap().remove(&transaction.id);
            }
            return Err(match rollback_error {
                Some(rollback_error) => {
                    format!("{error} Rollback also failed: {rollback_error}")
                }
                None => error,
            });
        }
        result
    }

    fn perform_swaps(
        &self,
        transaction: &PreparedMarketplaceTransaction,
        backup_root: &Path,
    ) -> Result<Vec<InstallReceipt>, String> {
        let mut receipts = Vec::new();
        for package in &transaction.packages {
            let inspection = inspect_bundle(&package.bundle_path)?;
            validate_prepared_bundle(package, &inspection)?;
            extract_bundle(&package.bundle_path, &package.staged_path)?;
            let resolved = package.as_resolved();
            let receipt = install_receipt(&resolved, transaction.catalogue_sequence);
            write_install_metadata(
                &package.staged_path,
                &resolved,
                transaction.catalogue_sequence,
                &receipt,
            )?;

            let installed = safe_installed_package_dir(&self.packages_dir, &package.package.id)?;
            let backup = backup_root.join(&package.package.id);
            if installed.exists() {
                fs::rename(&installed, &backup).map_err(|error| {
                    format!(
                        "Unable to back up package '{}': {error}",
                        package.package.id
                    )
                })?;
            }
            fs::rename(&package.staged_path, &installed).map_err(|error| {
                format!(
                    "Unable to activate package '{}': {error}",
                    package.package.id
                )
            })?;
            receipts.push(receipt);
        }
        Ok(receipts)
    }

    pub fn finish_commit(&self, transaction_id: &str) -> Result<(), String> {
        let journal = self.read_journal()?;
        if journal.transaction_id != transaction_id {
            return Err("Marketplace install journal does not match the transaction".to_string());
        }
        self.remove_journal()?;
        let transaction_root = self.transaction_root(transaction_id)?;
        let _ = fs::remove_dir_all(&transaction_root);
        self.prepared.lock().unwrap().remove(transaction_id);
        Ok(())
    }

    pub fn rollback_commit(&self, transaction_id: &str) -> Result<(), String> {
        let journal = self.read_journal()?;
        if journal.transaction_id != transaction_id {
            return Err("Marketplace install journal does not match the transaction".to_string());
        }
        self.rollback_journal(&journal)?;
        self.prepared.lock().unwrap().remove(transaction_id);
        Ok(())
    }

    pub fn abort_commit(&self, transaction_id: &str) -> Result<(), String> {
        if self.journal_path().exists() || self.journal_backup_path().exists() {
            self.rollback_commit(transaction_id)
        } else {
            self.prepared.lock().unwrap().remove(transaction_id);
            Ok(())
        }
    }

    pub fn discard(&self, transaction_id: &str) -> Result<(), String> {
        if self.journal_path().exists() || self.journal_backup_path().exists() {
            let journal = self.read_journal()?;
            if journal.transaction_id == transaction_id {
                return Err(
                    "A Marketplace transaction being committed cannot be discarded".to_string(),
                );
            }
        }
        let transaction = self.prepared.lock().unwrap().remove(transaction_id);
        if transaction.is_none() {
            return Ok(());
        }
        let root = self.transaction_root(transaction_id)?;
        if root.exists() {
            fs::remove_dir_all(root)
                .map_err(|error| format!("Unable to discard Marketplace transaction: {error}"))?;
        }
        Ok(())
    }

    pub fn write_package_state(&self, state: &PackageStateFile) -> Result<(), String> {
        let raw = serde_json::to_vec_pretty(state)
            .map_err(|error| format!("Unable to serialize package state: {error}"))?;
        atomic_write(&self.state_path, &raw)
    }

    fn recover_incomplete_commit(&self) -> Result<(), String> {
        if !self.journal_path().exists() && !self.journal_backup_path().exists() {
            return Ok(());
        }
        let journal = self.read_journal()?;
        self.rollback_journal(&journal)
    }

    fn cleanup_stale_transactions(&self) -> Result<(), String> {
        let transactions = self.root.join("transactions");
        for entry in fs::read_dir(&transactions)
            .map_err(|error| format!("Unable to inspect Marketplace transactions: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("Unable to inspect Marketplace transaction: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|error| {
                    format!("Unable to remove stale Marketplace transaction: {error}")
                })?;
            } else {
                fs::remove_file(&path).map_err(|error| {
                    format!("Unable to remove stale Marketplace transaction file: {error}")
                })?;
            }
        }
        Ok(())
    }

    fn rollback_journal(&self, journal: &MarketplaceInstallJournal) -> Result<(), String> {
        validate_transaction_id(&journal.transaction_id)?;
        let transaction_root = self.transaction_root(&journal.transaction_id)?;
        let backup_root = transaction_root.join("backups");
        let previously_installed = journal
            .previously_installed
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        for package_id in journal.package_ids.iter().rev() {
            validate_package_id(package_id)?;
            let installed = safe_installed_package_dir(&self.packages_dir, package_id)?;
            let backup = backup_root.join(package_id);
            if previously_installed.contains(package_id) {
                if backup.exists() {
                    if installed.exists() {
                        fs::remove_dir_all(&installed).map_err(|error| {
                            format!("Unable to remove failed package '{package_id}': {error}")
                        })?;
                    }
                    fs::rename(&backup, &installed).map_err(|error| {
                        format!("Unable to restore package '{package_id}': {error}")
                    })?;
                }
            } else if installed.exists() {
                fs::remove_dir_all(&installed).map_err(|error| {
                    format!("Unable to remove new package '{package_id}': {error}")
                })?;
            }
        }
        self.write_package_state(&journal.previous_state)?;
        self.remove_journal()?;
        if transaction_root.exists() {
            fs::remove_dir_all(transaction_root)
                .map_err(|error| format!("Unable to clean rolled back transaction: {error}"))?;
        }
        Ok(())
    }

    fn transaction_root(&self, transaction_id: &str) -> Result<PathBuf, String> {
        validate_transaction_id(transaction_id)?;
        Ok(self.root.join("transactions").join(transaction_id))
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join("install-journal.json")
    }

    fn write_journal(&self, journal: &MarketplaceInstallJournal) -> Result<(), String> {
        let raw = serde_json::to_vec_pretty(journal)
            .map_err(|error| format!("Unable to serialize Marketplace install journal: {error}"))?;
        atomic_write(&self.journal_path(), &raw)
    }

    fn read_journal(&self) -> Result<MarketplaceInstallJournal, String> {
        let raw = super::marketplace::read_atomic_file(&self.journal_path())
            .map_err(|error| format!("Unable to read Marketplace install journal: {error}"))?;
        let journal: MarketplaceInstallJournal = serde_json::from_slice(&raw)
            .map_err(|error| format!("Marketplace install journal is invalid: {error}"))?;
        if journal.schema != INSTALL_JOURNAL_SCHEMA {
            return Err(format!(
                "Unsupported Marketplace install journal '{}'.",
                journal.schema
            ));
        }
        Ok(journal)
    }

    fn remove_journal(&self) -> Result<(), String> {
        let path = self.journal_path();
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!("Unable to remove Marketplace install journal: {error}")
            })?;
        }
        let backup = self.journal_backup_path();
        if backup.exists() {
            fs::remove_file(backup)
                .map_err(|error| format!("Unable to remove Marketplace journal backup: {error}"))?;
        }
        Ok(())
    }

    fn journal_backup_path(&self) -> PathBuf {
        self.root.join(".install-journal.json.backup")
    }
}

impl PreparedMarketplacePackage {
    pub fn package_id(&self) -> &str {
        &self.package.id
    }

    fn as_resolved(&self) -> ResolvedMarketplacePackage {
        ResolvedMarketplacePackage {
            package: self.package.clone(),
            version: self.version.clone(),
            artifact: self.artifact.clone(),
            developer: self.developer.clone(),
            signing_key: self.signing_key.clone(),
            requested: false,
        }
    }
}

fn normalize_requested_packages(values: Vec<String>) -> Result<HashSet<String>, String> {
    if values.is_empty() {
        return Err("Select at least one Marketplace package".to_string());
    }
    if values.len() > 64 {
        return Err("A Marketplace transaction cannot contain more than 64 selections".to_string());
    }
    let mut requested = HashSet::new();
    for value in values {
        validate_package_id(&value)?;
        if !requested.insert(value.clone()) {
            return Err(format!(
                "Marketplace package '{value}' was selected more than once"
            ));
        }
    }
    Ok(requested)
}

fn resolve_install_graph(
    catalogue: &MarketplaceCatalogue,
    requested: &HashSet<String>,
    host_version: &Version,
) -> Result<Vec<ResolvedMarketplacePackage>, String> {
    let packages = catalogue
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<HashMap<_, _>>();
    let developers = catalogue
        .developers
        .iter()
        .map(|developer| (developer.id.as_str(), developer))
        .collect::<HashMap<_, _>>();
    let mut resolved = HashMap::<String, ResolvedMarketplacePackage>::new();
    let mut order = Vec::new();
    let mut visiting = HashSet::new();
    let mut roots = requested.iter().cloned().collect::<Vec<_>>();
    roots.sort();
    for package_id in roots {
        resolve_package(
            &package_id,
            None,
            true,
            &packages,
            &developers,
            host_version,
            &mut resolved,
            &mut order,
            &mut visiting,
        )?;
    }
    Ok(order
        .into_iter()
        .filter_map(|package_id| resolved.remove(&package_id))
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn resolve_package(
    package_id: &str,
    requirement: Option<&VersionReq>,
    requested: bool,
    packages: &HashMap<&str, &MarketplacePackage>,
    developers: &HashMap<&str, &MarketplaceDeveloper>,
    host_version: &Version,
    resolved: &mut HashMap<String, ResolvedMarketplacePackage>,
    order: &mut Vec<String>,
    visiting: &mut HashSet<String>,
) -> Result<(), String> {
    if visiting.contains(package_id) {
        return Err(format!(
            "Marketplace dependency cycle includes '{package_id}'"
        ));
    }
    if let Some(existing) = resolved.get_mut(package_id) {
        let version = Version::parse(&existing.version.version).unwrap();
        if requirement.is_some_and(|requirement| !requirement.matches(&version)) {
            return Err(format!(
                "Marketplace dependency constraints conflict for '{package_id}'"
            ));
        }
        existing.requested |= requested;
        return Ok(());
    }
    visiting.insert(package_id.to_string());
    let package = packages
        .get(package_id)
        .copied()
        .ok_or_else(|| format!("Marketplace package '{package_id}' does not exist"))?;
    if package.status != MarketplaceStatus::Active {
        return Err(format!("Marketplace package '{package_id}' is not active"));
    }
    let developer = developers
        .get(package.developer_id.as_str())
        .copied()
        .ok_or_else(|| {
            format!(
                "Marketplace developer '{}' does not exist",
                package.developer_id
            )
        })?;
    let (version, artifact) = select_version(package, requirement, host_version)?;
    let signing_key = developer
        .signing_keys
        .iter()
        .find(|key| key.id == artifact.signing_key_id)
        .filter(|key| key.status == MarketplaceStatus::Active)
        .ok_or_else(|| {
            format!(
                "Marketplace signing key '{}' is not active",
                artifact.signing_key_id
            )
        })?;
    let candidate = ResolvedMarketplacePackage {
        package: package.clone(),
        version: version.clone(),
        artifact: artifact.clone(),
        developer: developer.clone(),
        signing_key: signing_key.clone(),
        requested,
    };
    resolved.insert(package_id.to_string(), candidate);
    for dependency in version
        .dependencies
        .iter()
        .filter(|dependency| !dependency.optional)
    {
        let requirement = VersionReq::parse(&dependency.version)
            .map_err(|error| format!("Marketplace dependency requirement is invalid: {error}"))?;
        resolve_package(
            &dependency.package_id,
            Some(&requirement),
            false,
            packages,
            developers,
            host_version,
            resolved,
            order,
            visiting,
        )?;
    }
    visiting.remove(package_id);
    order.push(package_id.to_string());
    Ok(())
}

fn select_version<'a>(
    package: &'a MarketplacePackage,
    requirement: Option<&VersionReq>,
    host_version: &Version,
) -> Result<(&'a MarketplacePackageVersion, &'a MarketplaceArtifact), String> {
    let platform = current_platform()?;
    let mut candidates = package
        .versions
        .iter()
        .filter_map(|version| {
            let parsed = Version::parse(&version.version).ok()?;
            let runtime = Version::parse(&version.runtime_api).ok()?;
            let minimum_ok = version
                .min_bakingrl_version
                .as_ref()
                .and_then(|minimum| Version::parse(minimum).ok())
                .is_none_or(|minimum| host_version >= &minimum);
            let artifact = version
                .artifacts
                .iter()
                .find(|artifact| artifact.platform == platform)
                .or_else(|| {
                    version
                        .artifacts
                        .iter()
                        .find(|artifact| artifact.platform == "any")
                })?;
            (version.status == MarketplaceStatus::Active
                && version.channel == MarketplaceChannel::Stable
                && runtime.major == 2
                && runtime.minor == 3
                && runtime.pre.is_empty()
                && minimum_ok
                && requirement.is_none_or(|requirement| requirement.matches(&parsed)))
            .then_some((parsed, version, artifact))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, version, artifact)| (version, artifact))
        .ok_or_else(|| {
            format!(
                "Marketplace package '{}' has no compatible stable version for {platform}",
                package.id
            )
        })
}

fn validate_bundle_concordance(
    resolved: &ResolvedMarketplacePackage,
    inspection: &BundleInspection,
) -> Result<(), String> {
    if inspection.sha256 != resolved.artifact.bundle_sha256 {
        return Err(format!(
            "Bundle hash does not match Marketplace for '{}'",
            resolved.package.id
        ));
    }
    if !inspection.hashes_present
        || !inspection.signature_present
        || !inspection.signature_verified
        || inspection.signature_public_key.as_deref() != Some(&resolved.signing_key.public_key)
    {
        return Err(format!(
            "Bundle signature does not match publisher key for '{}'",
            resolved.package.id
        ));
    }
    let manifest = &inspection.manifest;
    if manifest.id() != resolved.package.id
        || manifest.version() != resolved.version.version
        || manifest.bakingrl_api() != resolved.version.runtime_api
    {
        return Err(format!(
            "Bundle manifest does not match Marketplace record for '{}'",
            resolved.package.id
        ));
    }
    validate_manifest_dependencies(resolved, inspection)?;
    validate_manifest_permissions(resolved, inspection)?;
    validate_manifest_runtime(resolved, inspection)
}

fn validate_prepared_bundle(
    package: &PreparedMarketplacePackage,
    inspection: &BundleInspection,
) -> Result<(), String> {
    validate_bundle_concordance(&package.as_resolved(), inspection)
}

fn validate_manifest_dependencies(
    resolved: &ResolvedMarketplacePackage,
    inspection: &BundleInspection,
) -> Result<(), String> {
    let mut manifest = inspection
        .manifest
        .dependencies_v4()
        .iter()
        .map(|dependency| {
            (
                dependency.package_id.clone(),
                dependency.version.clone().unwrap_or_default(),
                dependency.optional,
            )
        })
        .collect::<Vec<_>>();
    let mut catalogue = resolved
        .version
        .dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.package_id.clone(),
                dependency.version.clone(),
                dependency.optional,
            )
        })
        .collect::<Vec<_>>();
    manifest.sort();
    catalogue.sort();
    if manifest != catalogue {
        return Err(format!(
            "Bundle dependencies do not match Marketplace for '{}'",
            resolved.package.id
        ));
    }
    Ok(())
}

fn validate_manifest_permissions(
    resolved: &ResolvedMarketplacePackage,
    inspection: &BundleInspection,
) -> Result<(), String> {
    let manifest = inspection
        .manifest
        .permissions_v4()
        .cloned()
        .unwrap_or_default();
    let catalogue = plugin_permissions(&resolved.version.permissions);
    if manifest != catalogue {
        return Err(format!(
            "Bundle permissions do not match Marketplace for '{}'",
            resolved.package.id
        ));
    }
    Ok(())
}

fn validate_manifest_runtime(
    resolved: &ResolvedMarketplacePackage,
    inspection: &BundleInspection,
) -> Result<(), String> {
    let runtime = inspection.manifest.runtime_v4();
    if runtime.and_then(|runtime| runtime.node.as_ref()).is_some() != resolved.version.runtime.node
    {
        return Err(format!(
            "Bundle Node runtime does not match Marketplace for '{}'",
            resolved.package.id
        ));
    }
    let manifest_sidecars = runtime
        .into_iter()
        .flat_map(|runtime| &runtime.sidecars)
        .map(|sidecar| (sidecar.id.clone(), sidecar.platforms.clone()))
        .collect::<Vec<_>>();
    let catalogue_sidecars = resolved
        .version
        .runtime
        .sidecars
        .iter()
        .map(|sidecar| (sidecar.id.clone(), sidecar.platforms.clone()))
        .collect::<Vec<_>>();
    validate_sidecar_runtime_concordance(
        &resolved.package.id,
        &resolved.artifact.platform,
        &manifest_sidecars,
        &catalogue_sidecars,
    )?;
    let mut manifest_webviews = inspection
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .map(|webview| (webview.id.clone(), webview.kind.clone().unwrap_or_default()))
        .collect::<Vec<_>>();
    let mut catalogue_webviews = resolved
        .version
        .runtime
        .webviews
        .iter()
        .map(|webview| (webview.id.clone(), webview.kind.clone()))
        .collect::<Vec<_>>();
    manifest_webviews.sort();
    catalogue_webviews.sort();
    if manifest_webviews != catalogue_webviews {
        return Err(format!(
            "Bundle webviews do not match Marketplace for '{}'",
            resolved.package.id
        ));
    }
    Ok(())
}

fn validate_sidecar_runtime_concordance(
    package_id: &str,
    artifact_platform: &str,
    manifest_sidecars: &[(String, Vec<String>)],
    catalogue_sidecars: &[(String, Vec<String>)],
) -> Result<(), String> {
    let manifest_by_id = manifest_sidecars
        .iter()
        .map(|(id, platforms)| (id.as_str(), platforms))
        .collect::<HashMap<_, _>>();
    let catalogue_by_id = catalogue_sidecars
        .iter()
        .map(|(id, platforms)| (id.as_str(), platforms))
        .collect::<HashMap<_, _>>();

    if manifest_by_id.len() != catalogue_by_id.len()
        || manifest_by_id
            .keys()
            .any(|id| !catalogue_by_id.contains_key(id))
    {
        return Err(format!(
            "Bundle sidecars do not match Marketplace for '{package_id}'"
        ));
    }

    for (id, manifest_platforms) in manifest_by_id {
        let catalogue_platforms = catalogue_by_id[id];
        let catalogue_supports_artifact = catalogue_platforms
            .iter()
            .any(|platform| platform == artifact_platform);
        let manifest_supports_artifact = manifest_platforms.is_empty()
            || manifest_platforms
                .iter()
                .any(|platform| platform == artifact_platform);
        let manifest_only_declares_catalogue_platforms = manifest_platforms
            .iter()
            .all(|platform| catalogue_platforms.contains(platform));

        if !catalogue_supports_artifact
            || !manifest_supports_artifact
            || !manifest_only_declares_catalogue_platforms
        {
            return Err(format!(
                "Bundle sidecars do not match Marketplace for '{package_id}'"
            ));
        }
    }

    Ok(())
}

fn plugin_permissions(value: &MarketplacePermissions) -> PluginPermissionsV4 {
    PluginPermissionsV4 {
        bus: PluginBusPermissionsV4 {
            read: value.bus.read.clone(),
            publish: value.bus.publish.clone(),
        },
        registry: PluginRegistryPermissionsV4 {
            read: value.registry.read.clone(),
            write: value.registry.write.clone(),
        },
        network: PluginNetworkPermissionsV4 {
            http: value
                .network
                .http
                .iter()
                .map(plugin_network_endpoint)
                .collect(),
            websocket: value
                .network
                .websocket
                .iter()
                .map(plugin_network_endpoint)
                .collect(),
            listen: value
                .network
                .listen
                .iter()
                .map(|endpoint| PluginListenEndpointV4 {
                    transport: endpoint.transport.clone(),
                    host: endpoint.host.clone(),
                    ports: plugin_ports(&endpoint.ports),
                })
                .collect(),
        },
        storage: PluginStoragePermissionsV4 {
            read: value.storage.read.clone(),
            write: value.storage.write.clone(),
        },
    }
}

fn plugin_network_endpoint(value: &MarketplaceNetworkEndpoint) -> PluginNetworkEndpointV4 {
    PluginNetworkEndpointV4 {
        scheme: value.scheme.clone(),
        host: value.host.clone(),
        ports: plugin_ports(&value.ports),
        path_prefixes: value.path_prefixes.clone(),
    }
}

fn plugin_ports(value: &MarketplacePorts) -> PluginNetworkPortsV4 {
    match value {
        MarketplacePorts::Any(value) => PluginNetworkPortsV4::Any(value.clone()),
        MarketplacePorts::Ports(values) => PluginNetworkPortsV4::Ports(values.clone()),
    }
}

fn install_receipt(
    resolved: &ResolvedMarketplacePackage,
    catalogue_sequence: u64,
) -> InstallReceipt {
    InstallReceipt {
        package_id: resolved.package.id.clone(),
        version: resolved.version.version.clone(),
        source: format!(
            "marketplace:{catalogue_sequence}:{}@{}",
            resolved.package.id, resolved.version.version
        ),
        bundle_sha256: resolved.artifact.bundle_sha256.clone(),
        installed_at_ms: now_millis(),
    }
}

fn write_install_metadata(
    staged_path: &Path,
    resolved: &ResolvedMarketplacePackage,
    catalogue_sequence: u64,
    receipt: &InstallReceipt,
) -> Result<(), String> {
    let receipt = serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("Unable to serialize install receipt: {error}"))?;
    fs::write(staged_path.join("bakingrl.install.json"), receipt)
        .map_err(|error| format!("Unable to write install receipt: {error}"))?;
    let provenance = MarketplaceProvenance {
        schema: PROVENANCE_SCHEMA.to_string(),
        catalogue_sequence,
        package_id: resolved.package.id.clone(),
        version: resolved.version.version.clone(),
        developer_id: resolved.developer.id.clone(),
        signing_key_id: resolved.signing_key.id.clone(),
        bundle_sha256: resolved.artifact.bundle_sha256.clone(),
        bundle_url: resolved.artifact.bundle_url.clone(),
    };
    let provenance = serde_json::to_vec_pretty(&provenance)
        .map_err(|error| format!("Unable to serialize Marketplace provenance: {error}"))?;
    fs::write(staged_path.join("bakingrl.marketplace.json"), provenance)
        .map_err(|error| format!("Unable to write Marketplace provenance: {error}"))
}

fn public_key_fingerprint(public_key: &str) -> Result<String, String> {
    let decoded = BASE64_STANDARD
        .decode(public_key)
        .map_err(|error| format!("Publisher public key is invalid: {error}"))?;
    if decoded.len() != 32 || BASE64_STANDARD.encode(&decoded) != public_key {
        return Err("Publisher public key is invalid".to_string());
    }
    Ok(hex::encode(Sha256::digest(decoded)))
}

fn install_operation(
    installed: Option<&str>,
    candidate: &str,
) -> Result<MarketplaceInstallOperation, String> {
    let Some(installed) = installed else {
        return Ok(MarketplaceInstallOperation::Install);
    };
    let installed = Version::parse(installed)
        .map_err(|error| format!("Installed package version is invalid: {error}"))?;
    let candidate = Version::parse(candidate)
        .map_err(|error| format!("Marketplace package version is invalid: {error}"))?;
    Ok(match candidate.cmp(&installed) {
        std::cmp::Ordering::Greater => MarketplaceInstallOperation::Update,
        std::cmp::Ordering::Less => MarketplaceInstallOperation::Downgrade,
        std::cmp::Ordering::Equal => MarketplaceInstallOperation::Reinstall,
    })
}

fn current_platform() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        ("windows", "x86_64") => Ok("windows-x64"),
        (os, arch) => Err(format!("Marketplace does not support platform {os}-{arch}")),
    }
}

fn validate_package_id(value: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let first_is_valid = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    let mut has_separator = false;
    let mut previous_was_separator = false;
    let rest_is_valid = bytes.all(|byte| {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_was_separator = false;
            true
        } else if matches!(byte, b'.' | b'-') && !previous_was_separator {
            has_separator = true;
            previous_was_separator = true;
            true
        } else {
            false
        }
    });
    if !first_is_valid || !rest_is_valid || !has_separator || previous_was_separator {
        return Err(format!("Marketplace package id '{value}' is invalid"));
    }
    Ok(())
}

fn validate_transaction_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("Marketplace transaction id is invalid".to_string());
    }
    Ok(())
}

fn unique_transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("install-{}-{nanos}", std::process::id())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_catalogue(cycle: bool) -> MarketplaceCatalogue {
        let dependency_a = serde_json::json!({
            "packageId": "com.example.dependency",
            "version": "^1.0.0",
            "optional": false
        });
        let dependency_b = serde_json::json!({
            "packageId": "com.example.root",
            "version": "^1.0.0",
            "optional": false
        });
        let package = |id: &str, dependencies: Vec<serde_json::Value>| {
            serde_json::json!({
                "schema": "bakingrl.marketplace-package/2",
                "id": id,
                "developerId": "example",
                "status": "active",
                "repo": "https://github.com/example/plugins",
                "listing": {
                    "sourceUrl": "https://raw.githubusercontent.com/example/plugins/main/listing.json",
                    "snapshotSha256": "a".repeat(64),
                    "snapshot": {
                        "schema": "bakingrl.marketplace-listing/2",
                        "packageId": id,
                        "displayName": id,
                        "shortDescription": "Short",
                        "longDescription": "Long",
                        "tags": [],
                        "repo": "https://github.com/example/plugins",
                        "media": { "icon": null, "banner": null, "screenshots": [] },
                        "links": {
                            "docs": "https://github.com/example/plugins",
                            "support": "https://github.com/example/plugins/issues"
                        }
                    }
                },
                "versions": [{
                    "version": "1.0.0",
                    "status": "active",
                    "channel": "stable",
                    "runtimeApi": "2.3.0",
                    "runtime": { "node": false, "sidecars": [], "webviews": [] },
                    "dependencies": dependencies,
                    "permissions": {
                        "bus": { "read": [], "publish": [] },
                        "registry": { "read": [], "write": [] },
                        "network": { "http": [], "websocket": [], "listen": [] },
                        "storage": { "read": [], "write": [] }
                    },
                    "nativeCapabilities": { "node": null, "sidecars": [], "surfaces": [] },
                    "artifacts": [{
                        "platform": "any",
                        "bundleUrl": "https://github.com/example/plugins/releases/download/v1.0.0/plugin.brlp",
                        "bundleSha256": "b".repeat(64),
                        "signingKeyId": "example-release-1"
                    }],
                    "reviewedAt": "2026-07-16T12:00:00Z"
                }]
            })
        };
        serde_json::from_value(serde_json::json!({
            "schema": "bakingrl.marketplace/2",
            "sequence": 1,
            "generatedAt": "2026-07-16T12:00:00Z",
            "expiresAt": "2026-07-23T12:00:00Z",
            "sections": { "recommended": [], "new": [], "firstRun": [] },
            "developers": [{
                "id": "example",
                "name": "Example",
                "kind": "organization",
                "verification": "verified",
                "signingKeys": [{
                    "id": "example-release-1",
                    "algorithm": "ed25519",
                    "publicKey": BASE64_STANDARD.encode([4u8; 32]),
                    "status": "active"
                }]
            }],
            "packages": [
                package("com.example.root", vec![dependency_a]),
                package(
                    "com.example.dependency",
                    if cycle { vec![dependency_b] } else { Vec::new() }
                )
            ]
        }))
        .unwrap()
    }

    #[test]
    fn resolves_required_dependencies_before_requested_packages() {
        let catalogue = test_catalogue(false);
        let requested = HashSet::from(["com.example.root".to_string()]);
        let resolved =
            resolve_install_graph(&catalogue, &requested, &Version::parse("0.10.0").unwrap())
                .unwrap();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].package.id, "com.example.dependency");
        assert_eq!(resolved[1].package.id, "com.example.root");
    }

    #[test]
    fn rejects_dependency_cycles() {
        let catalogue = test_catalogue(true);
        let requested = HashSet::from(["com.example.root".to_string()]);
        let error =
            resolve_install_graph(&catalogue, &requested, &Version::parse("0.10.0").unwrap())
                .unwrap_err();

        assert!(error.contains("cycle"));
    }

    #[test]
    fn requires_explicit_publisher_acceptance() {
        let directory = tempfile::tempdir().unwrap();
        let packages_dir = directory.path().join("packages");
        fs::create_dir_all(&packages_dir).unwrap();
        let state_path = directory.path().join("package_state.json");
        let installer =
            MarketplaceInstaller::new(directory.path(), &packages_dir, &state_path).unwrap();
        installer.prepared.lock().unwrap().insert(
            "install-test".to_string(),
            PreparedMarketplaceTransaction {
                id: "install-test".to_string(),
                catalogue_sequence: 1,
                packages: Vec::new(),
                publishers: Vec::new(),
                required_trust_ids: vec!["example:fingerprint".to_string()],
            },
        );

        let error = installer
            .transaction_for_commit("install-test", &[])
            .unwrap_err();
        assert!(error.contains("trust was not accepted"));
        assert!(installer
            .transaction_for_commit("install-test", &["example:fingerprint".to_string()])
            .is_ok());
    }

    #[test]
    fn recovers_enabled_state_when_interrupted_before_package_swaps() {
        let directory = tempfile::tempdir().unwrap();
        let packages_dir = directory.path().join("packages");
        fs::create_dir_all(&packages_dir).unwrap();
        let state_path = directory.path().join("package_state.json");
        let transaction_id = "install-before-swap";
        let installer =
            MarketplaceInstaller::new(directory.path(), &packages_dir, &state_path).unwrap();
        fs::create_dir_all(
            directory
                .path()
                .join("marketplace/transactions")
                .join(transaction_id),
        )
        .unwrap();
        let transaction = PreparedMarketplaceTransaction {
            id: transaction_id.to_string(),
            catalogue_sequence: 1,
            packages: Vec::new(),
            publishers: Vec::new(),
            required_trust_ids: Vec::new(),
        };
        let previous_state = PackageStateFile {
            enabled: HashMap::from([("com.example.package".to_string(), true)]),
        };
        installer
            .begin_commit(&transaction, &previous_state)
            .unwrap();
        installer
            .write_package_state(&PackageStateFile {
                enabled: HashMap::from([("com.example.package".to_string(), false)]),
            })
            .unwrap();
        drop(installer);

        MarketplaceInstaller::new(directory.path(), &packages_dir, &state_path).unwrap();

        let restored: PackageStateFile =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(restored.enabled, previous_state.enabled);
        assert!(!directory
            .path()
            .join("marketplace/install-journal.json")
            .exists());
        assert!(!directory
            .path()
            .join("marketplace/transactions")
            .join(transaction_id)
            .exists());
    }

    #[test]
    fn package_ids_match_marketplace_separator_rules() {
        assert!(validate_package_id("com.bakingrl.player-streak").is_ok());
        assert!(validate_package_id("bakingrl.obs-gateway").is_ok());

        for invalid in ["bakingrl", "a.-b", "a-.b", "a..b", "a--b", ".a", "a."] {
            assert!(validate_package_id(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn accepts_artifact_scoped_sidecar_against_multiplatform_catalogue() {
        let manifest = vec![("gateway".to_string(), Vec::new())];
        let catalogue = vec![(
            "gateway".to_string(),
            vec![
                "darwin-arm64".to_string(),
                "darwin-x64".to_string(),
                "linux-x64".to_string(),
                "windows-x64".to_string(),
            ],
        )];

        assert!(validate_sidecar_runtime_concordance(
            "bakingrl.obs-gateway",
            "windows-x64",
            &manifest,
            &catalogue,
        )
        .is_ok());
    }

    #[test]
    fn rejects_sidecar_when_catalogue_does_not_cover_selected_artifact() {
        let manifest = vec![("gateway".to_string(), Vec::new())];
        let catalogue = vec![("gateway".to_string(), vec!["linux-x64".to_string()])];

        let error = validate_sidecar_runtime_concordance(
            "bakingrl.obs-gateway",
            "windows-x64",
            &manifest,
            &catalogue,
        )
        .unwrap_err();

        assert!(error.contains("Bundle sidecars do not match Marketplace"));
    }

    #[test]
    fn recovers_packages_and_enabled_state_after_interrupted_swap() {
        let directory = tempfile::tempdir().unwrap();
        let packages_dir = directory.path().join("packages");
        fs::create_dir_all(&packages_dir).unwrap();
        let state_path = directory.path().join("package_state.json");
        let transaction_root = directory
            .path()
            .join("marketplace/transactions/install-recovery");
        let backup = transaction_root.join("backups/com.example.existing");
        let installed_existing = packages_dir.join("com.example.existing");
        let installed_new = packages_dir.join("com.example.new");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("marker.txt"), "old").unwrap();
        fs::create_dir_all(&installed_existing).unwrap();
        fs::write(installed_existing.join("marker.txt"), "new").unwrap();
        fs::create_dir_all(&installed_new).unwrap();
        fs::write(installed_new.join("marker.txt"), "new package").unwrap();

        let previous_state = PackageStateFile {
            enabled: HashMap::from([
                ("com.example.existing".to_string(), true),
                ("com.example.other".to_string(), false),
            ]),
        };
        let journal = MarketplaceInstallJournal {
            schema: INSTALL_JOURNAL_SCHEMA.to_string(),
            transaction_id: "install-recovery".to_string(),
            package_ids: vec![
                "com.example.existing".to_string(),
                "com.example.new".to_string(),
            ],
            previously_installed: vec!["com.example.existing".to_string()],
            previous_state: previous_state.clone(),
        };
        let marketplace_root = directory.path().join("marketplace");
        fs::create_dir_all(&marketplace_root).unwrap();
        fs::write(
            marketplace_root.join("install-journal.json"),
            serde_json::to_vec_pretty(&journal).unwrap(),
        )
        .unwrap();

        MarketplaceInstaller::new(directory.path(), &packages_dir, &state_path).unwrap();

        assert_eq!(
            fs::read_to_string(installed_existing.join("marker.txt")).unwrap(),
            "old"
        );
        assert!(!installed_new.exists());
        let restored: PackageStateFile =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(restored.enabled, previous_state.enabled);
        assert!(!marketplace_root.join("install-journal.json").exists());
        assert!(!transaction_root.exists());
    }
}
