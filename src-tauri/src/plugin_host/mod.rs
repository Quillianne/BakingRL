mod descriptors;
mod diagnostics;
pub(crate) mod extension_host_runtime;
mod json_store;
mod package_files;
mod runtime_specs;
mod service_registry;
mod settings_contract;
pub(crate) mod sidecar_runtime;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, Window};
use tracing::{info, warn};

use crate::bus::EventBus;
use crate::models::{AppSettings, PackageSettingsFile, PackageStateFile};
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
    pub runtime_api: String,
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
    state_path: PathBuf,
    app_settings_path: PathBuf,
    package_settings_path: PathBuf,
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

        Ok(Self {
            app_handle,
            bus,
            registry,
            packages_dir,
            state_path: app_data.join("package_state.json"),
            app_settings_path: app_data.join("app_settings.json"),
            package_settings_path: app_data.join("package_settings.json"),
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

    #[allow(dead_code)]
    pub(crate) fn reload_v3_extension_hosts(
        &self,
        specs: Vec<ExtensionHostRuntimeSpec>,
    ) -> Result<(), String> {
        self.extension_host_runtimes
            .reload_with_app_handle(
                specs,
                self.app_handle.clone(),
                self.bus.clone(),
                self.registry.clone(),
                self.command_router.clone(),
                self.service_router.clone(),
                self.sidecar_runtimes.controller(),
                self.diagnostics.clone(),
            )
            .map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(crate) fn start_v3_extension_host(
        &self,
        spec: ExtensionHostRuntimeSpec,
    ) -> Result<(), String> {
        self.extension_host_runtimes
            .start_with_app_handle(
                spec,
                self.app_handle.clone(),
                self.bus.clone(),
                self.registry.clone(),
                self.command_router.clone(),
                self.service_router.clone(),
                self.sidecar_runtimes.controller(),
                self.diagnostics.clone(),
            )
            .map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(crate) fn stop_v3_extension_host(&self, package_id: &str) -> bool {
        self.extension_host_runtimes.stop(package_id)
    }

    #[allow(dead_code)]
    pub(crate) fn reload_v3_sidecars(&self, specs: Vec<SidecarRuntimeSpec>) -> Result<(), String> {
        self.sidecar_runtimes
            .reload_with_app_handle(specs, self.app_handle.clone())
            .map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(crate) fn start_v3_sidecar(&self, spec: SidecarRuntimeSpec) -> Result<(), String> {
        self.sidecar_runtimes
            .start_with_app_handle(spec, self.app_handle.clone())
            .map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(crate) fn stop_v3_sidecar(&self, package_id: &str, sidecar_name: &str) -> bool {
        self.sidecar_runtimes.stop_with_app_handle(
            package_id,
            sidecar_name,
            self.app_handle.clone(),
        )
    }

    pub fn inspect_package_bundle(&self, path: String) -> Result<BundleInspection, String> {
        inspect_bundle_file(Path::new(&path))
    }

    pub fn install_package_from_file(&self, path: String) -> Result<InstallReceipt, String> {
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
        let packages = records
            .values()
            .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
            .map(|record| {
                serde_json::json!({
                    "id": record.manifest.id(),
                    "name": record.manifest.name(),
                    "version": record.manifest.version(),
                    "author": record.manifest.author(),
                    "bakingrlApi": record.manifest.compatibility().and_then(|compatibility| compatibility.runtime_api.as_deref()),
                    "enabled": record.descriptor.enabled,
                    "active": record.descriptor.enabled && record.descriptor.error.is_none(),
                    "dependencies": record.manifest.dependencies_v4(),
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::Value::Array(packages))
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

    pub fn can_package_read_registry(&self, package_id: &str, _key: &str) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        self.require_enabled_record(&records, package_id)?;
        Ok(())
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
            has_custom_page: preferred_settings_webview_id(record).is_some(),
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
        let (label, path, title, width, height) = {
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
            )
        };

        self.open_standalone_page_window(label, path, title, width, height)
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
                "bakingrlApi": "2.2.0",
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
    fn caller_depends_on_requires_declared_dependency() {
        let caller = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.consumer",
            "name": "Consumer",
            "version": "1.0.0",
            "bakingrlApi": "2.2.0",
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
    fn command_exports_require_declared_dependency_and_command() {
        let provider = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.provider",
            "name": "Provider",
            "version": "1.0.0",
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
            "bakingrlApi": "2.2.0",
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
        assert_eq!(descriptor.runtime_api, "2.2.0");
    }

    #[test]
    fn package_webview_runtime_descriptor_rejects_undeclared_webview() {
        let record = package_record(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "bakingrl.webviews",
            "name": "Webviews",
            "version": "1.0.0",
            "bakingrlApi": "2.2.0",
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
    let safe_id: String = page_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!("page-{safe_id}")
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

fn package_webview_route(package_id: &str, webview_id: &str) -> String {
    format!(
        "/plugin-webview/{}/{}",
        encode_route_segment(package_id),
        encode_route_segment(webview_id)
    )
}

fn package_webview_window_label(package_id: &str, webview_id: &str) -> String {
    let safe: String = format!("{package_id}-{webview_id}")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!("plugin-webview-{safe}")
}

fn encode_route_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[tauri::command]
pub fn list_packages(host: State<'_, Arc<PluginHost>>) -> Vec<PackageDescriptor> {
    host.list_packages()
}

#[tauri::command]
pub fn inspect_package_bundle(
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<crate::plugin_package::bundle::BundleInspection, String> {
    host.inspect_package_bundle(path)
}

#[tauri::command]
pub fn install_package_from_file(
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<InstallReceipt, String> {
    host.install_package_from_file(path)
}

#[tauri::command]
pub async fn install_package_from_url(
    host: State<'_, Arc<PluginHost>>,
    url: String,
) -> Result<InstallReceipt, String> {
    host.install_package_from_url(url).await
}

#[tauri::command]
pub async fn prepare_package_from_url(
    host: State<'_, Arc<PluginHost>>,
    url: String,
) -> Result<PreparedPackageInstall, String> {
    host.prepare_package_from_url(url).await
}

#[tauri::command]
pub async fn prepare_package_from_deep_link(
    host: State<'_, Arc<PluginHost>>,
    deep_link: String,
) -> Result<PreparedPackageInstall, String> {
    host.prepare_package_from_deep_link(deep_link).await
}

#[tauri::command]
pub async fn prepare_package_from_git(
    host: State<'_, Arc<PluginHost>>,
    repo: String,
    rev: Option<String>,
) -> Result<PreparedPackageInstall, String> {
    host.prepare_package_from_git(repo, rev).await
}

#[tauri::command]
pub fn install_prepared_package(
    host: State<'_, Arc<PluginHost>>,
    path: String,
    source: String,
) -> Result<InstallReceipt, String> {
    host.install_prepared_package(path, source)
}

#[tauri::command]
pub fn discard_prepared_package(
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<(), String> {
    host.discard_prepared_package(path)
}

#[tauri::command]
pub fn packages_dir(host: State<'_, Arc<PluginHost>>) -> String {
    host.packages_dir()
}

#[tauri::command]
pub fn get_runtime_info() -> RuntimeInfo {
    runtime_info()
}

#[tauri::command]
pub fn list_plugin_diagnostics(host: State<'_, Arc<PluginHost>>) -> Vec<PluginDiagnosticEvent> {
    host.list_diagnostics()
}

#[tauri::command]
pub fn clear_plugin_diagnostics(host: State<'_, Arc<PluginHost>>) {
    host.clear_diagnostics();
}

#[tauri::command]
pub async fn reload_packages(
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PackageDescriptor>, String> {
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.reload_packages())
        .await
        .map_err(|err| format!("Unable to reload packages: {err}"))
}

#[tauri::command]
pub fn set_package_enabled(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    enabled: bool,
) -> Result<Vec<PackageDescriptor>, String> {
    host.set_package_enabled(package_id, enabled)
}

#[tauri::command]
pub fn remove_package(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<Vec<PackageDescriptor>, String> {
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
    let expected_label = package_webview_window_label(&package_id, &webview_id);
    if window.label() != expected_label {
        return Err(format!(
            "Window '{}' cannot read module files for webview '{}/{}'.",
            window.label(),
            package_id,
            webview_id
        ));
    }
    host.read_package_webview_module_text(&package_id, &webview_id, &relative_path)
}

#[tauri::command]
pub fn get_package_webview_runtime_descriptor(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
) -> Result<PackageWebviewRuntimeDescriptor, String> {
    host.package_webview_runtime_descriptor(&package_id, &webview_id)
}

#[tauri::command]
pub async fn call_service_export(
    host: State<'_, Arc<PluginHost>>,
    caller_package_id: String,
    service_ref: String,
    method: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    host.call_service_export(&caller_package_id, &service_ref, &method, input)
        .await
}

#[tauri::command]
pub fn plugin_registry_get(
    host: State<'_, Arc<PluginHost>>,
    registry: State<'_, Arc<Registry>>,
    package_id: String,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    host.can_package_read_registry(&package_id, &key)?;
    Ok(registry.get(&key))
}

#[tauri::command]
pub fn get_app_settings(host: State<'_, Arc<PluginHost>>) -> AppSettings {
    host.get_app_settings()
}

#[tauri::command]
pub fn save_app_settings(
    host: State<'_, Arc<PluginHost>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    host.save_app_settings(settings)
}

#[tauri::command]
pub fn get_package_settings(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<serde_json::Value, String> {
    host.get_package_settings(&package_id)
}

#[tauri::command]
pub fn save_package_settings(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    values: serde_json::Value,
) -> Result<serde_json::Value, String> {
    host.save_package_settings(package_id, values)
}

#[tauri::command]
pub fn get_package_configuration_state(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<PackageConfigurationState, String> {
    host.get_package_configuration_state(package_id)
}

#[tauri::command]
pub fn set_package_secret(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    key: String,
    value: String,
) -> Result<PackageConfigurationState, String> {
    host.set_package_secret(package_id, key, value)
}

#[tauri::command]
pub fn delete_package_secret(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    key: String,
) -> Result<PackageConfigurationState, String> {
    host.delete_package_secret(package_id, key)
}

#[tauri::command]
pub fn open_package_webview(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    webview_id: String,
) -> Result<(), String> {
    host.open_package_webview(package_id, webview_id)
}

#[tauri::command]
pub fn open_package_configuration(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<(), String> {
    host.open_package_configuration(package_id)
}

#[tauri::command]
pub fn open_package_secrets(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<(), String> {
    host.open_package_secrets(package_id)
}
