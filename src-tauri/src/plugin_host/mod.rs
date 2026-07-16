mod descriptors;
mod diagnostics;
pub(crate) mod extension_host_runtime;
mod json_store;
mod marketplace;
mod marketplace_transaction;
mod package_files;
mod plugin_storage;
mod runtime_specs;
mod service_registry;
mod settings_contract;
pub(crate) mod sidecar_runtime;
mod surface_runtime;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, Window};
use tracing::{info, warn};

use crate::bus::{BusEvent, EventBus};
use crate::models::{AppSettings, GameEvent, PackageSettingsFile, PackageStateFile};
use crate::plugin_package::bundle::BundleInspection;
use crate::plugin_package::install::{
    download_bundle_to_file, inspect_bundle_file, install_bundle_from_file,
    parse_install_deep_link, InstallReceipt,
};
use crate::plugin_package::manifest::{PluginPackageManifest, HOST_RUNTIME_API_VERSION};
use crate::registry::Registry;
use descriptors::{
    apply_graph_diagnostics, compatibility_for_manifest, descriptor_for_manifest,
    supported_runtime_api_label,
};
pub use descriptors::{
    PackageContributionsDescriptor, PackageDescriptor, PackageStatus, PreparedPackageInstall,
};
pub use diagnostics::{
    PluginDiagnosticEvent, PluginDiagnosticInput, PluginDiagnosticSeverity, PluginDiagnosticsStore,
};
use extension_host_runtime::{ExtensionHostRuntimeManager, ExtensionHostRuntimeSpec};
use json_store::{read_json_or_default, write_json};
use marketplace::MarketplaceService;
pub use marketplace::MarketplaceSnapshot;
use marketplace_transaction::MarketplaceInstaller;
pub use marketplace_transaction::{MarketplaceInstallPlan, MarketplaceInstallResult};
use package_files::{
    find_first_bundle, format_command_error, is_remote_package_source, parse_export_ref,
    read_binary_package_file, safe_installed_package_dir, safe_package_relative_path,
};
use runtime_specs::{
    extension_host_specs_for_records, sidecar_service_specs_for_records, sidecar_specs_for_records,
    SidecarServiceRuntimeSpec,
};
use service_registry::{CommandCallRouter, ServiceCallClient, ServiceCallRouter};
use settings_contract::{
    delete_package_secret as delete_keychain_package_secret, merge_package_settings,
    merge_package_settings_with_schema, package_secret_configured, read_package_settings_schema,
    sanitize_package_settings_values, secret_definitions, secret_store_status,
    set_package_secret_configured, write_package_secret,
};
pub use settings_contract::{PackageConfigurationState, PackageSecretDescriptor};
use sidecar_runtime::{SidecarRuntimeManager, SidecarRuntimeSpec, SidecarRuntimeStatus};
use surface_runtime::{
    close_package_surfaces, open_surface, surface_window_label, SurfaceOpenOptions,
    SurfaceOpenRequest,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub app_version: String,
    pub runtime_api_version: String,
    pub supported_runtime_api: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageWebviewRuntimeDescriptor {
    pub package_id: String,
    pub webview_id: String,
    pub entry: String,
    pub kind: Option<String>,
    pub runtime_api: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageWebviewAssetPayload {
    pub contents_base64: String,
    pub content_type: String,
    pub path: String,
}

#[derive(Debug)]
struct PackageRecord {
    descriptor: PackageDescriptor,
    manifest: PluginPackageManifest,
}

#[derive(Debug, Clone)]
struct PendingPackageDeletion {
    previous_enabled: bool,
}

struct PackageRemovalStart {
    packages: Vec<PackageDescriptor>,
    started: bool,
}

pub struct PluginHost {
    app_handle: AppHandle,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    packages_dir: PathBuf,
    plugin_storage_dir: PathBuf,
    state_path: PathBuf,
    app_settings_path: PathBuf,
    package_settings_path: PathBuf,
    marketplace: MarketplaceService,
    marketplace_installer: MarketplaceInstaller,
    package_install_lock: Mutex<()>,
    records: Mutex<HashMap<String, PackageRecord>>,
    deleting_packages: Mutex<HashMap<String, PendingPackageDeletion>>,
    diagnostics: PluginDiagnosticsStore,
    command_router: CommandCallRouter,
    service_router: ServiceCallRouter,
    sidecar_services: Mutex<HashSet<String>>,
    #[allow(dead_code)]
    extension_host_runtimes: ExtensionHostRuntimeManager,
    #[allow(dead_code)]
    sidecar_runtimes: SidecarRuntimeManager,
}

impl PluginHost {
    pub fn new(
        app_handle: AppHandle,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
    ) -> Result<Self, String> {
        let app_data = app_handle
            .path()
            .app_local_data_dir()
            .map_err(|e| format!("Unable to resolve app data directory: {e}"))?;
        fs::create_dir_all(&app_data)
            .map_err(|e| format!("Unable to create app data directory: {e}"))?;

        let packages_dir = app_data.join("packages");
        fs::create_dir_all(&packages_dir)
            .map_err(|e| format!("Unable to create package directory: {e}"))?;
        let plugin_storage_dir = app_data.join("plugin-storage");
        fs::create_dir_all(&plugin_storage_dir)
            .map_err(|e| format!("Unable to create plugin storage directory: {e}"))?;
        let state_path = app_data.join("package_state.json");
        let marketplace = MarketplaceService::new(&app_data)?;
        let marketplace_installer =
            MarketplaceInstaller::new(&app_data, &packages_dir, &state_path)?;

        Ok(Self {
            app_handle,
            bus,
            registry,
            packages_dir,
            plugin_storage_dir,
            state_path,
            app_settings_path: app_data.join("app_settings.json"),
            package_settings_path: app_data.join("package_settings.json"),
            marketplace,
            marketplace_installer,
            package_install_lock: Mutex::new(()),
            records: Mutex::new(HashMap::new()),
            deleting_packages: Mutex::new(HashMap::new()),
            diagnostics: PluginDiagnosticsStore::default(),
            command_router: CommandCallRouter::default(),
            service_router: ServiceCallRouter::default(),
            sidecar_services: Mutex::new(HashSet::new()),
            extension_host_runtimes: ExtensionHostRuntimeManager::default(),
            sidecar_runtimes: SidecarRuntimeManager::default(),
        })
    }

    pub fn initialize(&self) {
        self.reload_packages();
    }

    pub fn packages_dir(&self) -> String {
        self.packages_dir.to_string_lossy().to_string()
    }

    pub async fn marketplace_snapshot(&self, refresh: bool) -> Result<MarketplaceSnapshot, String> {
        self.marketplace.snapshot(refresh).await
    }

    pub fn complete_marketplace_first_run(&self) -> Result<(), String> {
        self.marketplace.complete_first_run()
    }

    pub async fn prepare_marketplace_install(
        &self,
        package_ids: Vec<String>,
    ) -> Result<MarketplaceInstallPlan, String> {
        let snapshot = self.marketplace.snapshot(true).await?;
        if !snapshot.installable {
            return Err(
                "The verified Marketplace catalogue is expired and cannot be used for installation."
                    .to_string(),
            );
        }
        let installed_versions = self
            .records
            .lock()
            .unwrap()
            .iter()
            .map(|(package_id, record)| (package_id.clone(), record.descriptor.version.clone()))
            .collect::<HashMap<_, _>>();
        self.marketplace_installer
            .prepare(
                snapshot.catalogue,
                package_ids,
                &installed_versions,
                &self.marketplace.trusted_publisher_ids(),
                env!("CARGO_PKG_VERSION"),
            )
            .await
    }

    pub fn commit_marketplace_install(
        &self,
        transaction_id: String,
        accepted_publishers: Vec<String>,
    ) -> Result<MarketplaceInstallResult, String> {
        let _install_guard = self.package_install_lock.lock().unwrap();
        let transaction = self
            .marketplace_installer
            .transaction_for_commit(&transaction_id, &accepted_publishers)?;
        let previous_state = self.load_state();
        self.marketplace_installer
            .begin_commit(&transaction, &previous_state)?;
        let mut stopped_state = previous_state.clone();
        for package in &transaction.packages {
            stopped_state
                .enabled
                .insert(package.package_id().to_string(), false);
        }
        if let Err(error) = self
            .marketplace_installer
            .write_package_state(&stopped_state)
        {
            let rollback = self.marketplace_installer.abort_commit(&transaction_id);
            self.reload_packages();
            return Err(combine_rollback_error(error, rollback));
        }
        self.reload_packages();

        let receipts = match self.marketplace_installer.swap_packages(&transaction) {
            Ok(receipts) => receipts,
            Err(error) => {
                let rollback = self.marketplace_installer.abort_commit(&transaction_id);
                self.reload_packages();
                return Err(combine_rollback_error(error, rollback));
            }
        };

        let trust_pairs = transaction
            .publishers
            .iter()
            .map(|publisher| {
                (
                    publisher.developer_id.clone(),
                    publisher.key_fingerprint.clone(),
                )
            })
            .collect::<Vec<_>>();
        if let Err(error) = self.marketplace.trust_publishers(&trust_pairs) {
            let rollback = self.marketplace_installer.abort_commit(&transaction_id);
            self.reload_packages();
            return Err(combine_rollback_error(error, rollback));
        }

        let mut next_state = previous_state;
        for package in &transaction.packages {
            next_state
                .enabled
                .insert(package.package_id().to_string(), true);
        }
        if let Err(error) = self.marketplace_installer.write_package_state(&next_state) {
            let rollback = self.marketplace_installer.abort_commit(&transaction_id);
            self.reload_packages();
            return Err(combine_rollback_error(error, rollback));
        }
        if let Err(error) = self.marketplace_installer.finish_commit(&transaction_id) {
            let rollback = self.marketplace_installer.abort_commit(&transaction_id);
            self.reload_packages();
            return Err(combine_rollback_error(error, rollback));
        }
        self.reload_packages();
        Ok(MarketplaceInstallResult {
            transaction_id,
            receipts,
        })
    }

    pub fn discard_marketplace_install(&self, transaction_id: String) -> Result<(), String> {
        let _install_guard = self.package_install_lock.lock().unwrap();
        self.marketplace_installer.discard(&transaction_id)
    }

    pub fn frontend_dist_dir(&self) -> PathBuf {
        let mut candidates = Vec::new();
        if let Ok(resource_dir) = self.app_handle.path().resource_dir() {
            candidates.push(resource_dir.join("build"));
        }
        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(current_dir.join("build"));
            candidates.push(current_dir.join("../build"));
        }
        candidates
            .into_iter()
            .find(|path| path.join("index.html").exists())
            .unwrap_or_else(|| PathBuf::from("build"))
    }

    fn downloads_dir(&self) -> PathBuf {
        self.packages_dir.join(".downloads")
    }

    fn git_sources_dir(&self) -> PathBuf {
        self.packages_dir.join(".git-sources")
    }

    pub fn list_packages(&self) -> Vec<PackageDescriptor> {
        let mut packages: Vec<_> = self
            .records
            .lock()
            .unwrap()
            .values()
            .map(|record| record.descriptor.clone())
            .collect();
        let extension_host_statuses = self.extension_host_runtimes.status_map();
        apply_extension_host_runtime_statuses(&mut packages, &extension_host_statuses);
        let sidecar_statuses = self.sidecar_runtimes.status_map();
        apply_sidecar_runtime_statuses(&mut packages, &sidecar_statuses);
        self.apply_package_statuses(&mut packages);
        packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        packages
    }

    pub fn diagnostics(&self) -> PluginDiagnosticsStore {
        self.diagnostics.clone()
    }

    pub fn list_diagnostics(&self) -> Vec<PluginDiagnosticEvent> {
        self.diagnostics.list()
    }

    pub fn clear_diagnostics(&self) {
        self.diagnostics.clear();
    }

    pub fn reload_packages(&self) -> Vec<PackageDescriptor> {
        info!("Reloading plugin packages from {:?}", self.packages_dir);
        self.close_all_package_surfaces();
        let state = self.load_state();
        let mut records = HashMap::new();

        match fs::read_dir(&self.packages_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    if path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with('.'))
                    {
                        continue;
                    }
                    match self.read_package_record(&path, &state) {
                        Ok(record) => {
                            records.insert(record.descriptor.id.clone(), record);
                        }
                        Err(err) => {
                            warn!("Package {:?} ignored: {}", path, err);
                            self.diagnostics.push(PluginDiagnosticInput {
                                package_id: None,
                                source: path.to_string_lossy().to_string(),
                                severity: PluginDiagnosticSeverity::Error,
                                phase: "discovery".to_string(),
                                message: err,
                                crash_count: None,
                            });
                        }
                    }
                }
            }
            Err(err) => warn!("Unable to read package directory: {}", err),
        }

        apply_graph_diagnostics(&mut records);
        let settings = self.load_app_settings();
        let package_settings = self.load_package_settings();
        let extension_host_specs = extension_host_specs_for_records(
            &records,
            &package_settings,
            &self.package_settings_path,
            &self.plugin_storage_dir,
        );
        let sidecar_specs = sidecar_specs_for_records(&records);
        let sidecar_service_specs = sidecar_service_specs_for_records(&records);
        self.reload_runtimes_with_specs(
            &settings,
            extension_host_specs,
            sidecar_specs,
            sidecar_service_specs,
        );
        *self.records.lock().unwrap() = records;
        let packages = self.list_packages();
        self.emit_packages_changed(&packages);
        packages
    }

    fn close_all_package_surfaces(&self) {
        let surfaces = {
            let records = self.records.lock().unwrap();
            records
                .values()
                .map(|record| {
                    (
                        record.manifest.id().to_string(),
                        record
                            .manifest
                            .contributes_v4()
                            .webviews
                            .iter()
                            .filter(|webview| webview.kind.as_deref() == Some("surface"))
                            .map(|webview| webview.id.clone())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        for (package_id, surface_ids) in surfaces {
            close_package_surfaces(
                &self.app_handle,
                &package_id,
                surface_ids.iter().map(String::as_str),
            );
        }
    }

    fn reload_runtimes_from_current_records(&self) {
        let settings = self.load_app_settings();
        let package_settings = self.load_package_settings();
        let (extension_host_specs, sidecar_specs, sidecar_service_specs) = {
            let records = self.records.lock().unwrap();
            (
                extension_host_specs_for_records(
                    &records,
                    &package_settings,
                    &self.package_settings_path,
                    &self.plugin_storage_dir,
                ),
                sidecar_specs_for_records(&records),
                sidecar_service_specs_for_records(&records),
            )
        };
        self.reload_runtimes_with_specs(
            &settings,
            extension_host_specs,
            sidecar_specs,
            sidecar_service_specs,
        );
    }

    fn reload_runtimes_with_specs(
        &self,
        settings: &AppSettings,
        extension_host_specs: Vec<ExtensionHostRuntimeSpec>,
        sidecar_specs: Vec<SidecarRuntimeSpec>,
        sidecar_service_specs: HashMap<String, SidecarServiceRuntimeSpec>,
    ) {
        if settings.security.plugins_safe_mode {
            self.diagnostics.push(PluginDiagnosticInput {
                package_id: None,
                source: "host".to_string(),
                severity: PluginDiagnosticSeverity::Warning,
                phase: "activation".to_string(),
                message: "Plugin safe mode is enabled; plugin runtimes are stopped.".to_string(),
                crash_count: None,
            });
            let _ = self.extension_host_runtimes.reload_with_app_handle(
                Vec::new(),
                self.app_handle.clone(),
                self.bus.clone(),
                self.registry.clone(),
                self.command_router.clone(),
                self.service_router.clone(),
                self.sidecar_runtimes.controller(),
                self.diagnostics.clone(),
            );
            let _ = self
                .sidecar_runtimes
                .reload_with_app_handle(Vec::new(), self.app_handle.clone());
            self.reconcile_sidecar_services(&HashMap::new());
            return;
        }

        if settings.security.disable_plugin_activation {
            self.diagnostics.push(PluginDiagnosticInput {
                package_id: None,
                source: "host".to_string(),
                severity: PluginDiagnosticSeverity::Warning,
                phase: "activation".to_string(),
                message: "Plugin activation is disabled; extension hosts and sidecars are stopped."
                    .to_string(),
                crash_count: None,
            });
            let _ = self.extension_host_runtimes.reload_with_app_handle(
                Vec::new(),
                self.app_handle.clone(),
                self.bus.clone(),
                self.registry.clone(),
                self.command_router.clone(),
                self.service_router.clone(),
                self.sidecar_runtimes.controller(),
                self.diagnostics.clone(),
            );
            let _ = self
                .sidecar_runtimes
                .reload_with_app_handle(Vec::new(), self.app_handle.clone());
            self.reconcile_sidecar_services(&HashMap::new());
            return;
        }
        if let Err(err) = self
            .sidecar_runtimes
            .reload_with_app_handle(sidecar_specs, self.app_handle.clone())
        {
            warn!("Unable to reload sidecar runtimes: {}", err);
        }
        if let Err(err) = self.extension_host_runtimes.reload_with_app_handle(
            extension_host_specs,
            self.app_handle.clone(),
            self.bus.clone(),
            self.registry.clone(),
            self.command_router.clone(),
            self.service_router.clone(),
            self.sidecar_runtimes.controller(),
            self.diagnostics.clone(),
        ) {
            warn!("Unable to reload extension host runtimes: {}", err);
        }
        self.reconcile_sidecar_services(&sidecar_service_specs);
    }

    fn reconcile_sidecar_services(
        &self,
        sidecar_service_specs: &HashMap<String, SidecarServiceRuntimeSpec>,
    ) {
        let desired: HashSet<String> = sidecar_service_specs.keys().cloned().collect();
        let mut registered = self.sidecar_services.lock().unwrap();
        for service_ref in registered
            .iter()
            .filter(|service_ref| !desired.contains(*service_ref))
        {
            self.service_router.remove(service_ref);
        }
        registered.retain(|service_ref| desired.contains(service_ref));

        for (service_ref, spec) in sidecar_service_specs {
            self.service_router.insert(
                service_ref.clone(),
                ServiceCallClient::new_sidecar(
                    format!("sidecar:{}", spec.sidecar_name),
                    spec.package_id.clone(),
                    spec.sidecar_name.clone(),
                    self.sidecar_runtimes.controller(),
                ),
            );
            registered.insert(service_ref.clone());
        }
    }

    pub fn inspect_package_bundle(&self, path: String) -> Result<BundleInspection, String> {
        inspect_bundle_file(Path::new(&path))
    }

    pub fn install_package_from_file(&self, path: String) -> Result<InstallReceipt, String> {
        let _install_guard = self.package_install_lock.lock().unwrap();
        let inspection = inspect_bundle_file(Path::new(&path))?;
        let should_enable = compatibility_for_manifest(&inspection.manifest).is_compatible();
        let receipt = install_bundle_from_file(
            Path::new(&path),
            &self.packages_dir,
            &self.packages_dir.join(".staging"),
            format!("file:{path}"),
        )?;
        self.save_package_enabled(&receipt.package_id, should_enable)?;
        self.reload_packages();
        Ok(receipt)
    }

    pub async fn prepare_package_from_url(
        &self,
        url: String,
    ) -> Result<PreparedPackageInstall, String> {
        self.prepare_package_from_source_url(url, None, "url").await
    }

    pub async fn prepare_package_from_deep_link(
        &self,
        deep_link: String,
    ) -> Result<PreparedPackageInstall, String> {
        let request = parse_install_deep_link(&deep_link)?;
        self.prepare_package_from_source_url(request.url, request.sha256, "deeplink")
            .await
    }

    pub async fn prepare_package_from_git(
        &self,
        repo: String,
        rev: Option<String>,
    ) -> Result<PreparedPackageInstall, String> {
        let repo = repo.trim();
        if repo.is_empty() {
            return Err("Git repository URL cannot be empty".to_string());
        }
        let rev = rev.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });
        let git_sources_dir = self.git_sources_dir();
        fs::create_dir_all(&git_sources_dir)
            .map_err(|e| format!("Unable to create Git source directory: {e}"))?;
        let checkout_dir = git_sources_dir.join(unique_id("git"));
        let clone = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(repo)
            .arg(&checkout_dir)
            .output()
            .map_err(|e| format!("Unable to run git clone: {e}"))?;
        if !clone.status.success() {
            let _ = fs::remove_dir_all(&checkout_dir);
            return Err(format_command_error("git clone", &clone.stderr));
        }
        if let Some(rev) = &rev {
            let checkout = Command::new("git")
                .arg("-C")
                .arg(&checkout_dir)
                .arg("checkout")
                .arg(rev)
                .output()
                .map_err(|e| format!("Unable to run git checkout: {e}"))?;
            if !checkout.status.success() {
                let _ = fs::remove_dir_all(&checkout_dir);
                return Err(format_command_error("git checkout", &checkout.stderr));
            }
        }

        let source_bundle = match find_first_bundle(&checkout_dir) {
            Ok(source_bundle) => source_bundle,
            Err(err) => {
                let _ = fs::remove_dir_all(&checkout_dir);
                return Err(err);
            }
        };
        let download_path = self
            .downloads_dir()
            .join(format!("prepared-{}.brlp", unique_id("git")));
        if let Some(parent) = download_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Unable to create package download directory: {e}"))?;
        }
        if let Err(err) = fs::copy(&source_bundle, &download_path) {
            let _ = fs::remove_dir_all(&checkout_dir);
            return Err(format!("Unable to stage Git bundle: {err}"));
        }
        let _ = fs::remove_dir_all(&checkout_dir);
        let inspection = match inspect_bundle_file(&download_path) {
            Ok(inspection) => inspection,
            Err(err) => {
                let _ = fs::remove_file(&download_path);
                return Err(err);
            }
        };
        let source = match rev {
            Some(rev) => format!("git:{repo}#{rev}"),
            None => format!("git:{repo}"),
        };
        Ok(PreparedPackageInstall {
            path: download_path.to_string_lossy().to_string(),
            source,
            inspection,
        })
    }

    async fn prepare_package_from_source_url(
        &self,
        url: String,
        expected_sha256: Option<String>,
        source_kind: &str,
    ) -> Result<PreparedPackageInstall, String> {
        let download_path = self
            .downloads_dir()
            .join(format!("prepared-{}.brlp", unique_id("bundle")));
        download_bundle_to_file(&url, &download_path).await?;
        let inspection = match inspect_bundle_file(&download_path) {
            Ok(inspection) => inspection,
            Err(err) => {
                let _ = fs::remove_file(&download_path);
                return Err(err);
            }
        };
        if let Some(expected_sha256) = expected_sha256 {
            if !inspection.sha256.eq_ignore_ascii_case(&expected_sha256) {
                let _ = fs::remove_file(&download_path);
                return Err("Downloaded bundle SHA-256 does not match deep link".to_string());
            }
        }
        Ok(PreparedPackageInstall {
            path: download_path.to_string_lossy().to_string(),
            source: format!("{source_kind}:{url}"),
            inspection,
        })
    }

    pub fn install_prepared_package(
        &self,
        path: String,
        source: String,
    ) -> Result<InstallReceipt, String> {
        let _install_guard = self.package_install_lock.lock().unwrap();
        let bundle_path = PathBuf::from(&path);
        self.validate_install_trust(&bundle_path, &source)?;
        let inspection = inspect_bundle_file(&bundle_path)?;
        let should_enable = compatibility_for_manifest(&inspection.manifest).is_compatible();
        let receipt = install_bundle_from_file(
            &bundle_path,
            &self.packages_dir,
            &self.packages_dir.join(".staging"),
            source,
        )?;
        self.save_package_enabled(&receipt.package_id, should_enable)?;
        let _ = self.discard_prepared_download(&bundle_path);
        self.reload_packages();
        Ok(receipt)
    }

    pub fn discard_prepared_package(&self, path: String) -> Result<(), String> {
        self.discard_prepared_download(Path::new(&path))
    }

    pub async fn install_package_from_url(&self, url: String) -> Result<InstallReceipt, String> {
        let prepared = self.prepare_package_from_url(url).await?;
        self.install_prepared_package(prepared.path, prepared.source)
    }

    fn discard_prepared_download(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        let downloads_dir = self.downloads_dir();
        fs::create_dir_all(&downloads_dir)
            .map_err(|e| format!("Unable to create package download directory: {e}"))?;
        let canonical_path = fs::canonicalize(path)
            .map_err(|e| format!("Unable to resolve prepared bundle: {e}"))?;
        let canonical_downloads = fs::canonicalize(&downloads_dir)
            .map_err(|e| format!("Unable to resolve package download directory: {e}"))?;
        if canonical_path.starts_with(&canonical_downloads) {
            fs::remove_file(&canonical_path)
                .map_err(|e| format!("Unable to discard prepared bundle: {e}"))?;
        }
        Ok(())
    }

    pub fn set_package_enabled(
        &self,
        package_id: String,
        enabled: bool,
    ) -> Result<Vec<PackageDescriptor>, String> {
        if self.is_package_deleting(&package_id) {
            return Err(format!("Package '{package_id}' is being removed."));
        }
        let settings = self.load_app_settings();
        if enabled && settings.security.disable_plugin_activation {
            self.diagnostics.push(PluginDiagnosticInput {
                package_id: Some(package_id.clone()),
                source: "host".to_string(),
                severity: PluginDiagnosticSeverity::Warning,
                phase: "activation".to_string(),
                message: "Plugin activation is disabled by host settings.".to_string(),
                crash_count: None,
            });
            return Err("Plugin activation is disabled by host settings.".to_string());
        }

        {
            let records = self.records.lock().unwrap();
            let record = records
                .get(&package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            if enabled && !record.descriptor.compatibility.is_compatible() {
                return Err(record
                    .descriptor
                    .compatibility
                    .message
                    .clone()
                    .unwrap_or_else(|| format!("Package '{package_id}' is incompatible.")));
            }
        }

        let mut state = self.load_state();
        state.enabled.insert(package_id.clone(), enabled);
        self.save_state(&state)?;
        Ok(self.reload_packages())
    }

    fn begin_remove_package(&self, package_id: &str) -> Result<PackageRemovalStart, String> {
        let target = safe_installed_package_dir(&self.packages_dir, package_id)?;
        if !target.exists() {
            return Err(format!("Package '{package_id}' is not installed."));
        }

        let previous_enabled = {
            let records = self.records.lock().unwrap();
            let record = records
                .get(package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            record.descriptor.enabled
        };

        let mut deleting_packages = self.deleting_packages.lock().unwrap();
        let started = if deleting_packages.contains_key(package_id) {
            false
        } else {
            deleting_packages.insert(
                package_id.to_string(),
                PendingPackageDeletion { previous_enabled },
            );
            true
        };
        drop(deleting_packages);

        let packages = self.list_packages();
        self.emit_packages_changed(&packages);
        Ok(PackageRemovalStart { packages, started })
    }

    fn remove_package_in_background(&self, package_id: String) {
        let result = self.remove_package_files(&package_id);
        match result {
            Ok(()) => self.finish_package_removal(&package_id),
            Err(error) => self.fail_package_removal(&package_id, &error),
        }
    }

    fn remove_package_files(&self, package_id: &str) -> Result<(), String> {
        let mut state = self.load_state();
        state.enabled.insert(package_id.to_string(), false);
        self.save_state(&state)?;
        self.reload_packages();

        let target = safe_installed_package_dir(&self.packages_dir, package_id)?;
        if target.exists() {
            fs::remove_dir_all(&target).map_err(|e| format!("Unable to remove package: {e}"))?;
        }
        Ok(())
    }

    fn finish_package_removal(&self, package_id: &str) {
        let mut state = self.load_state();
        state.enabled.remove(package_id);
        if let Err(error) = self.save_state(&state) {
            self.emit_package_operation_error(package_id, &error);
        }
        self.deleting_packages.lock().unwrap().remove(package_id);
        self.reload_packages();
    }

    fn fail_package_removal(&self, package_id: &str, error: &str) {
        let previous_enabled = self
            .deleting_packages
            .lock()
            .unwrap()
            .remove(package_id)
            .map(|pending| pending.previous_enabled);
        if let Some(previous_enabled) = previous_enabled {
            let mut state = self.load_state();
            state
                .enabled
                .insert(package_id.to_string(), previous_enabled);
            if let Err(save_error) = self.save_state(&state) {
                self.emit_package_operation_error(package_id, &save_error);
            }
        }
        self.reload_packages();
        self.emit_package_operation_error(package_id, error);
    }

    pub fn read_package_file(
        &self,
        package_id: &str,
        relative_path: &str,
    ) -> Result<Vec<u8>, String> {
        let records = self.records.lock().unwrap();
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        if self.is_package_deleting(package_id) {
            return Err(format!("Package '{package_id}' is being removed."));
        }
        if !record.descriptor.enabled {
            return Err(format!("Package '{package_id}' is disabled."));
        }
        let safe_path = safe_package_relative_path(relative_path)?;
        read_binary_package_file(Path::new(&record.descriptor.path), &safe_path)
    }

    pub fn read_package_file_text(
        &self,
        package_id: &str,
        relative_path: &str,
    ) -> Result<String, String> {
        let bytes = self.read_package_file(package_id, relative_path)?;
        String::from_utf8(bytes)
            .map_err(|e| format!("Package file '{relative_path}' is not valid UTF-8: {e}"))
    }

    pub fn read_package_webview_asset(
        &self,
        package_id: &str,
        relative_path: &str,
    ) -> Result<PackageWebviewAssetPayload, String> {
        let safe_path = safe_package_relative_path(relative_path)?;
        let safe_path = safe_path.to_string_lossy().to_string();
        let bytes = self.read_package_file(package_id, &safe_path)?;
        Ok(PackageWebviewAssetPayload {
            contents_base64: BASE64_STANDARD.encode(bytes),
            content_type: content_type_for_path(&safe_path).to_string(),
            path: safe_path,
        })
    }

    pub fn read_package_webview_module_text(
        &self,
        package_id: &str,
        webview_id: &str,
        relative_path: &str,
    ) -> Result<String, String> {
        let (package_root, safe_path) = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, package_id)?;
            let webview = record
                .manifest
                .contributes_v4()
                .webviews
                .iter()
                .find(|webview| webview.id == webview_id)
                .ok_or_else(|| {
                    format!("Package '{package_id}' does not declare webview '{webview_id}'.")
                })?;
            (
                PathBuf::from(&record.descriptor.path),
                package_webview_module_relative_path(&webview.entry, relative_path)?,
            )
        };
        let bytes = read_binary_package_file(&package_root, &safe_path)?;
        String::from_utf8(bytes).map_err(|e| {
            format!("Package webview module '{relative_path}' is not valid UTF-8: {e}")
        })
    }

    pub fn package_webview_runtime_descriptor(
        &self,
        package_id: &str,
        webview_id: &str,
    ) -> Result<PackageWebviewRuntimeDescriptor, String> {
        let records = self.records.lock().unwrap();
        let record = self.require_enabled_record(&records, package_id)?;
        package_webview_runtime_descriptor_for_record(package_id, webview_id, record)
    }

    pub fn list_runtime_packages(
        &self,
        caller_package_id: &str,
    ) -> Result<serde_json::Value, String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, caller_package_id)?;
        Ok(serde_json::Value::Array(runtime_packages_for_records(
            &records,
            caller_package_id,
        )))
    }

    pub fn list_extension_points(
        &self,
        caller_package_id: &str,
        package_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, caller_package_id)?;
        let points = records
            .values()
            .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
            .filter(|record| package_id.is_none_or(|package_id| record.manifest.id() == package_id))
            .flat_map(|record| {
                record
                    .descriptor
                    .contributions
                    .extension_points
                    .iter()
                    .map(|point| {
                        serde_json::json!({
                            "packageId": record.manifest.id(),
                            "id": point.name,
                            "reference": point.reference,
                            "version": point.version,
                            "title": point.title,
                            "description": point.description,
                            "schema": point.schema,
                            "service": point.service,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Ok(serde_json::Value::Array(points))
    }

    pub fn list_extension_contributions(
        &self,
        caller_package_id: &str,
        target: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, caller_package_id)?;
        let contributions = listed_extension_contributions(&records, target);
        Ok(serde_json::Value::Array(contributions))
    }

    pub fn list_package_resources(
        &self,
        caller_package_id: &str,
        package_id: Option<&str>,
        resource_type: Option<&str>,
        visibility: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        validate_resource_visibility_filter(visibility)?;
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, caller_package_id)?;
        let resources = listed_package_resources(
            &records,
            caller_package_id,
            package_id,
            resource_type,
            visibility,
        );
        Ok(serde_json::Value::Array(resources))
    }

    pub fn read_package_resource(
        &self,
        caller_package_id: &str,
        resource_ref: &str,
        requested_path: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let (provider_package_id, resource_id) = parse_export_ref(resource_ref)?;
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, caller_package_id)?;
        let provider = self.require_enabled_record(&records, provider_package_id)?;
        let resource = provider
            .descriptor
            .contributions
            .resources
            .iter()
            .find(|resource| resource.name == resource_id)
            .ok_or_else(|| format!("Resource '{resource_ref}' does not exist."))?;
        if !resource.public && caller_package_id != provider_package_id {
            let message = format!("Resource '{resource_ref}' is private.");
            self.push_host_diagnostic(
                Some(caller_package_id),
                "resources",
                PluginDiagnosticSeverity::Warning,
                message.clone(),
            );
            return Err(message);
        }
        let relative_path = select_resource_path(resource_ref, &resource.paths, requested_path)?;
        let bytes = read_binary_package_file(
            Path::new(&provider.descriptor.path),
            Path::new(&relative_path),
        )?;
        Ok(serde_json::json!({
            "contentsBase64": BASE64_STANDARD.encode(bytes),
            "contentType": resource.resource_type.clone().unwrap_or_else(|| content_type_for_path(&relative_path).to_string()),
            "path": relative_path,
            "resource": {
                "packageId": provider_package_id,
                "id": resource.name,
                "reference": resource.reference,
                "type": resource.resource_type,
                "visibility": resource.visibility,
                "public": resource.public,
                "metadata": resource.metadata,
            }
        }))
    }

    pub fn validate_service_call(
        &self,
        caller_package_id: &str,
        service_ref: &str,
        method: &str,
    ) -> Result<(), String> {
        let (provider_package_id, export_name) = parse_export_ref(service_ref)?;
        let records = self.records.lock().unwrap();
        let caller = self.require_enabled_record(&records, caller_package_id)?;
        let provider = self.require_enabled_record(&records, provider_package_id)?;
        if provider_package_id != caller_package_id
            && !caller_depends_on(provider_package_id, caller)
        {
            let message = format!(
                "Package '{caller_package_id}' cannot call service '{service_ref}' without declaring a dependency on '{provider_package_id}'."
            );
            self.push_host_diagnostic(
                Some(caller_package_id),
                "services",
                PluginDiagnosticSeverity::Warning,
                message.clone(),
            );
            return Err(message);
        }
        let export = provider
            .manifest
            .contributes_v4()
            .services
            .iter()
            .find(|service| service.id == export_name)
            .ok_or_else(|| format!("Service export '{service_ref}' does not exist."))?;
        if !export.methods.iter().any(|allowed| allowed == method) {
            return Err(format!(
                "Service export '{service_ref}' does not expose method '{method}'."
            ));
        }
        Ok(())
    }

    pub fn validate_command_call(
        &self,
        caller_package_id: &str,
        command_ref: &str,
    ) -> Result<(), String> {
        let (provider_package_id, export_name) = parse_export_ref(command_ref)?;
        let records = self.records.lock().unwrap();
        let caller = self.require_enabled_record(&records, caller_package_id)?;
        let provider = self.require_enabled_record(&records, provider_package_id)?;
        let result = validate_command_export(
            caller,
            provider,
            caller_package_id,
            provider_package_id,
            command_ref,
            export_name,
        );
        if let Err(message) = &result {
            if message.contains("without declaring a dependency") {
                self.push_host_diagnostic(
                    Some(caller_package_id),
                    "commands",
                    PluginDiagnosticSeverity::Warning,
                    message.clone(),
                );
            }
        }
        result
    }

    pub async fn call_service_export(
        &self,
        caller_package_id: &str,
        service_ref: &str,
        method: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.validate_service_call(caller_package_id, service_ref, method)?;
        self.service_router
            .call(service_ref, method.to_string(), input)
            .await
    }

    pub async fn call_command_export(
        &self,
        caller_package_id: &str,
        command_ref: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        self.validate_command_call(caller_package_id, command_ref)?;
        self.command_router.call(command_ref, args).await
    }

    pub fn can_package_read_registry(&self, package_id: &str, key: &str) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        let caller = self.require_enabled_record(&records, package_id)?;
        if key.trim().is_empty() || key_is_in_plugin_namespace(package_id, key) {
            return Ok(());
        }
        if let Some(owner_package_id) = registry_key_owner(&records, key) {
            if owner_package_id != package_id && caller_depends_on(owner_package_id, caller) {
                return Ok(());
            }
            return Err(format!(
                "Package '{package_id}' cannot read registry key '{key}' without declaring a dependency on '{owner_package_id}'."
            ));
        }
        if dependency_key_owner(caller, key).is_some() {
            return Ok(());
        }
        Err(format!(
            "Package '{package_id}' cannot read registry key '{key}' outside a declared plugin namespace."
        ))
    }

    pub fn can_package_write_registry(&self, package_id: &str, key: &str) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, package_id)?;
        ensure_plugin_namespace(package_id, key, "registry key")
    }

    pub fn readable_registry_entries(
        &self,
        package_id: &str,
        registry: &Registry,
    ) -> Result<Vec<crate::registry::RegistryEntry>, String> {
        let records = self.records.lock().unwrap();
        let caller = self.require_enabled_record(&records, package_id)?;
        Ok(registry
            .entries()
            .into_iter()
            .filter(|entry| {
                if key_is_in_plugin_namespace(package_id, &entry.key) {
                    return true;
                }
                registry_key_owner(&records, &entry.key)
                    .is_some_and(|owner| owner != package_id && caller_depends_on(owner, caller))
            })
            .collect())
    }

    pub fn can_package_write_event(
        &self,
        package_id: &str,
        event_name: &str,
    ) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, package_id)?;
        ensure_plugin_namespace(package_id, event_name, "event name")
    }

    pub fn is_settings_webview(&self, package_id: &str, webview_id: &str) -> Result<bool, String> {
        let records = self.records.lock().unwrap();
        let record = self.require_enabled_record(&records, package_id)?;
        Ok(record
            .manifest
            .contributes_v4()
            .webviews
            .iter()
            .any(|webview| webview.id == webview_id && is_settings_webview(webview)))
    }

    pub fn push_package_webview_diagnostic(
        &self,
        package_id: &str,
        webview_id: &str,
        severity: PluginDiagnosticSeverity,
        phase: Option<String>,
        message: String,
    ) -> Result<PluginDiagnosticEvent, String> {
        let input = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, package_id)?;
            package_webview_diagnostic_input_for_record(
                package_id, webview_id, record, severity, phase, message,
            )?
        };
        let event = self.diagnostics.push(input.clone());
        let payload = serde_json::json!({
            "kind": "webview",
            "source": input.source,
            "stream": diagnostic_log_stream(&input.severity),
            "line": input.message,
        });
        let _ = self.app_handle.emit("bakingrl-runtime-log", payload);
        Ok(event)
    }

    fn can_window_configure_package(
        &self,
        window_label: &str,
        package_id: &str,
        include_secrets_page: bool,
    ) -> Result<(), String> {
        if window_label == "main"
            || window_label == page_window_label(&format!("configuration-{package_id}"))
            || (include_secrets_page
                && window_label == page_window_label(&format!("secrets-{package_id}")))
        {
            return Ok(());
        }
        let records = self.records.lock().unwrap();
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        if settings_webview_label_can_access_package_record(package_id, record, window_label) {
            Ok(())
        } else {
            Err(format!(
                "Window '{window_label}' cannot configure package '{package_id}'."
            ))
        }
    }

    pub fn can_window_save_package_settings(
        &self,
        window_label: &str,
        package_id: &str,
    ) -> Result<(), String> {
        self.can_window_configure_package(window_label, package_id, false)
    }

    pub fn can_window_manage_package_secrets(
        &self,
        window_label: &str,
        package_id: &str,
    ) -> Result<(), String> {
        self.can_window_configure_package(window_label, package_id, true)
    }

    fn require_enabled_record<'a>(
        &self,
        records: &'a HashMap<String, PackageRecord>,
        package_id: &str,
    ) -> Result<&'a PackageRecord, String> {
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        if self.is_package_deleting(package_id) {
            return Err(format!("Package '{package_id}' is being removed."));
        }
        if !record.descriptor.enabled {
            return Err(format!("Package '{package_id}' is disabled."));
        }
        if !record.descriptor.compatibility.is_compatible() {
            return Err(record
                .descriptor
                .compatibility
                .message
                .clone()
                .unwrap_or_else(|| format!("Package '{package_id}' is incompatible.")));
        }
        if let Some(error) = &record.descriptor.error {
            return Err(format!(
                "Package '{package_id}' has unresolved diagnostics: {error}"
            ));
        }
        Ok(record)
    }

    pub fn get_app_settings(&self) -> AppSettings {
        self.load_app_settings()
    }

    pub fn save_app_settings(&self, settings: AppSettings) -> Result<AppSettings, String> {
        let previous_settings = self.load_app_settings();
        let raw = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize app settings: {e}"))?;
        if previous_settings.behavior.launch_at_startup != settings.behavior.launch_at_startup {
            self.apply_launch_at_startup_setting(settings.behavior.launch_at_startup)?;
        }
        fs::write(&self.app_settings_path, raw)
            .map_err(|e| format!("Failed to write app settings: {e}"))?;
        if previous_settings.security.plugins_safe_mode != settings.security.plugins_safe_mode
            || previous_settings.security.disable_plugin_activation
                != settings.security.disable_plugin_activation
        {
            self.reload_runtimes_from_current_records();
        }
        Ok(settings)
    }

    fn apply_launch_at_startup_setting(&self, enabled: bool) -> Result<(), String> {
        #[cfg(desktop)]
        {
            use tauri_plugin_autostart::ManagerExt;

            let autostart = self.app_handle.autolaunch();
            if enabled {
                autostart
                    .enable()
                    .map_err(|error| format!("Failed to enable launch at startup: {error}"))?;
            } else {
                autostart
                    .disable()
                    .map_err(|error| format!("Failed to disable launch at startup: {error}"))?;
            }
        }
        Ok(())
    }

    pub fn get_package_settings(&self, package_id: &str) -> Result<serde_json::Value, String> {
        let records = self.records.lock().unwrap();
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        let schema = read_package_settings_schema(record)?;
        Ok(merge_package_settings_with_schema(
            schema.as_ref(),
            self.load_package_settings()
                .values
                .get(package_id)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        ))
    }

    pub fn save_package_settings(
        &self,
        package_id: String,
        values: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let sanitized = {
            let records = self.records.lock().unwrap();
            let record = records
                .get(&package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            let schema = read_package_settings_schema(record)?;
            sanitize_package_settings_values(schema.as_ref(), values)?
        };
        let mut settings = self.load_package_settings();
        settings.values.insert(package_id.clone(), sanitized);
        self.save_package_settings_file(&settings)?;
        self.emit_package_settings_changed(&package_id);
        self.reload_runtimes_from_current_records();
        self.get_package_settings(&package_id)
    }

    pub fn get_package_configuration_state(
        &self,
        package_id: String,
    ) -> Result<PackageConfigurationState, String> {
        let records = self.records.lock().unwrap();
        let record = records
            .get(&package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        let schema = read_package_settings_schema(record)?;
        let package_settings = self.load_package_settings();
        let values = merge_package_settings_with_schema(
            schema.as_ref(),
            package_settings
                .values
                .get(&package_id)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        );
        let secret_store = secret_store_status();
        let secret_store_error = secret_store.as_ref().err().cloned();
        let secrets = secret_definitions(schema.as_ref())
            .into_iter()
            .map(|definition| PackageSecretDescriptor {
                configured: package_secret_configured(
                    &package_settings,
                    &package_id,
                    &definition.key,
                ),
                key: definition.key,
                label: definition.label,
                description: definition.description,
                required: definition.required,
            })
            .collect::<Vec<_>>();
        let secret_store_available = secret_store_error.is_none();
        let title = schema
            .as_ref()
            .and_then(|schema| schema.get("title"))
            .and_then(|title| title.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{} Settings", record.descriptor.name));
        Ok(PackageConfigurationState {
            package_id,
            title,
            has_settings_webview: preferred_settings_webview_id(record).is_some(),
            schema,
            values,
            secrets,
            secret_store_available,
            secret_store_error,
        })
    }

    pub fn set_package_secret(
        &self,
        package_id: String,
        key: String,
        value: String,
    ) -> Result<PackageConfigurationState, String> {
        self.ensure_declared_package_secret(&package_id, &key)?;
        let configured = !value.is_empty();
        if value.is_empty() {
            delete_keychain_package_secret(&package_id, &key)?;
        } else {
            write_package_secret(&package_id, &key, &value)?;
        }
        let mut settings = self.load_package_settings();
        set_package_secret_configured(&mut settings, &package_id, &key, configured);
        self.save_package_settings_file(&settings)?;
        self.emit_package_settings_changed(&package_id);
        self.get_package_configuration_state(package_id)
    }

    pub fn delete_package_secret(
        &self,
        package_id: String,
        key: String,
    ) -> Result<PackageConfigurationState, String> {
        self.ensure_declared_package_secret(&package_id, &key)?;
        delete_keychain_package_secret(&package_id, &key)?;
        let mut settings = self.load_package_settings();
        set_package_secret_configured(&mut settings, &package_id, &key, false);
        self.save_package_settings_file(&settings)?;
        self.emit_package_settings_changed(&package_id);
        self.get_package_configuration_state(package_id)
    }

    fn ensure_declared_package_secret(&self, package_id: &str, key: &str) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        let schema = read_package_settings_schema(record)?;
        if secret_definitions(schema.as_ref())
            .iter()
            .any(|definition| definition.key == key)
        {
            Ok(())
        } else {
            Err(format!(
                "Package '{package_id}' did not declare secret setting '{key}'."
            ))
        }
    }

    pub fn open_package_webview(
        &self,
        package_id: String,
        webview_id: String,
    ) -> Result<(), String> {
        let (label, path, title, width, height, surface) = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, &package_id)?;
            let webview = record
                .manifest
                .contributes_v4()
                .webviews
                .iter()
                .find(|webview| webview.id == webview_id)
                .ok_or_else(|| {
                    format!("Package '{package_id}' does not declare webview '{webview_id}'.")
                })?;
            let [width, height] = webview.default_size.unwrap_or([960.0, 640.0]);
            (
                package_webview_window_label(&package_id, &webview.id),
                package_webview_route(&package_id, &webview.id),
                webview
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("{} - {}", record.descriptor.name, webview.id)),
                width,
                height,
                webview.surface.clone(),
            )
        };

        if let Some(surface) = surface {
            let result = open_surface(
                &self.app_handle,
                SurfaceOpenRequest {
                    package_id: &package_id,
                    surface_id: &webview_id,
                    route: &path,
                    title: &title,
                    default_size: [width, height],
                    declaration: &surface,
                    options: SurfaceOpenOptions::default(),
                },
            )?;
            if let Some(message) = result.diagnostic {
                self.push_host_diagnostic(
                    Some(&package_id),
                    "surface",
                    PluginDiagnosticSeverity::Warning,
                    message,
                );
            }
            return Ok(());
        }

        self.open_standalone_page_window(label, path, title, width, height)
    }

    pub fn close_package_webview(
        &self,
        package_id: &str,
        webview_id: &str,
    ) -> Result<bool, String> {
        let is_surface = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, package_id)?;
            let webview = record
                .manifest
                .contributes_v4()
                .webviews
                .iter()
                .find(|webview| webview.id == webview_id)
                .ok_or_else(|| {
                    format!("Package '{package_id}' does not declare webview '{webview_id}'.")
                })?;
            webview.surface.is_some()
        };

        if is_surface {
            return surface_runtime::close_surface(&self.app_handle, package_id, webview_id);
        }
        let label = package_webview_window_label(package_id, webview_id);
        let Some(window) = self.app_handle.get_webview_window(&label) else {
            return Ok(false);
        };
        window.close().map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn open_package_configuration(&self, package_id: String) -> Result<(), String> {
        let settings_webview = {
            let records = self.records.lock().unwrap();
            match self.require_enabled_record(&records, &package_id) {
                Ok(record) => preferred_settings_webview_id(record),
                Err(_) => None,
            }
        };
        if let Some(webview_id) = settings_webview {
            return self.open_package_webview(package_id, webview_id);
        }

        let title = {
            let records = self.records.lock().unwrap();
            let record = records
                .get(&package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            if !record.descriptor.has_public_settings {
                return Err(format!(
                    "Package '{package_id}' does not expose supported settings UI in manifest V4."
                ));
            }
            let settings_schema_title =
                read_package_settings_schema(record)
                    .ok()
                    .and_then(|schema| {
                        schema.and_then(|schema| {
                            schema
                                .get("title")
                                .and_then(|title| title.as_str().map(ToOwned::to_owned))
                        })
                    });
            settings_schema_title.unwrap_or_else(|| format!("{} Settings", record.descriptor.name))
        };
        let page_id = format!("configuration-{package_id}");
        self.open_standalone_page_window(
            page_window_label(&page_id),
            format!("/page/{page_id}"),
            format!("BakingRL - {title}"),
            1200.0,
            740.0,
        )
    }

    pub fn open_package_secrets(&self, package_id: String) -> Result<(), String> {
        let title = {
            let records = self.records.lock().unwrap();
            let record = records
                .get(&package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            if !record.descriptor.has_secrets {
                return Err(format!("Package '{package_id}' does not declare secrets."));
            }
            format!("{} Secrets", record.descriptor.name)
        };
        let page_id = format!("secrets-{package_id}");
        self.open_standalone_page_window(
            page_window_label(&page_id),
            format!("/page/{page_id}"),
            format!("BakingRL - {title}"),
            760.0,
            620.0,
        )
    }

    fn open_standalone_page_window(
        &self,
        label: String,
        path: String,
        title: String,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        let js_path = serde_json::to_string(&path).map_err(|error| error.to_string())?;
        if let Some(window) = self.app_handle.get_webview_window(&label) {
            window
                .eval(format!("window.location.href = {js_path};"))
                .map_err(|error| error.to_string())?;
            let _ = window.show();
            let _ = window.set_focus();
            return Ok(());
        }

        let window = WebviewWindowBuilder::new(
            &self.app_handle,
            label,
            WebviewUrl::App(PathBuf::from(path)),
        )
        .title(title)
        .inner_size(width.max(480.0), (height + 48.0).max(368.0))
        .min_inner_size(480.0, 320.0)
        .decorations(false)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|error| error.to_string())?;
        let _ = window.set_focus();
        Ok(())
    }

    fn read_package_record(
        &self,
        package_dir: &Path,
        state: &PackageStateFile,
    ) -> Result<PackageRecord, String> {
        let manifest_path = package_dir.join("bakingrl.plugin.json");
        let manifest_str = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("bakingrl.plugin.json unreadable: {e}"))?;
        let manifest = PluginPackageManifest::parse(&manifest_str)
            .map_err(|e| format!("bakingrl.plugin.json invalid: {e}"))?;
        let enabled = state.enabled.get(manifest.id()).copied().unwrap_or(false);
        let descriptor = descriptor_for_manifest(
            &manifest,
            package_dir.to_string_lossy().to_string(),
            enabled,
        );
        Ok(PackageRecord {
            descriptor,
            manifest,
        })
    }

    fn load_state(&self) -> PackageStateFile {
        read_json_or_default(&self.state_path)
    }

    fn save_state(&self, state: &PackageStateFile) -> Result<(), String> {
        write_json(&self.state_path, state)
    }

    fn save_package_enabled(&self, package_id: &str, enabled: bool) -> Result<(), String> {
        let mut state = self.load_state();
        state.enabled.insert(package_id.to_string(), enabled);
        self.save_state(&state)
    }

    fn load_app_settings(&self) -> AppSettings {
        read_json_or_default(&self.app_settings_path)
    }

    fn load_package_settings(&self) -> PackageSettingsFile {
        read_json_or_default(&self.package_settings_path)
    }

    fn save_package_settings_file(&self, file: &PackageSettingsFile) -> Result<(), String> {
        write_json(&self.package_settings_path, file)
    }

    fn validate_install_trust(&self, bundle_path: &Path, source: &str) -> Result<(), String> {
        let settings = self.load_app_settings();
        if !settings.security.require_trusted_remote_packages || !is_remote_package_source(source) {
            return Ok(());
        }
        let inspection = inspect_bundle_file(bundle_path)?;
        if !inspection.signature_verified {
            return Err("Remote package installs require a verified Ed25519 signature. Disable signed remote package enforcement only for packages you explicitly trust.".to_string());
        }
        let trusted_keys = settings
            .security
            .trusted_package_public_keys
            .iter()
            .map(|key| key.trim())
            .filter(|key| !key.is_empty())
            .collect::<Vec<_>>();
        if trusted_keys.is_empty() {
            return Ok(());
        }
        let public_key = inspection
            .signature_public_key
            .as_deref()
            .ok_or_else(|| "Remote package signature did not expose a public key.".to_string())?;
        if trusted_keys.iter().any(|key| key == &public_key) {
            Ok(())
        } else {
            Err(
                "Remote package signature public key is not trusted by this installation."
                    .to_string(),
            )
        }
    }

    fn emit_packages_changed(&self, packages: &[PackageDescriptor]) {
        let _ = self.app_handle.emit("bakingrl-packages-changed", packages);
    }

    fn apply_package_statuses(&self, packages: &mut [PackageDescriptor]) {
        let deleting_packages = self.deleting_packages.lock().unwrap();
        for package in packages {
            if deleting_packages.contains_key(&package.id) {
                package.enabled = false;
                package.status = PackageStatus::Deleting;
            }
        }
    }

    fn is_package_deleting(&self, package_id: &str) -> bool {
        self.deleting_packages
            .lock()
            .unwrap()
            .contains_key(package_id)
    }

    fn emit_package_operation_error(&self, package_id: &str, message: &str) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        self.diagnostics.push(PluginDiagnosticInput {
            package_id: Some(package_id.to_string()),
            source: "host".to_string(),
            severity: PluginDiagnosticSeverity::Error,
            phase: "operation".to_string(),
            message: message.to_string(),
            crash_count: None,
        });
        let payload = serde_json::json!({
            "kind": "package",
            "source": package_id,
            "message": message,
            "timestamp_ms": timestamp_ms
        });
        let _ = self.app_handle.emit("bakingrl-runtime-error", payload);
    }

    fn push_host_diagnostic(
        &self,
        package_id: Option<&str>,
        phase: &str,
        severity: PluginDiagnosticSeverity,
        message: String,
    ) {
        self.diagnostics.push(PluginDiagnosticInput {
            package_id: package_id.map(ToOwned::to_owned),
            source: "host".to_string(),
            severity,
            phase: phase.to_string(),
            message,
            crash_count: None,
        });
    }

    fn emit_package_settings_changed(&self, package_id: &str) {
        let _ = self
            .app_handle
            .emit("bakingrl-package-settings-changed", package_id);
    }
}

pub fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        runtime_api_version: HOST_RUNTIME_API_VERSION.to_string(),
        supported_runtime_api: supported_runtime_api_label(),
    }
}

fn validate_command_export(
    caller: &PackageRecord,
    provider: &PackageRecord,
    caller_package_id: &str,
    provider_package_id: &str,
    command_ref: &str,
    export_name: &str,
) -> Result<(), String> {
    if provider_package_id != caller_package_id && !caller_depends_on(provider_package_id, caller) {
        return Err(format!(
            "Package '{caller_package_id}' cannot execute command '{command_ref}' without declaring a dependency on '{provider_package_id}'."
        ));
    }
    if !provider
        .manifest
        .contributes_v4()
        .commands
        .iter()
        .any(|command| command.id == export_name)
    {
        return Err(format!("Command export '{command_ref}' does not exist."));
    }
    Ok(())
}

fn caller_depends_on(provider_package_id: &str, caller: &PackageRecord) -> bool {
    caller
        .manifest
        .dependencies_v4()
        .iter()
        .any(|dependency| dependency.package_id == provider_package_id)
}

fn runtime_packages_for_records(
    records: &HashMap<String, PackageRecord>,
    caller_package_id: &str,
) -> Vec<serde_json::Value> {
    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .map(|record| runtime_package_for_record(record, caller_package_id))
        .collect()
}

fn runtime_package_for_record(
    record: &PackageRecord,
    caller_package_id: &str,
) -> serde_json::Value {
    let contributes = record.manifest.contributes_v4();
    let package_id = record.manifest.id();
    let resources = contributes
        .resources
        .iter()
        .filter(|resource| {
            package_id == caller_package_id
                || resource.visibility
                    == crate::plugin_package::manifest::PluginResourceVisibilityV4::Public
        })
        .map(|resource| {
            let visibility = match resource.visibility {
                crate::plugin_package::manifest::PluginResourceVisibilityV4::Public => "public",
                crate::plugin_package::manifest::PluginResourceVisibilityV4::Private => "private",
            };
            serde_json::json!({
                "id": resource.id,
                "path": resource.path,
                "paths": resource.paths,
                "type": resource.resource_type,
                "visibility": visibility,
                "public": visibility == "public",
                "metadata": resource.metadata,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "id": record.manifest.id(),
        "name": record.manifest.name(),
        "version": record.manifest.version(),
        "author": record.manifest.author(),
        "bakingrlApi": record.manifest.compatibility().and_then(|compatibility| compatibility.runtime_api.as_deref()),
        "enabled": record.descriptor.enabled,
        "active": record.descriptor.enabled && record.descriptor.error.is_none(),
        "dependencies": record.manifest.dependencies_v4(),
        "runtime": record.manifest.runtime_v4(),
        "contributes": {
            "settings": contributes.settings,
            "services": contributes.services,
            "commands": contributes.commands,
            "extensionPoints": contributes.extension_points,
            "contributions": contributes.contributions,
            "resources": resources,
            "webviews": contributes.webviews,
        },
    })
}

fn validate_resource_visibility_filter(visibility: Option<&str>) -> Result<(), String> {
    if let Some(visibility) = visibility {
        if !matches!(visibility, "public" | "private") {
            return Err("resources.list visibility must be 'public' or 'private'.".to_string());
        }
    }
    Ok(())
}

fn listed_package_resources(
    records: &HashMap<String, PackageRecord>,
    caller_package_id: &str,
    package_id: Option<&str>,
    resource_type: Option<&str>,
    visibility: Option<&str>,
) -> Vec<serde_json::Value> {
    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .filter(|record| package_id.is_none_or(|package_id| record.manifest.id() == package_id))
        .flat_map(|record| {
            let is_owner = record.manifest.id() == caller_package_id;
            record
                .descriptor
                .contributions
                .resources
                .iter()
                .filter(move |resource| is_owner || resource.public)
                .filter(move |resource| {
                    resource_type.is_none_or(|resource_type| {
                        resource.resource_type.as_deref() == Some(resource_type)
                    })
                })
                .filter(move |resource| {
                    visibility.is_none_or(|visibility| resource.visibility == visibility)
                })
                .map(|resource| {
                    serde_json::json!({
                        "packageId": record.manifest.id(),
                        "id": resource.name,
                        "reference": resource.reference,
                        "paths": resource.paths,
                        "type": resource.resource_type,
                        "visibility": resource.visibility,
                        "public": resource.public,
                        "metadata": resource.metadata,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn listed_extension_contributions(
    records: &HashMap<String, PackageRecord>,
    target: Option<&str>,
) -> Vec<serde_json::Value> {
    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .flat_map(|record| {
            record
                .descriptor
                .contributions
                .contributions
                .iter()
                .filter(|contribution| target.is_none_or(|target| contribution.target == target))
                .filter(|contribution| extension_target_is_active(records, &contribution.target))
                .map(|contribution| {
                    serde_json::json!({
                        "packageId": record.manifest.id(),
                        "id": contribution.name,
                        "reference": format!("{}/{}", record.manifest.id(), contribution.name),
                        "target": contribution.target,
                        "kind": contribution.kind,
                        "title": contribution.title,
                        "description": contribution.description,
                        "dataSchema": contribution.data_schema,
                        "service": contribution.service,
                        "resources": contribution.resources,
                        "metadata": contribution.metadata,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn extension_target_is_active(records: &HashMap<String, PackageRecord>, target: &str) -> bool {
    let Some((package_id, extension_point_id)) = target.split_once('/') else {
        return false;
    };
    let Some(record) = records.get(package_id) else {
        return false;
    };
    record.descriptor.enabled
        && record.descriptor.error.is_none()
        && record.descriptor.compatibility.is_compatible()
        && record
            .descriptor
            .contributions
            .extension_points
            .iter()
            .any(|point| point.name == extension_point_id)
}

fn apply_sidecar_runtime_statuses(
    packages: &mut [PackageDescriptor],
    statuses: &HashMap<String, SidecarRuntimeStatus>,
) {
    for package in packages {
        package.sidecar_statuses.clear();
        let Some(runtime) = package.runtime.as_ref() else {
            continue;
        };
        for sidecar in &runtime.sidecars {
            let sidecar_ref = format!("{}/{}", package.id, sidecar.id);
            if let Some(status) = statuses.get(&sidecar_ref) {
                package
                    .sidecar_statuses
                    .insert(sidecar.id.clone(), status.clone());
            }
        }
    }
}

fn apply_extension_host_runtime_statuses(
    packages: &mut [PackageDescriptor],
    statuses: &HashMap<String, extension_host_runtime::ExtensionHostRuntimeStatus>,
) {
    for package in packages {
        package.extension_host_status = None;
        if package
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.node.as_ref())
            .is_none()
        {
            continue;
        }
        package.extension_host_status = statuses.get(&package.id).cloned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_record(raw: serde_json::Value) -> PackageRecord {
        let manifest = PluginPackageManifest::parse(&raw.to_string()).unwrap();
        let descriptor =
            descriptor_for_manifest(&manifest, format!("/tmp/{}", manifest.id()), true);
        PackageRecord {
            descriptor,
            manifest,
        }
    }

    #[test]
    fn sidecar_runtime_statuses_are_attached_by_sidecar_id() {
        let mut packages = vec![
            package_record(serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "bakingrl.sidecar",
                "name": "Sidecar",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "runtime": {
                    "sidecars": [
                        {
                            "id": "helper",
                            "bin": "bin/helper",
                            "protocol": "jsonrpc-stdio"
                        },
                        {
                            "id": "cold",
                            "bin": "bin/cold",
                            "protocol": "jsonrpc-stdio"
                        }
                    ]
                },
                "contributes": {}
            }))
            .descriptor,
        ];
        let mut statuses = HashMap::new();
        statuses.insert(
            "bakingrl.sidecar/helper".to_string(),
            SidecarRuntimeStatus {
                running: true,
                healthy: Some(true),
                restart_count: 2,
                ..SidecarRuntimeStatus::default()
            },
        );
        statuses.insert(
            "bakingrl.other/helper".to_string(),
            SidecarRuntimeStatus {
                running: true,
                ..SidecarRuntimeStatus::default()
            },
        );

        apply_sidecar_runtime_statuses(&mut packages, &statuses);

        assert_eq!(packages[0].sidecar_statuses.len(), 1);
        assert_eq!(
            packages[0].sidecar_statuses.get("helper").map(|status| (
                status.running,
                status.healthy,
                status.restart_count
            )),
            Some((true, Some(true), 2))
        );
        assert!(!packages[0].sidecar_statuses.contains_key("cold"));
    }

    #[test]
    fn extension_host_runtime_statuses_are_attached_to_node_packages() {
        let mut packages = vec![
            package_record(serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "bakingrl.node",
                "name": "Node",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "runtime": {
                    "node": {
                        "entry": "dist/extension/index.js"
                    }
                },
                "contributes": {}
            }))
            .descriptor,
            package_record(serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "bakingrl.content",
                "name": "Content",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "contributes": {}
            }))
            .descriptor,
        ];
        let mut statuses = HashMap::new();
        statuses.insert(
            "bakingrl.node".to_string(),
            extension_host_runtime::ExtensionHostRuntimeStatus {
                state: extension_host_runtime::ExtensionHostRuntimeState::Running,
                running: true,
                ..extension_host_runtime::ExtensionHostRuntimeStatus::default()
            },
        );
        statuses.insert(
            "bakingrl.content".to_string(),
            extension_host_runtime::ExtensionHostRuntimeStatus {
                state: extension_host_runtime::ExtensionHostRuntimeState::Running,
                running: true,
                ..extension_host_runtime::ExtensionHostRuntimeStatus::default()
            },
        );

        apply_extension_host_runtime_statuses(&mut packages, &statuses);

        assert_eq!(
            packages[0]
                .extension_host_status
                .as_ref()
                .map(|status| (&status.state, status.running)),
            Some((
                &extension_host_runtime::ExtensionHostRuntimeState::Running,
                true
            ))
        );
        assert!(packages[1].extension_host_status.is_none());
    }

    #[test]
    fn caller_depends_on_requires_declared_dependency() {
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "bakingrl.provider"
                }
            ],
            "contributes": {}
        }));

        assert!(caller_depends_on("bakingrl.provider", &caller));
        assert!(!caller_depends_on("bakingrl.other", &caller));
    }

    #[test]
    fn runtime_packages_expose_graph_without_foreign_private_resources() {
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "bakingrl.provider"
                }
            ],
            "contributes": {}
        }));
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension/index.js"
                }
            },
            "contributes": {
                "commands": [
                    {
                        "id": "open"
                    }
                ],
                "services": [
                    {
                        "id": "providerService",
                        "runtime": "node",
                        "methods": ["snapshot"]
                    }
                ],
                "extensionPoints": [
                    {
                        "id": "provider.items",
                        "version": "1.0.0"
                    }
                ],
                "contributions": [
                    {
                        "id": "localItem",
                        "target": "bakingrl.provider/provider.items",
                        "resources": ["publicJson"]
                    }
                ],
                "resources": [
                    {
                        "id": "publicJson",
                        "path": "resources/public.json",
                        "type": "application/json",
                        "visibility": "public"
                    },
                    {
                        "id": "privateJson",
                        "path": "resources/private.json",
                        "type": "application/json",
                        "visibility": "private"
                    }
                ],
                "webviews": [
                    {
                        "id": "studio",
                        "entry": "dist/webviews/studio.js"
                    }
                ]
            }
        }));
        let records = HashMap::from([
            ("bakingrl.consumer".to_string(), caller),
            ("bakingrl.provider".to_string(), provider),
        ]);

        let foreign_view = runtime_packages_for_records(&records, "bakingrl.consumer");
        let provider_summary = foreign_view
            .iter()
            .find(|package| package["id"] == "bakingrl.provider")
            .unwrap();
        assert_eq!(
            provider_summary["runtime"]["node"]["entry"],
            "dist/extension/index.js"
        );
        assert_eq!(
            provider_summary["contributes"]["services"][0]["id"],
            "providerService"
        );
        assert_eq!(
            provider_summary["contributes"]["extensionPoints"][0]["id"],
            "provider.items"
        );
        assert_eq!(
            provider_summary["contributes"]["contributions"][0]["id"],
            "localItem"
        );
        assert_eq!(
            provider_summary["contributes"]["webviews"][0]["id"],
            "studio"
        );
        assert_eq!(
            provider_summary["contributes"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|resource| resource["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["publicJson"]
        );

        let owner_view = runtime_packages_for_records(&records, "bakingrl.provider");
        let owner_summary = owner_view
            .iter()
            .find(|package| package["id"] == "bakingrl.provider")
            .unwrap();
        assert_eq!(
            owner_summary["contributes"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|resource| {
                    (
                        resource["id"].as_str().unwrap(),
                        resource["public"].as_bool().unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("publicJson", true), ("privateJson", false)]
        );
    }

    #[test]
    fn command_exports_require_declared_dependency_and_command() {
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "commands": [
                    {
                        "id": "openSettings",
                        "title": "Open Settings"
                    }
                ]
            }
        }));
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "bakingrl.provider"
                }
            ],
            "contributes": {}
        }));
        let unrelated = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.other",
            "name": "Other",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {}
        }));

        assert!(validate_command_export(
            &caller,
            &provider,
            "bakingrl.consumer",
            "bakingrl.provider",
            "bakingrl.provider/openSettings",
            "openSettings",
        )
        .is_ok());
        assert!(validate_command_export(
            &unrelated,
            &provider,
            "bakingrl.other",
            "bakingrl.provider",
            "bakingrl.provider/openSettings",
            "openSettings",
        )
        .unwrap_err()
        .contains("without declaring a dependency"));
        assert_eq!(
            validate_command_export(
                &caller,
                &provider,
                "bakingrl.consumer",
                "bakingrl.provider",
                "bakingrl.provider/missing",
                "missing",
            )
            .unwrap_err(),
            "Command export 'bakingrl.provider/missing' does not exist."
        );
    }

    #[test]
    fn extension_contributions_require_active_target_extension_point() {
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "extensionPoints": [
                    {
                        "id": "overlay.visual",
                        "version": "1.0.0"
                    }
                ]
            }
        }));
        let consumer = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "bakingrl.provider"
                }
            ],
            "contributes": {
                "contributions": [
                    {
                        "id": "scoreboardBinding",
                        "target": "bakingrl.provider/overlay.visual",
                        "kind": "widget",
                        "metadata": {
                            "placement": "bottom-right"
                        }
                    }
                ]
            }
        }));
        let records = HashMap::from([
            ("bakingrl.provider".to_string(), provider),
            ("bakingrl.consumer".to_string(), consumer),
        ]);

        assert_eq!(
            contribution_ids(listed_extension_contributions(
                &records,
                Some("bakingrl.provider/overlay.visual"),
            )),
            vec!["scoreboardBinding"]
        );
        assert!(
            listed_extension_contributions(&records, Some("bakingrl.provider/missing.visual"),)
                .is_empty()
        );

        let mut disabled_target = records;
        disabled_target
            .get_mut("bakingrl.provider")
            .unwrap()
            .descriptor
            .enabled = false;
        assert!(listed_extension_contributions(
            &disabled_target,
            Some("bakingrl.provider/overlay.visual"),
        )
        .is_empty());

        let mut missing_point = disabled_target;
        missing_point
            .get_mut("bakingrl.provider")
            .unwrap()
            .descriptor
            .enabled = true;
        missing_point
            .get_mut("bakingrl.provider")
            .unwrap()
            .descriptor
            .contributions
            .extension_points
            .clear();
        assert!(listed_extension_contributions(
            &missing_point,
            Some("bakingrl.provider/overlay.visual"),
        )
        .is_empty());
    }

    #[test]
    fn package_resources_filter_by_package_type_and_visible_scope() {
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "resources": [
                    {
                        "id": "consumerPrivate",
                        "path": "resources/consumer-private.json",
                        "type": "application/json",
                        "visibility": "private"
                    }
                ]
            }
        }));
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "resources": [
                    {
                        "id": "publicJson",
                        "path": "resources/public.json",
                        "type": "application/json",
                        "visibility": "public",
                        "metadata": { "role": "overlay-content" }
                    },
                    {
                        "id": "publicSvg",
                        "path": "resources/public.svg",
                        "type": "image/svg+xml",
                        "visibility": "public",
                        "metadata": { "role": "team-badges" }
                    },
                    {
                        "id": "privateJson",
                        "path": "resources/private.json",
                        "type": "application/json",
                        "visibility": "private"
                    }
                ]
            }
        }));
        let records = HashMap::from([
            ("bakingrl.consumer".to_string(), caller),
            ("bakingrl.provider".to_string(), provider),
        ]);

        assert_eq!(
            resource_ids(listed_package_resources(
                &records,
                "bakingrl.consumer",
                Some("bakingrl.provider"),
                Some("application/json"),
                Some("public"),
            )),
            vec!["publicJson"]
        );
        assert!(listed_package_resources(
            &records,
            "bakingrl.consumer",
            Some("bakingrl.provider"),
            None,
            Some("private"),
        )
        .is_empty());

        assert_eq!(
            resource_ids(listed_package_resources(
                &records,
                "bakingrl.provider",
                Some("bakingrl.provider"),
                Some("application/json"),
                Some("private"),
            )),
            vec!["privateJson"]
        );

        let invalid_visibility = validate_resource_visibility_filter(Some("internal")).unwrap_err();
        assert!(invalid_visibility.contains("visibility must be 'public' or 'private'"));
    }

    #[test]
    fn preferred_settings_webview_uses_settings_id_first() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension/index.js"
                }
            },
            "contributes": {
                "webviews": [
                    {
                        "id": "preferences",
                        "entry": "dist/webviews/preferences.js",
                        "kind": "settings"
                    },
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js",
                        "kind": "settings"
                    },
                    {
                        "id": "studio",
                        "entry": "dist/webviews/studio.js",
                        "kind": "tool"
                    }
                ]
            }
        }));

        assert_eq!(
            preferred_settings_webview_id(&record),
            Some("settings".to_string())
        );
    }

    #[test]
    fn package_webview_route_targets_declared_webview_without_source_query() {
        let route = package_webview_route("com.example.plugin", "settings");

        assert_eq!(route, "/plugin-webview/com.example.plugin/settings");
        assert!(!route.contains("entry="));
        assert!(!route.contains("path="));
        assert!(!route.contains("runtimeApi="));
    }

    #[test]
    fn package_webview_runtime_descriptor_uses_declared_manifest_entry() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js",
                        "kind": "settings"
                    }
                ]
            }
        }));

        let descriptor =
            package_webview_runtime_descriptor_for_record("bakingrl.webviews", "settings", &record)
                .unwrap();

        assert_eq!(descriptor.package_id, "bakingrl.webviews");
        assert_eq!(descriptor.webview_id, "settings");
        assert_eq!(descriptor.entry, "dist/webviews/settings.js");
        assert_eq!(descriptor.kind.as_deref(), Some("settings"));
        assert_eq!(descriptor.runtime_api, "2.3.0");
    }

    #[test]
    fn package_webview_runtime_descriptor_rejects_undeclared_webview() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js"
                    }
                ]
            }
        }));

        let error =
            package_webview_runtime_descriptor_for_record("bakingrl.webviews", "admin", &record)
                .unwrap_err();

        assert!(error.contains("does not declare webview 'admin'"));
    }

    #[test]
    fn package_webview_diagnostic_input_targets_declared_webview() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js"
                    }
                ]
            }
        }));

        let input = package_webview_diagnostic_input_for_record(
            "bakingrl.webviews",
            "settings",
            &record,
            PluginDiagnosticSeverity::Warning,
            Some(" render ".to_string()),
            " failed to refresh ".to_string(),
        )
        .unwrap();

        assert_eq!(input.package_id.as_deref(), Some("bakingrl.webviews"));
        assert_eq!(input.source, "webview:settings");
        assert_eq!(input.severity, PluginDiagnosticSeverity::Warning);
        assert_eq!(input.phase, "render");
        assert_eq!(input.message, "failed to refresh");
        assert_eq!(diagnostic_log_stream(&input.severity), "warning");
    }

    #[test]
    fn package_webview_diagnostic_input_rejects_undeclared_webview() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js"
                    }
                ]
            }
        }));

        let error = package_webview_diagnostic_input_for_record(
            "bakingrl.webviews",
            "admin",
            &record,
            PluginDiagnosticSeverity::Info,
            None,
            "".to_string(),
        )
        .unwrap_err();

        assert!(error.contains("does not declare webview 'admin'"));
    }

    #[test]
    fn package_webview_module_paths_stay_with_declared_bundle() {
        let entry = "dist/webviews/settings.js";

        assert_eq!(
            package_webview_module_relative_path(entry, entry).unwrap(),
            PathBuf::from("dist/webviews/settings.js")
        );
        assert_eq!(
            package_webview_module_relative_path(entry, "dist/assets/settings-helper.js").unwrap(),
            PathBuf::from("dist/assets/settings-helper.js")
        );

        let outside_bundle =
            package_webview_module_relative_path(entry, "src/extension/index.js").unwrap_err();
        assert!(outside_bundle.contains("declared webview bundle"));

        let non_js =
            package_webview_module_relative_path(entry, "dist/assets/style.css").unwrap_err();
        assert!(non_js.contains("built .js"));

        let escaping = package_webview_module_relative_path(entry, "../secret.js").unwrap_err();
        assert!(escaping.contains("escapes the package root"));
    }

    #[test]
    fn package_window_labels_are_scoped_to_declared_package_surfaces() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js",
                        "kind": "settings"
                    },
                    {
                        "id": "tool",
                        "entry": "dist/webviews/tool.js",
                        "kind": "tool"
                    }
                ]
            }
        }));

        assert!(window_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.webviews", "settings")
        ));
        assert!(window_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.webviews", "tool")
        ));
        assert!(!window_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            "main"
        ));
        assert!(!window_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.other", "settings")
        ));
        assert!(!window_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.webviews", "undeclared")
        ));
    }

    #[test]
    fn settings_webview_labels_are_the_only_webviews_that_can_configure_package() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settings",
                        "entry": "dist/webviews/settings.js",
                        "kind": "settings"
                    },
                    {
                        "id": "tool",
                        "entry": "dist/webviews/tool.js",
                        "kind": "tool"
                    }
                ]
            }
        }));

        assert!(settings_webview_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.webviews", "settings")
        ));
        assert!(!settings_webview_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.webviews", "tool")
        ));
        assert!(!settings_webview_label_can_access_package_record(
            "bakingrl.webviews",
            &record,
            &package_webview_window_label("bakingrl.other", "settings")
        ));
    }

    #[test]
    fn package_window_labels_do_not_collapse_punctuation() {
        assert_ne!(
            package_webview_window_label("bakingrl.poc", "settings"),
            package_webview_window_label("bakingrl-poc", "settings")
        );
        assert_ne!(
            page_window_label("configuration-bakingrl.poc"),
            page_window_label("configuration-bakingrl-poc")
        );
    }

    #[test]
    fn package_webview_commands_require_exact_window_label() {
        let label = package_webview_window_label("bakingrl.poc", "settings");

        assert!(ensure_package_webview_window_label(
            &label,
            "bakingrl.poc",
            "settings",
            "read runtime descriptor"
        )
        .is_ok());
        assert!(ensure_package_webview_window_label(
            &label,
            "bakingrl.poc",
            "other",
            "read runtime descriptor"
        )
        .unwrap_err()
        .contains("cannot read runtime descriptor"));
    }

    #[test]
    fn plugin_namespaces_are_package_scoped_without_prefix_collisions() {
        assert!(key_is_in_plugin_namespace(
            "com.example.plugin",
            "plugin.com.example.plugin.state"
        ));
        assert!(!key_is_in_plugin_namespace(
            "com.example.plugin",
            "plugin.com.example.plugin-extra.state"
        ));
        assert!(!key_is_in_plugin_namespace(
            "com.example.plugin",
            "UpdateState"
        ));
        assert!(ensure_plugin_namespace(
            "com.example.plugin",
            "plugin.com.example.plugin.state",
            "event name"
        )
        .is_ok());
        assert!(
            ensure_plugin_namespace("com.example.plugin", "UpdateState", "event name")
                .unwrap_err()
                .contains("outside namespace")
        );
    }

    #[test]
    fn registry_keys_resolve_to_declared_plugin_namespace_owners() {
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {}
        }));
        let other = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.provider-extra",
            "name": "Provider Extra",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {}
        }));
        let records = HashMap::from([
            ("com.example.provider".to_string(), provider),
            ("com.example.provider-extra".to_string(), other),
        ]);

        assert_eq!(
            registry_key_owner(&records, "plugin.com.example.provider.state"),
            Some("com.example.provider")
        );
        assert_eq!(
            registry_key_owner(&records, "plugin.com.example.provider-extra.state"),
            Some("com.example.provider-extra")
        );
        assert_eq!(registry_key_owner(&records, "plugin.unknown.state"), None);
    }

    #[test]
    fn registry_keys_can_match_declared_dependency_namespaces() {
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "com.example.provider",
                    "optional": true
                }
            ],
            "contributes": {}
        }));

        assert_eq!(
            dependency_key_owner(&caller, "plugin.com.example.provider.state"),
            Some("com.example.provider")
        );
        assert_eq!(
            dependency_key_owner(&caller, "plugin.com.example.provider-extra.state"),
            None
        );
    }

    fn resource_ids(resources: Vec<serde_json::Value>) -> Vec<String> {
        let mut ids = resources
            .into_iter()
            .filter_map(|resource| {
                resource
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    fn contribution_ids(contributions: Vec<serde_json::Value>) -> Vec<String> {
        let mut ids = contributions
            .into_iter()
            .filter_map(|contribution| {
                contribution
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }
}

fn select_resource_path(
    resource_ref: &str,
    paths: &[String],
    requested_path: Option<&str>,
) -> Result<String, String> {
    if paths.is_empty() {
        return Err(format!(
            "Resource '{resource_ref}' does not declare a file path."
        ));
    }
    if paths.len() == 1 {
        return Ok(paths[0].clone());
    }
    let requested_path = requested_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!("Resource '{resource_ref}' contains multiple files; a path is required.")
        })?;
    if paths.iter().any(|path| path == requested_path) {
        return Ok(requested_path.to_string());
    }
    Err(format!(
        "Resource '{resource_ref}' does not expose path '{requested_path}'."
    ))
}

fn content_type_for_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn merge_settings(
    schema_path: Option<&str>,
    package_root: &Path,
    values: serde_json::Value,
) -> serde_json::Value {
    merge_package_settings(schema_path, package_root, values)
}

fn unique_id(prefix: &str) -> String {
    let millis = now_ms();
    format!("{prefix}-{millis}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn page_window_label(page_id: &str) -> String {
    format!("page-{}", window_label_component(page_id))
}

fn ensure_admin_window_label(window_label: &str) -> Result<(), String> {
    if window_label == "main" {
        Ok(())
    } else {
        Err(format!(
            "Window '{window_label}' cannot call admin-only package APIs."
        ))
    }
}

fn ensure_package_webview_window_label(
    window_label: &str,
    package_id: &str,
    webview_id: &str,
    action: &str,
) -> Result<(), String> {
    let expected_label = package_webview_window_label(package_id, webview_id);
    let expected_surface_label = surface_window_label(package_id, webview_id);
    if window_label == expected_label || window_label == expected_surface_label {
        Ok(())
    } else {
        Err(format!(
            "Window '{window_label}' cannot {action} for webview '{package_id}/{webview_id}'."
        ))
    }
}

fn ensure_window_label_can_access_package(
    host: &PluginHost,
    window_label: &str,
    package_id: &str,
) -> Result<(), String> {
    if window_label == "main" {
        return Ok(());
    }
    if window_label == page_window_label(&format!("configuration-{package_id}"))
        || window_label == page_window_label(&format!("secrets-{package_id}"))
    {
        return Ok(());
    }

    let records = host.records.lock().unwrap();
    let record = records
        .get(package_id)
        .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
    if window_label_can_access_package_record(package_id, record, window_label) {
        Ok(())
    } else {
        Err(format!(
            "Window '{window_label}' cannot access package '{package_id}'."
        ))
    }
}

fn window_label_can_access_package_record(
    package_id: &str,
    record: &PackageRecord,
    window_label: &str,
) -> bool {
    record
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .any(|webview| {
            window_label == package_webview_window_label(package_id, &webview.id)
                || (webview.kind.as_deref() == Some("surface")
                    && window_label == surface_window_label(package_id, &webview.id))
        })
}

fn settings_webview_label_can_access_package_record(
    package_id: &str,
    record: &PackageRecord,
    window_label: &str,
) -> bool {
    record
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .any(|webview| {
            is_settings_webview(webview)
                && window_label == package_webview_window_label(package_id, &webview.id)
        })
}

fn ensure_plugin_namespace(package_id: &str, value: &str, label: &str) -> Result<(), String> {
    if key_is_in_plugin_namespace(package_id, value) {
        Ok(())
    } else {
        Err(format!(
            "Package '{package_id}' cannot write {label} '{value}' outside namespace 'plugin.{package_id}.'."
        ))
    }
}

fn key_is_in_plugin_namespace(package_id: &str, value: &str) -> bool {
    value
        .strip_prefix("plugin.")
        .and_then(|rest| rest.strip_prefix(package_id))
        .is_some_and(|rest| rest.starts_with('.'))
}

fn registry_key_owner<'a>(
    records: &'a HashMap<String, PackageRecord>,
    key: &str,
) -> Option<&'a str> {
    records
        .keys()
        .map(String::as_str)
        .find(|package_id| key_is_in_plugin_namespace(package_id, key))
}

fn dependency_key_owner<'a>(record: &'a PackageRecord, key: &str) -> Option<&'a str> {
    record
        .manifest
        .dependencies_v4()
        .iter()
        .map(|dependency| dependency.package_id.as_str())
        .find(|package_id| key_is_in_plugin_namespace(package_id, key))
}

fn preferred_settings_webview_id(record: &PackageRecord) -> Option<String> {
    record
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .find(|webview| is_settings_webview(webview) && webview.id == "settings")
        .or_else(|| {
            record
                .manifest
                .contributes_v4()
                .webviews
                .iter()
                .find(|webview| is_settings_webview(webview))
        })
        .map(|webview| webview.id.clone())
}

fn is_settings_webview(
    webview: &crate::plugin_package::manifest::PluginWebviewContributionV4,
) -> bool {
    webview.kind.as_deref() == Some("settings")
}

fn package_webview_diagnostic_input_for_record(
    package_id: &str,
    webview_id: &str,
    record: &PackageRecord,
    severity: PluginDiagnosticSeverity,
    phase: Option<String>,
    message: String,
) -> Result<PluginDiagnosticInput, String> {
    if !record
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .any(|webview| webview.id == webview_id)
    {
        return Err(format!(
            "Package '{package_id}' does not declare webview '{webview_id}'."
        ));
    }
    Ok(PluginDiagnosticInput {
        package_id: Some(package_id.to_string()),
        source: format!("webview:{webview_id}"),
        severity,
        phase: normalized_diagnostic_phase(phase, "webview"),
        message: normalized_diagnostic_message(message),
        crash_count: None,
    })
}

fn normalized_diagnostic_phase(phase: Option<String>, fallback: &str) -> String {
    phase
        .map(|phase| phase.trim().to_string())
        .filter(|phase| !phase.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn normalized_diagnostic_message(message: String) -> String {
    let message = message.trim().to_string();
    if message.is_empty() {
        "(empty diagnostic)".to_string()
    } else {
        message
    }
}

fn diagnostic_log_stream(severity: &PluginDiagnosticSeverity) -> &'static str {
    match severity {
        PluginDiagnosticSeverity::Info => "diagnostics",
        PluginDiagnosticSeverity::Warning => "warning",
        PluginDiagnosticSeverity::Error | PluginDiagnosticSeverity::Fatal => "error",
    }
}

fn package_webview_runtime_descriptor_for_record(
    package_id: &str,
    webview_id: &str,
    record: &PackageRecord,
) -> Result<PackageWebviewRuntimeDescriptor, String> {
    let webview = record
        .manifest
        .contributes_v4()
        .webviews
        .iter()
        .find(|webview| webview.id == webview_id)
        .ok_or_else(|| {
            format!("Package '{package_id}' does not declare webview '{webview_id}'.")
        })?;

    Ok(PackageWebviewRuntimeDescriptor {
        package_id: package_id.to_string(),
        webview_id: webview.id.clone(),
        entry: webview.entry.clone(),
        kind: webview.kind.clone(),
        runtime_api: record.manifest.bakingrl_api().to_string(),
    })
}

fn package_webview_module_relative_path(
    entry: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let entry_path = safe_package_relative_path(entry)?;
    let requested_path = safe_package_relative_path(relative_path)?;
    if requested_path.extension().and_then(|ext| ext.to_str()) != Some("js") {
        return Err(format!(
            "Package webview module '{relative_path}' must be a built .js file."
        ));
    }
    if requested_path == entry_path {
        return Ok(requested_path);
    }
    if entry_path.iter().next().is_some()
        && entry_path.iter().next() == requested_path.iter().next()
    {
        return Ok(requested_path);
    }
    Err(format!(
        "Package webview module '{relative_path}' is outside the declared webview bundle."
    ))
}

fn combine_rollback_error(error: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => format!("{error} Rollback also failed: {rollback_error}"),
    }
}

fn package_webview_route(package_id: &str, webview_id: &str) -> String {
    format!(
        "/plugin-webview/{}/{}",
        encode_route_segment(package_id),
        encode_route_segment(webview_id)
    )
}

fn package_webview_window_label(package_id: &str, webview_id: &str) -> String {
    format!(
        "plugin-webview-{}-{}",
        window_label_component(package_id),
        window_label_component(webview_id)
    )
}

fn window_label_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (byte as char).to_string()
            } else {
                format!("_{byte:02x}")
            }
        })
        .collect()
}

fn encode_route_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[tauri::command]
pub fn list_packages(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PackageDescriptor>, String> {
    ensure_admin_window_label(window.label())?;
    Ok(host.list_packages())
}

#[tauri::command]
pub async fn get_marketplace_snapshot(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    refresh: bool,
) -> Result<MarketplaceSnapshot, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.marketplace_snapshot(refresh).await
}

#[tauri::command]
pub fn complete_marketplace_first_run(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.complete_marketplace_first_run()
}

#[tauri::command]
pub async fn prepare_marketplace_install(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_ids: Vec<String>,
) -> Result<MarketplaceInstallPlan, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.prepare_marketplace_install(package_ids).await
}

#[tauri::command]
pub fn commit_marketplace_install(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    transaction_id: String,
    accepted_publishers: Vec<String>,
) -> Result<MarketplaceInstallResult, String> {
    ensure_admin_window_label(window.label())?;
    host.commit_marketplace_install(transaction_id, accepted_publishers)
}

#[tauri::command]
pub fn discard_marketplace_install(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    transaction_id: String,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.discard_marketplace_install(transaction_id)
}

#[tauri::command]
pub fn inspect_package_bundle(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<crate::plugin_package::bundle::BundleInspection, String> {
    ensure_admin_window_label(window.label())?;
    host.inspect_package_bundle(path)
}

#[tauri::command]
pub fn install_package_from_file(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<InstallReceipt, String> {
    ensure_admin_window_label(window.label())?;
    host.install_package_from_file(path)
}

#[tauri::command]
pub async fn install_package_from_url(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    url: String,
) -> Result<InstallReceipt, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.install_package_from_url(url).await
}

#[tauri::command]
pub async fn prepare_package_from_url(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    url: String,
) -> Result<PreparedPackageInstall, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.prepare_package_from_url(url).await
}

#[tauri::command]
pub async fn prepare_package_from_deep_link(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    deep_link: String,
) -> Result<PreparedPackageInstall, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.prepare_package_from_deep_link(deep_link).await
}

#[tauri::command]
pub async fn prepare_package_from_git(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    repo: String,
    rev: Option<String>,
) -> Result<PreparedPackageInstall, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    host.prepare_package_from_git(repo, rev).await
}

#[tauri::command]
pub fn install_prepared_package(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    path: String,
    source: String,
) -> Result<InstallReceipt, String> {
    ensure_admin_window_label(window.label())?;
    host.install_prepared_package(path, source)
}

#[tauri::command]
pub fn discard_prepared_package(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.discard_prepared_package(path)
}

#[tauri::command]
pub fn packages_dir(window: Window, host: State<'_, Arc<PluginHost>>) -> Result<String, String> {
    ensure_admin_window_label(window.label())?;
    Ok(host.packages_dir())
}

#[tauri::command]
pub fn get_runtime_info() -> RuntimeInfo {
    runtime_info()
}

#[tauri::command]
pub fn list_plugin_diagnostics(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PluginDiagnosticEvent>, String> {
    ensure_admin_window_label(window.label())?;
    Ok(host.list_diagnostics())
}

#[tauri::command]
pub fn clear_plugin_diagnostics(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.clear_diagnostics();
    Ok(())
}

#[tauri::command]
pub async fn reload_packages(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PackageDescriptor>, String> {
    ensure_admin_window_label(window.label())?;
    drop(window);
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.reload_packages())
        .await
        .map_err(|err| format!("Unable to reload packages: {err}"))
}

#[tauri::command]
pub fn set_package_enabled(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    enabled: bool,
) -> Result<Vec<PackageDescriptor>, String> {
    ensure_admin_window_label(window.label())?;
    host.set_package_enabled(package_id, enabled)
}

#[tauri::command]
pub fn remove_package(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<Vec<PackageDescriptor>, String> {
    ensure_admin_window_label(window.label())?;
    let host = host.inner().clone();
    let removal = host.begin_remove_package(&package_id)?;
    if removal.started {
        tauri::async_runtime::spawn_blocking(move || {
            host.remove_package_in_background(package_id);
        });
    }
    Ok(removal.packages)
}

#[tauri::command]
pub fn read_package_webview_module_text(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
    relative_path: String,
) -> Result<String, String> {
    ensure_package_webview_window_label(
        window.label(),
        &package_id,
        &webview_id,
        "read module files",
    )?;
    host.read_package_webview_module_text(&package_id, &webview_id, &relative_path)
}

#[tauri::command]
pub fn read_package_webview_asset(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
    relative_path: String,
) -> Result<PackageWebviewAssetPayload, String> {
    ensure_package_webview_window_label(window.label(), &package_id, &webview_id, "read assets")?;
    host.read_package_webview_asset(&package_id, &relative_path)
}

#[tauri::command]
pub fn get_package_webview_runtime_descriptor(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
) -> Result<PackageWebviewRuntimeDescriptor, String> {
    ensure_package_webview_window_label(
        window.label(),
        &package_id,
        &webview_id,
        "read runtime descriptor",
    )?;
    host.package_webview_runtime_descriptor(&package_id, &webview_id)
}

#[tauri::command]
pub fn push_package_webview_diagnostic(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
    severity: PluginDiagnosticSeverity,
    phase: Option<String>,
    message: String,
) -> Result<PluginDiagnosticEvent, String> {
    ensure_package_webview_window_label(
        window.label(),
        &package_id,
        &webview_id,
        "report diagnostics",
    )?;
    host.push_package_webview_diagnostic(&package_id, &webview_id, severity, phase, message)
}

#[tauri::command]
pub fn emit_package_webview_event(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    bus: State<'_, Arc<EventBus>>,
    package_id: String,
    webview_id: String,
    event_name: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    ensure_package_webview_window_label(
        window.label(),
        &package_id,
        &webview_id,
        "publish events",
    )?;
    host.can_package_write_event(&package_id, &event_name)?;
    bus.publish(BusEvent::PluginEvent(Arc::new(GameEvent {
        event: event_name,
        data: payload,
    })));
    Ok(())
}

#[tauri::command]
pub async fn call_service_export(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    caller_package_id: String,
    service_ref: String,
    method: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    ensure_window_label_can_access_package(
        host.inner().as_ref(),
        window.label(),
        &caller_package_id,
    )?;
    drop(window);
    host.call_service_export(&caller_package_id, &service_ref, &method, input)
        .await
}

#[tauri::command]
pub fn plugin_registry_get(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    registry: State<'_, Arc<Registry>>,
    package_id: String,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.can_package_read_registry(&package_id, &key)?;
    Ok(registry.get(&key))
}

#[tauri::command]
pub fn get_app_settings(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
) -> Result<AppSettings, String> {
    ensure_admin_window_label(window.label())?;
    Ok(host.get_app_settings())
}

#[tauri::command]
pub fn save_app_settings(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    ensure_admin_window_label(window.label())?;
    host.save_app_settings(settings)
}

#[tauri::command]
pub fn get_package_settings(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<serde_json::Value, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.get_package_settings(&package_id)
}

#[tauri::command]
pub fn save_package_settings(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    values: serde_json::Value,
) -> Result<serde_json::Value, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.can_window_save_package_settings(window.label(), &package_id)?;
    host.save_package_settings(package_id, values)
}

#[tauri::command]
pub fn get_package_configuration_state(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<PackageConfigurationState, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.get_package_configuration_state(package_id)
}

#[tauri::command]
pub fn set_package_secret(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    key: String,
    value: String,
) -> Result<PackageConfigurationState, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.can_window_manage_package_secrets(window.label(), &package_id)?;
    host.set_package_secret(package_id, key, value)
}

#[tauri::command]
pub fn delete_package_secret(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    key: String,
) -> Result<PackageConfigurationState, String> {
    ensure_window_label_can_access_package(host.inner().as_ref(), window.label(), &package_id)?;
    host.can_window_manage_package_secrets(window.label(), &package_id)?;
    host.delete_package_secret(package_id, key)
}

#[tauri::command]
pub fn open_package_webview(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.open_package_webview(package_id, webview_id)
}

#[tauri::command]
pub fn open_package_configuration(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.open_package_configuration(package_id)
}

#[tauri::command]
pub fn open_package_secrets(
    window: Window,
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<(), String> {
    ensure_admin_window_label(window.label())?;
    host.open_package_secrets(package_id)
}
