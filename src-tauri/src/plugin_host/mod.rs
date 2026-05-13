mod connector_runtime;
mod descriptors;
mod json_store;
mod layout_documents;
mod package_files;
mod runtime_module_loader;
mod runtime_specs;
mod service_runtime;
mod settings_contract;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder,
};
use tracing::{info, warn};

use crate::bus::EventBus;
use crate::models::{
    AppSettings, OverlayLayout, OverlayLayoutsFile, PackageSettingsFile, PackageStateFile,
    PageLayout, PagesFile,
};
use crate::plugin_v2::bundle::BundleInspection;
use crate::plugin_v2::install::{
    download_bundle_to_file, inspect_bundle_file, install_bundle_from_file,
    parse_install_deep_link, InstallReceipt,
};
use crate::plugin_v2::manifest::PluginPackageManifestV2;
use crate::plugin_v2::manifest::{HOST_RUNTIME_API_RANGE, HOST_RUNTIME_API_VERSION};
use crate::plugin_v2::marketplace::{
    catalog_for_index, developer_allows_key, fetch_verified_marketplace_index,
    find_marketplace_version, read_cached_marketplace_index, verified_developer_for_key,
    write_marketplace_cache, MarketplaceApprovedVersion, MarketplaceCatalog, MarketplaceIndex,
    OFFICIAL_MARKETPLACE_URL,
};
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;
use crate::registry::Registry;
use connector_runtime::ConnectorRuntimeManager;
use descriptors::{apply_graph_diagnostics, compatibility_for_manifest, descriptor_for_manifest};
pub use descriptors::{
    ComponentExportSource, PackageDescriptor, PackageExportsDescriptor, PackageImportsDescriptor,
    PackageStatus, PreparedPackageInstall,
};
use json_store::{read_json_or_default, write_json};
use layout_documents::{
    default_overlay_layouts, ensure_active_layout_ids, new_overlay_layout, new_page_layout,
    normalize_overlay_layout, normalize_overlay_layouts, normalize_page, normalize_pages,
    rekey_overlay_layout_contents,
};
use package_files::{
    find_first_bundle, format_command_error, is_remote_package_source, parse_export_ref,
    read_binary_package_file, read_json_package_file, read_package_file,
    safe_installed_package_dir, safe_package_relative_path,
};
use runtime_specs::{connector_specs_for_records, service_specs_for_records};
use service_runtime::ServiceRuntimeManager;
use settings_contract::{
    delete_package_secret as delete_keychain_package_secret, merge_package_settings,
    merge_package_settings_with_schema, package_secret_configured, read_package_settings_schema,
    sanitize_package_settings_values, secret_definitions, secret_store_status,
    set_package_secret_configured, write_package_secret,
};
pub use settings_contract::{PackageConfigurationState, PackageSecretDescriptor};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub runtime_api_version: String,
    pub supported_runtime_api: String,
}

#[derive(Debug)]
struct PackageRecord {
    descriptor: PackageDescriptor,
    manifest: PluginPackageManifestV2,
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
    overlay_layouts_path: PathBuf,
    pages_path: PathBuf,
    app_settings_path: PathBuf,
    package_settings_path: PathBuf,
    marketplace_cache_path: PathBuf,
    records: Mutex<HashMap<String, PackageRecord>>,
    deleting_packages: Mutex<HashMap<String, PendingPackageDeletion>>,
    service_runtimes: ServiceRuntimeManager,
    connector_runtimes: ConnectorRuntimeManager,
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
            overlay_layouts_path: app_data.join("overlay_layouts.json"),
            pages_path: app_data.join("pages.json"),
            app_settings_path: app_data.join("app_settings.json"),
            package_settings_path: app_data.join("package_settings.json"),
            marketplace_cache_path: app_data.join("marketplace_cache.json"),
            records: Mutex::new(HashMap::new()),
            deleting_packages: Mutex::new(HashMap::new()),
            service_runtimes: ServiceRuntimeManager::default(),
            connector_runtimes: ConnectorRuntimeManager::default(),
        })
    }

    pub fn initialize(&self) {
        self.ensure_app_settings();
        self.ensure_overlay_layouts();
        self.ensure_pages();
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
        self.apply_package_statuses(&mut packages);
        packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        packages
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
                        Err(err) => warn!("Package {:?} ignored: {}", path, err),
                    }
                }
            }
            Err(err) => warn!("Unable to read package directory: {}", err),
        }

        apply_graph_diagnostics(&mut records);
        let runtime_isolation = self.load_app_settings().security.plugin_runtime_isolation;
        let package_settings = self.load_package_settings();
        let service_specs =
            service_specs_for_records(&records, &runtime_isolation, &package_settings);
        self.service_runtimes.reload_with_app_handle(
            service_specs,
            self.bus.clone(),
            self.registry.clone(),
            self.app_handle.clone(),
        );
        let connector_specs = connector_specs_for_records(
            &records,
            &runtime_isolation,
            &package_settings,
            &self.package_settings_path,
        );
        self.connector_runtimes.reload_with_app_handle(
            connector_specs,
            self.bus.clone(),
            self.registry.clone(),
            self.service_runtimes.router(),
            self.app_handle.clone(),
        );
        *self.records.lock().unwrap() = records;
        let packages = self.list_packages();
        self.emit_packages_changed(&packages);
        packages
    }

    fn reload_runtimes_from_current_records(&self) {
        let runtime_isolation = self.load_app_settings().security.plugin_runtime_isolation;
        let package_settings = self.load_package_settings();
        let (service_specs, connector_specs) = {
            let records = self.records.lock().unwrap();
            (
                service_specs_for_records(&records, &runtime_isolation, &package_settings),
                connector_specs_for_records(
                    &records,
                    &runtime_isolation,
                    &package_settings,
                    &self.package_settings_path,
                ),
            )
        };
        self.service_runtimes.reload_with_app_handle(
            service_specs,
            self.bus.clone(),
            self.registry.clone(),
            self.app_handle.clone(),
        );
        self.connector_runtimes.reload_with_app_handle(
            connector_specs,
            self.bus.clone(),
            self.registry.clone(),
            self.service_runtimes.router(),
            self.app_handle.clone(),
        );
    }

    pub fn inspect_package_bundle(&self, path: String) -> Result<BundleInspection, String> {
        inspect_bundle_file(Path::new(&path))
            .map(|inspection| self.with_verified_developer_from_cache(inspection))
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
        let mut inspection = match inspect_bundle_file(&download_path) {
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
        self.attach_verified_developer_from_cache(&mut inspection);
        Ok(PreparedPackageInstall {
            path: download_path.to_string_lossy().to_string(),
            source,
            inspection,
        })
    }

    pub async fn refresh_marketplace(&self) -> Result<MarketplaceCatalog, String> {
        let index = fetch_verified_marketplace_index(OFFICIAL_MARKETPLACE_URL).await?;
        write_marketplace_cache(&self.marketplace_cache_path, &index)?;
        Ok(catalog_for_index(index).await)
    }

    pub async fn get_marketplace_catalog(&self) -> Result<MarketplaceCatalog, String> {
        let index = match read_cached_marketplace_index(&self.marketplace_cache_path) {
            Ok(index) => index,
            Err(_) => {
                let index = fetch_verified_marketplace_index(OFFICIAL_MARKETPLACE_URL).await?;
                write_marketplace_cache(&self.marketplace_cache_path, &index)?;
                index
            }
        };
        Ok(catalog_for_index(index).await)
    }

    pub async fn prepare_marketplace_package(
        &self,
        package_id: String,
        version: String,
    ) -> Result<PreparedPackageInstall, String> {
        let package_id = package_id.trim();
        let version = version.trim();
        if package_id.is_empty() || version.is_empty() {
            return Err("Marketplace package id and version are required.".to_string());
        }
        let index = fetch_verified_marketplace_index(OFFICIAL_MARKETPLACE_URL)
            .await
            .map_err(|e| format!("Unable to refresh marketplace index: {e}"))?;
        write_marketplace_cache(&self.marketplace_cache_path, &index)?;
        let (package, approved_version) = find_marketplace_version(&index, package_id, version)?;
        if !developer_allows_key(
            &index,
            &package.developer_id,
            &approved_version.signature_public_key,
        ) {
            return Err("Marketplace developer does not own the approved signing key.".to_string());
        }
        let developer_id = package.developer_id.clone();
        let approved_version = approved_version.clone();

        let download_path = self
            .downloads_dir()
            .join(format!("prepared-marketplace-{}.brlp", unique_id("bundle")));
        download_bundle_to_file(&approved_version.bundle_url, &download_path).await?;
        let mut inspection = match inspect_bundle_file(&download_path) {
            Ok(inspection) => inspection,
            Err(err) => {
                let _ = fs::remove_file(&download_path);
                return Err(err);
            }
        };
        self.validate_marketplace_bundle(
            &inspection,
            package_id,
            version,
            &developer_id,
            &approved_version,
        )
        .map_err(|err| {
            let _ = fs::remove_file(&download_path);
            err
        })?;
        self.attach_verified_developer_from_index(&mut inspection, &index);
        Ok(PreparedPackageInstall {
            path: download_path.to_string_lossy().to_string(),
            source: format!("marketplace:official:{package_id}@{version}"),
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
        let mut inspection = match inspect_bundle_file(&download_path) {
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
        self.attach_verified_developer_from_cache(&mut inspection);
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
        {
            let mut records = self.records.lock().unwrap();
            if let Some(record) = records.get_mut(&package_id) {
                record.descriptor.enabled = enabled;
            }
        }
        let packages = self.list_packages();
        self.emit_packages_changed(&packages);
        Ok(packages)
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

    pub fn read_visual_export_source(
        &self,
        package_id: &str,
        export_name: &str,
    ) -> Result<String, String> {
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
        let export = record
            .manifest
            .exports
            .visuals
            .get(export_name)
            .ok_or_else(|| format!("Visual export '{package_id}/{export_name}' does not exist."))?;
        fs::read_to_string(Path::new(&record.descriptor.path).join(&export.entry))
            .map_err(|e| format!("Unable to read visual export source: {e}"))
    }

    pub fn get_visual_settings_schema(
        &self,
        package_id: &str,
        export_name: &str,
    ) -> Result<Option<serde_json::Value>, String> {
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
        let export = record
            .manifest
            .exports
            .visuals
            .get(export_name)
            .ok_or_else(|| format!("Visual export '{package_id}/{export_name}' does not exist."))?;
        let Some(settings_path) = export.settings.as_deref() else {
            return Ok(None);
        };
        read_json_package_file(Path::new(&record.descriptor.path), settings_path).map(Some)
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

    pub fn read_component_export_source(
        &self,
        caller_package_id: &str,
        component_ref: &str,
    ) -> Result<ComponentExportSource, String> {
        let (provider_package_id, export_name) = parse_export_ref(component_ref)?;
        let records = self.records.lock().unwrap();
        let caller = self.require_enabled_record(&records, caller_package_id)?;
        if provider_package_id != caller_package_id
            && !caller
                .manifest
                .imports
                .components
                .iter()
                .any(|import| import == component_ref)
        {
            return Err(format!(
                "Package '{caller_package_id}' did not declare component import '{component_ref}'."
            ));
        }
        let provider = self.require_enabled_record(&records, provider_package_id)?;
        let export = provider
            .manifest
            .exports
            .components
            .get(export_name)
            .ok_or_else(|| format!("Component export '{component_ref}' does not exist."))?;
        let package_root = Path::new(&provider.descriptor.path);
        let source = read_package_file(package_root, &export.entry)
            .map_err(|e| format!("Unable to read component export source: {e}"))?;
        let props_schema = export
            .props
            .as_deref()
            .map(|path| read_json_package_file(package_root, path))
            .transpose()?;
        Ok(ComponentExportSource {
            package_id: provider_package_id.to_string(),
            export_name: export_name.to_string(),
            entry: export.entry.clone(),
            source,
            props_schema,
        })
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
        if provider_package_id != caller_package_id
            && !caller
                .manifest
                .imports
                .services
                .iter()
                .any(|import| import == service_ref)
        {
            return Err(format!(
                "Package '{caller_package_id}' did not declare service import '{service_ref}'."
            ));
        }
        let provider = self.require_enabled_record(&records, provider_package_id)?;
        let export = provider
            .manifest
            .exports
            .services
            .get(export_name)
            .ok_or_else(|| format!("Service export '{service_ref}' does not exist."))?;
        if !export.methods.iter().any(|allowed| allowed == method) {
            return Err(format!(
                "Service export '{service_ref}' does not expose method '{method}'."
            ));
        }
        Ok(())
    }

    pub async fn call_service_export(
        &self,
        caller_package_id: &str,
        service_ref: &str,
        method: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.validate_service_call(caller_package_id, service_ref, method)?;
        self.service_runtimes
            .call(service_ref, method.to_string(), input)
            .await
    }

    pub fn can_package_read_registry(&self, package_id: &str, key: &str) -> Result<(), String> {
        let records = self.records.lock().unwrap();
        let record = self.require_enabled_record(&records, package_id)?;
        if record
            .descriptor
            .effective_permissions
            .can_read_registry(key)
        {
            Ok(())
        } else {
            Err(format!(
                "Package '{package_id}' cannot read registry key '{key}'."
            ))
        }
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
        let mut settings = settings;
        if settings.obs.access_token.trim().is_empty() {
            settings.obs.access_token = generate_access_token();
        }
        let previous_settings = self.load_app_settings();
        let raw = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize app settings: {e}"))?;
        if previous_settings.behavior.launch_at_startup != settings.behavior.launch_at_startup {
            self.apply_launch_at_startup_setting(settings.behavior.launch_at_startup)?;
        }
        fs::write(&self.app_settings_path, raw)
            .map_err(|e| format!("Failed to write app settings: {e}"))?;
        self.apply_overlay_window_settings(&settings);
        if previous_settings.security.plugin_runtime_isolation
            != settings.security.plugin_runtime_isolation
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

    pub fn apply_overlay_window_settings(&self, settings: &AppSettings) {
        let Some(window) = self.app_handle.get_webview_window("overlay-ingame") else {
            return;
        };

        if settings.overlay.use_monitor_size {
            let selected_monitor = settings
                .overlay
                .monitor_id
                .as_deref()
                .and_then(|monitor_id| {
                    window
                        .available_monitors()
                        .ok()?
                        .into_iter()
                        .find(|monitor| monitor_matches_setting(monitor, monitor_id))
                });
            let Some(monitor) = selected_monitor
                .or_else(|| window.current_monitor().ok().flatten())
                .or_else(|| window.primary_monitor().ok().flatten())
            else {
                return;
            };
            let position = monitor.position();
            let size = monitor.size();
            let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
            let _ = window.set_size(PhysicalSize::new(size.width, size.height));
            return;
        }

        let _ = window.set_size(PhysicalSize::new(
            settings.overlay.screen_width.max(1),
            settings.overlay.screen_height.max(1),
        ));
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
        let title = record
            .manifest
            .exports
            .configuration
            .as_ref()
            .and_then(|configuration| configuration.title.clone())
            .or_else(|| {
                schema
                    .as_ref()
                    .and_then(|schema| schema.get("title"))
                    .and_then(|title| title.as_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| format!("{} Settings", record.descriptor.name));
        Ok(PackageConfigurationState {
            package_id,
            title,
            has_custom_page: record.manifest.exports.configuration.is_some(),
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

    pub fn get_overlay_layouts(&self) -> OverlayLayoutsFile {
        self.load_overlay_layouts()
    }

    pub fn get_package_configuration_page(&self, package_id: String) -> Result<PageLayout, String> {
        let records = self.records.lock().unwrap();
        let record = self.require_enabled_record(&records, &package_id)?;
        let configuration = record
            .manifest
            .exports
            .configuration
            .as_ref()
            .ok_or_else(|| {
                format!("Package '{package_id}' does not expose a configuration page.")
            })?;
        let package_root = Path::new(&record.descriptor.path);
        let raw = read_json_package_file(package_root, &configuration.path)?;
        let mut page: PageLayout = serde_json::from_value(raw).map_err(|e| {
            format!("Configuration page for package '{package_id}' is invalid: {e}")
        })?;
        page.id = format!("configuration-{package_id}");
        page.name = configuration
            .title
            .clone()
            .unwrap_or_else(|| format!("{} Configuration", record.descriptor.name));
        page.width = 1200.0;
        page.height = 740.0;
        normalize_page(&mut page, false);
        Ok(page)
    }

    pub fn save_overlay_layout(&self, layout: OverlayLayout) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let mut layout = layout;
        normalize_overlay_layout(&mut layout, true);
        match file.layouts.iter_mut().find(|entry| entry.id == layout.id) {
            Some(existing) => *existing = layout,
            None => file.layouts.push(layout),
        }
        normalize_overlay_layouts(&mut file);
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn create_overlay_layout(
        &self,
        name: String,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let id = unique_id("overlay");
        let now = now_ms();
        file.layouts
            .push(new_overlay_layout(id.clone(), name, width, height, now));
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn duplicate_overlay_layout(
        &self,
        layout_id: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let source = file
            .layouts
            .iter()
            .find(|layout| layout.id == layout_id)
            .cloned()
            .ok_or_else(|| format!("Layout '{layout_id}' does not exist."))?;
        let new_id = unique_id("overlay");
        let mut duplicated = source;
        duplicated.id = new_id.clone();
        duplicated.name = format!("{} Copy", duplicated.name);
        duplicated.template_source = None;
        let now = now_ms();
        duplicated.created_at_ms = now;
        duplicated.updated_at_ms = now;
        rekey_overlay_layout_contents(&mut duplicated);
        duplicated.items.clear();
        file.layouts.push(duplicated);
        file.active_layout_id = new_id;
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn set_active_overlay_layout(
        &self,
        layout_id: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if !file.layouts.iter().any(|layout| layout.id == layout_id) {
            return Err(format!("Layout '{layout_id}' does not exist."));
        }
        file.active_layout_id = layout_id;
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn set_stream_overlay_layout(
        &self,
        layout_id: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if !file.layouts.iter().any(|layout| layout.id == layout_id) {
            return Err(format!("Layout '{layout_id}' does not exist."));
        }
        file.stream_layout_id = layout_id;
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn delete_overlay_layout(&self, layout_id: String) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if file.layouts.len() <= 1 {
            return Err("At least one layout is required.".to_string());
        }
        file.layouts.retain(|layout| layout.id != layout_id);
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn import_package_layout(
        &self,
        package_id: String,
        export_name: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let mut layout = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, &package_id)?;
            let export = record
                .manifest
                .exports
                .layouts
                .get(&export_name)
                .ok_or_else(|| {
                    format!("Layout template '{package_id}/{export_name}' does not exist.")
                })?;
            let package_root = Path::new(&record.descriptor.path);
            let raw = read_json_package_file(package_root, &export.path)?;
            let mut layout: OverlayLayout = serde_json::from_value(raw).map_err(|e| {
                format!("Layout template '{package_id}/{export_name}' is invalid: {e}")
            })?;
            if layout.name.trim().is_empty() {
                layout.name = export
                    .title
                    .clone()
                    .unwrap_or_else(|| export_name.replace('-', " "));
            }
            layout.template_source = Some(format!("{package_id}/{export_name}"));
            layout
        };
        let layout_id = unique_id("overlay");
        let now = now_ms();
        layout.id = layout_id.clone();
        layout.created_at_ms = now;
        layout.updated_at_ms = now;
        normalize_overlay_layout(&mut layout, false);
        rekey_overlay_layout_contents(&mut layout);
        file.layouts.push(layout);
        file.active_layout_id = layout_id;
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlay_layouts_changed(&file);
        Ok(file)
    }

    pub fn get_pages(&self) -> PagesFile {
        self.load_pages()
    }

    pub fn save_page(&self, page: PageLayout) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        let mut page = page;
        normalize_page(&mut page, true);
        match file.pages.iter_mut().find(|entry| entry.id == page.id) {
            Some(existing) => *existing = page,
            None => file.pages.push(page),
        }
        normalize_pages(&mut file);
        self.save_pages(&file)?;
        self.emit_pages_changed(&file);
        Ok(file)
    }

    pub fn create_page(
        &self,
        name: String,
        open_target: Option<String>,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        let now = now_ms();
        let mut page = new_page_layout(unique_id("page"), name, open_target, width, height, now);
        normalize_page(&mut page, false);
        file.pages.push(page);
        self.save_pages(&file)?;
        self.emit_pages_changed(&file);
        Ok(file)
    }

    pub fn duplicate_page(&self, page_id: String) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        let source = file
            .pages
            .iter()
            .find(|page| page.id == page_id)
            .cloned()
            .ok_or_else(|| format!("Page '{page_id}' does not exist."))?;
        let now = now_ms();
        let mut duplicated = source;
        duplicated.id = unique_id("page");
        duplicated.name = format!("{} Copy", duplicated.name);
        duplicated.favorite = false;
        duplicated.created_at_ms = now;
        duplicated.updated_at_ms = now;
        duplicated.template_source = None;
        for layer in &mut duplicated.layers {
            layer.id = unique_id("layer");
            for item in &mut layer.items {
                item.id = unique_id("item");
            }
        }
        normalize_page(&mut duplicated, false);
        file.pages.push(duplicated);
        self.save_pages(&file)?;
        self.emit_pages_changed(&file);
        Ok(file)
    }

    pub fn delete_page(&self, page_id: String) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        file.pages.retain(|page| page.id != page_id);
        self.save_pages(&file)?;
        self.emit_pages_changed(&file);
        Ok(file)
    }

    pub fn import_package_page(
        &self,
        package_id: String,
        export_name: String,
    ) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        let mut page = {
            let records = self.records.lock().unwrap();
            let record = self.require_enabled_record(&records, &package_id)?;
            let export = record
                .manifest
                .exports
                .pages
                .get(&export_name)
                .ok_or_else(|| {
                    format!("Page template '{package_id}/{export_name}' does not exist.")
                })?;
            let package_root = Path::new(&record.descriptor.path);
            let raw = read_json_package_file(package_root, &export.path)?;
            let mut page: PageLayout = serde_json::from_value(raw).map_err(|e| {
                format!("Page template '{package_id}/{export_name}' is invalid: {e}")
            })?;
            if page.name.trim().is_empty() {
                page.name = export
                    .title
                    .clone()
                    .unwrap_or_else(|| export_name.replace('-', " "));
            }
            page.template_source = Some(format!("{package_id}/{export_name}"));
            page
        };
        let now = now_ms();
        page.id = unique_id("page");
        page.favorite = false;
        page.created_at_ms = now;
        page.updated_at_ms = now;
        normalize_page(&mut page, false);
        file.pages.push(page);
        self.save_pages(&file)?;
        self.emit_pages_changed(&file);
        Ok(file)
    }

    pub fn open_page(&self, page_id: String) -> Result<(), String> {
        let pages = self.load_pages();
        let page = pages
            .pages
            .iter()
            .find(|page| page.id == page_id)
            .ok_or_else(|| format!("Page '{page_id}' does not exist."))?;
        let path = format!("/page/{page_id}");
        let js_path = serde_json::to_string(&path).map_err(|error| error.to_string())?;

        if page.settings.open_target == "window" {
            let label = page_window_label(&page_id);
            return self.open_standalone_page_window(
                label,
                path,
                format!("BakingRL - {}", page.name),
                page.width,
                page.height,
            );
        }

        let main_window = self
            .app_handle
            .get_webview_window("main")
            .ok_or_else(|| "Main window is not available.".to_string())?;
        main_window
            .eval(format!("window.location.href = {js_path};"))
            .map_err(|error| error.to_string())?;
        let _ = main_window.show();
        let _ = main_window.set_focus();
        Ok(())
    }

    pub fn open_package_configuration(&self, package_id: String) -> Result<(), String> {
        let title = {
            let records = self.records.lock().unwrap();
            let record = records
                .get(&package_id)
                .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
            if record.manifest.exports.configuration.is_none()
                && !record.descriptor.has_public_settings
            {
                return Err(format!(
                    "Package '{package_id}' does not expose configuration settings."
                ));
            }
            record
                .manifest
                .exports
                .configuration
                .as_ref()
                .and_then(|configuration| configuration.title.clone())
                .unwrap_or_else(|| format!("{} Settings", record.descriptor.name))
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
        let manifest: PluginPackageManifestV2 = serde_json::from_str(&manifest_str)
            .map_err(|e| format!("bakingrl.plugin.json invalid: {e}"))?;
        manifest.validate()?;
        let effective_permissions = EffectivePackagePermissionsV2::for_manifest(&manifest)?;
        let enabled = state.enabled.get(&manifest.id).copied().unwrap_or(false);
        let descriptor = descriptor_for_manifest(
            &manifest,
            package_dir.to_string_lossy().to_string(),
            enabled,
            effective_permissions,
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

    fn with_verified_developer_from_cache(
        &self,
        mut inspection: BundleInspection,
    ) -> BundleInspection {
        self.attach_verified_developer_from_cache(&mut inspection);
        inspection
    }

    fn attach_verified_developer_from_cache(&self, inspection: &mut BundleInspection) {
        let Ok(index) = read_cached_marketplace_index(&self.marketplace_cache_path) else {
            return;
        };
        self.attach_verified_developer_from_index(inspection, &index);
    }

    fn attach_verified_developer_from_index(
        &self,
        inspection: &mut BundleInspection,
        index: &MarketplaceIndex,
    ) {
        if !inspection.signature_verified {
            return;
        }
        let Some(public_key) = inspection.signature_public_key.as_deref() else {
            return;
        };
        inspection.verified_developer = verified_developer_for_key(index, public_key);
    }

    fn load_app_settings(&self) -> AppSettings {
        read_json_or_default(&self.app_settings_path)
    }

    fn ensure_app_settings(&self) {
        let mut settings = self.load_app_settings();
        if settings.obs.access_token.trim().is_empty() {
            settings.obs.access_token = generate_access_token();
            let _ = self.save_app_settings(settings);
        }
    }

    fn load_package_settings(&self) -> PackageSettingsFile {
        read_json_or_default(&self.package_settings_path)
    }

    fn save_package_settings_file(&self, file: &PackageSettingsFile) -> Result<(), String> {
        write_json(&self.package_settings_path, file)
    }

    fn ensure_overlay_layouts(&self) {
        if !self.overlay_layouts_path.exists() {
            let _ = self.save_overlay_layouts(&default_overlay_layouts());
        } else {
            let mut file = self.load_overlay_layouts();
            normalize_overlay_layouts(&mut file);
            let _ = self.save_overlay_layouts(&file);
        }
    }

    fn validate_install_trust(&self, bundle_path: &Path, source: &str) -> Result<(), String> {
        if let Some((package_id, version)) = parse_marketplace_source(source) {
            let inspection = inspect_bundle_file(bundle_path)?;
            let index = read_cached_marketplace_index(&self.marketplace_cache_path)?;
            let (package, approved_version) =
                find_marketplace_version(&index, package_id, version)?;
            if !developer_allows_key(
                &index,
                &package.developer_id,
                &approved_version.signature_public_key,
            ) {
                return Err(
                    "Marketplace developer does not own the approved signing key.".to_string(),
                );
            }
            return self.validate_marketplace_bundle(
                &inspection,
                package_id,
                version,
                &package.developer_id,
                approved_version,
            );
        }
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

    fn validate_marketplace_bundle(
        &self,
        inspection: &BundleInspection,
        package_id: &str,
        version: &str,
        developer_id: &str,
        approved_version: &MarketplaceApprovedVersion,
    ) -> Result<(), String> {
        if inspection.manifest.id != package_id {
            return Err(
                "Downloaded marketplace bundle package id does not match the approved entry."
                    .to_string(),
            );
        }
        if inspection.manifest.version != version {
            return Err(
                "Downloaded marketplace bundle version does not match the approved entry."
                    .to_string(),
            );
        }
        if !inspection
            .sha256
            .eq_ignore_ascii_case(&approved_version.bundle_sha256)
        {
            return Err(
                "Downloaded marketplace bundle SHA-256 does not match the approved entry."
                    .to_string(),
            );
        }
        if !inspection.signature_verified {
            return Err(
                "Marketplace packages require a verified Ed25519 bundle signature.".to_string(),
            );
        }
        if inspection.signature_public_key.as_deref()
            != Some(approved_version.signature_public_key.as_str())
        {
            return Err(
                "Marketplace bundle signature key does not match the approved entry.".to_string(),
            );
        }
        let effective_permissions =
            EffectivePackagePermissionsV2::for_manifest(&inspection.manifest)?;
        if effective_permissions != approved_version.review.permissions {
            return Err(
                "Marketplace bundle permissions do not match the reviewed permissions.".to_string(),
            );
        }
        if inspection
            .manifest
            .author
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            warn!(
                "Marketplace package {}@{} from developer {} has no manifest author",
                package_id, version, developer_id
            );
        }
        Ok(())
    }

    fn load_overlay_layouts(&self) -> OverlayLayoutsFile {
        let mut file = fs::read_to_string(&self.overlay_layouts_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(default_overlay_layouts);
        normalize_overlay_layouts(&mut file);
        file
    }

    fn save_overlay_layouts(&self, file: &OverlayLayoutsFile) -> Result<(), String> {
        write_json(&self.overlay_layouts_path, file)
    }

    fn ensure_pages(&self) {
        if !self.pages_path.exists() {
            let _ = self.save_pages(&PagesFile::default());
        } else {
            let mut file = self.load_pages();
            normalize_pages(&mut file);
            let _ = self.save_pages(&file);
        }
    }

    fn load_pages(&self) -> PagesFile {
        let mut file = fs::read_to_string(&self.pages_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        normalize_pages(&mut file);
        file
    }

    fn save_pages(&self, file: &PagesFile) -> Result<(), String> {
        write_json(&self.pages_path, file)
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
        let payload = serde_json::json!({
            "kind": "package",
            "source": package_id,
            "message": message,
            "timestamp_ms": timestamp_ms
        });
        let _ = self.app_handle.emit("bakingrl-runtime-error", payload);
    }

    fn emit_package_settings_changed(&self, package_id: &str) {
        let _ = self
            .app_handle
            .emit("bakingrl-package-settings-changed", package_id);
    }

    fn emit_overlay_layouts_changed(&self, file: &OverlayLayoutsFile) {
        let _ = self
            .app_handle
            .emit("bakingrl-overlay-layouts-changed", file);
    }

    fn emit_pages_changed(&self, file: &PagesFile) {
        let _ = self.app_handle.emit("bakingrl-pages-changed", file);
    }
}

pub fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        runtime_api_version: HOST_RUNTIME_API_VERSION.to_string(),
        supported_runtime_api: HOST_RUNTIME_API_RANGE.to_string(),
    }
}

fn monitor_id(monitor: &Monitor) -> String {
    if let Some(name) = monitor.name() {
        if !name.trim().is_empty() {
            return format!("name:{name}");
        }
    }

    let position = monitor.position();
    let size = monitor.size();
    format!(
        "rect:{}:{}:{}:{}",
        position.x, position.y, size.width, size.height
    )
}

fn monitor_matches_setting(monitor: &Monitor, setting: &str) -> bool {
    monitor_id(monitor) == setting || monitor.name().is_some_and(|name| name == setting)
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

fn parse_marketplace_source(source: &str) -> Option<(&str, &str)> {
    let value = source.strip_prefix("marketplace:official:")?;
    let (package_id, version) = value.rsplit_once('@')?;
    if package_id.is_empty() || version.is_empty() {
        None
    } else {
        Some((package_id, version))
    }
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

fn generate_access_token() -> String {
    let mut bytes = [0_u8; 32];
    if getrandom::fill(&mut bytes).is_ok() {
        return hex::encode(bytes);
    }
    format!("fallback-{}", unique_id("obs"))
}

#[tauri::command]
pub fn list_packages(host: State<'_, Arc<PluginHost>>) -> Vec<PackageDescriptor> {
    host.list_packages()
}

#[tauri::command]
pub fn inspect_package_bundle(
    host: State<'_, Arc<PluginHost>>,
    path: String,
) -> Result<crate::plugin_v2::bundle::BundleInspection, String> {
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
pub async fn get_marketplace_catalog(
    host: State<'_, Arc<PluginHost>>,
) -> Result<MarketplaceCatalog, String> {
    host.get_marketplace_catalog().await
}

#[tauri::command]
pub async fn refresh_marketplace(
    host: State<'_, Arc<PluginHost>>,
) -> Result<MarketplaceCatalog, String> {
    host.refresh_marketplace().await
}

#[tauri::command]
pub async fn prepare_marketplace_package(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    version: String,
) -> Result<PreparedPackageInstall, String> {
    host.prepare_marketplace_package(package_id, version).await
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
    let packages = host.set_package_enabled(package_id, enabled)?;
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        host.reload_packages();
    });
    Ok(packages)
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
pub fn read_visual_export_source(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    export_name: String,
) -> Result<String, String> {
    host.read_visual_export_source(&package_id, &export_name)
}

#[tauri::command]
pub fn read_component_export_source(
    host: State<'_, Arc<PluginHost>>,
    caller_package_id: String,
    component_ref: String,
) -> Result<ComponentExportSource, String> {
    host.read_component_export_source(&caller_package_id, &component_ref)
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
pub fn get_visual_settings_schema(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    export_name: String,
) -> Result<Option<serde_json::Value>, String> {
    host.get_visual_settings_schema(&package_id, &export_name)
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
pub fn get_overlay_layouts(host: State<'_, Arc<PluginHost>>) -> OverlayLayoutsFile {
    host.get_overlay_layouts()
}

#[tauri::command]
pub fn get_package_configuration_page(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
) -> Result<PageLayout, String> {
    host.get_package_configuration_page(package_id)
}

#[tauri::command]
pub fn save_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    layout: OverlayLayout,
) -> Result<OverlayLayoutsFile, String> {
    host.save_overlay_layout(layout)
}

#[tauri::command]
pub fn create_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    name: String,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<OverlayLayoutsFile, String> {
    host.create_overlay_layout(name, width, height)
}

#[tauri::command]
pub fn duplicate_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    layout_id: String,
) -> Result<OverlayLayoutsFile, String> {
    host.duplicate_overlay_layout(layout_id)
}

#[tauri::command]
pub fn set_active_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    layout_id: String,
) -> Result<OverlayLayoutsFile, String> {
    host.set_active_overlay_layout(layout_id)
}

#[tauri::command]
pub fn set_stream_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    layout_id: String,
) -> Result<OverlayLayoutsFile, String> {
    host.set_stream_overlay_layout(layout_id)
}

#[tauri::command]
pub fn delete_overlay_layout(
    host: State<'_, Arc<PluginHost>>,
    layout_id: String,
) -> Result<OverlayLayoutsFile, String> {
    host.delete_overlay_layout(layout_id)
}

#[tauri::command]
pub fn import_package_layout(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    export_name: String,
) -> Result<OverlayLayoutsFile, String> {
    host.import_package_layout(package_id, export_name)
}

#[tauri::command]
pub fn get_pages(host: State<'_, Arc<PluginHost>>) -> PagesFile {
    host.get_pages()
}

#[tauri::command]
pub fn save_page(host: State<'_, Arc<PluginHost>>, page: PageLayout) -> Result<PagesFile, String> {
    host.save_page(page)
}

#[tauri::command]
pub fn create_page(
    host: State<'_, Arc<PluginHost>>,
    name: String,
    open_target: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<PagesFile, String> {
    host.create_page(name, open_target, width, height)
}

#[tauri::command]
pub fn duplicate_page(
    host: State<'_, Arc<PluginHost>>,
    page_id: String,
) -> Result<PagesFile, String> {
    host.duplicate_page(page_id)
}

#[tauri::command]
pub fn delete_page(host: State<'_, Arc<PluginHost>>, page_id: String) -> Result<PagesFile, String> {
    host.delete_page(page_id)
}

#[tauri::command]
pub fn import_package_page(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    export_name: String,
) -> Result<PagesFile, String> {
    host.import_package_page(package_id, export_name)
}

#[tauri::command]
pub fn open_page(host: State<'_, Arc<PluginHost>>, page_id: String) -> Result<(), String> {
    host.open_page(page_id)
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
