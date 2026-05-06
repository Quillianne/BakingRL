mod connector_runtime;
mod service_runtime;

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
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
    AppSettings, OverlayItem, OverlayLayer, OverlayLayout, OverlayLayoutsFile, PackageSettingsFile,
    PackageStateFile, PageBackground, PageItem, PageLayer, PageLayout, PageSettings, PagesFile,
};
use crate::plugin_v2::bundle::BundleInspection;
use crate::plugin_v2::install::{
    download_bundle_to_file, inspect_bundle_file, install_bundle_from_file,
    parse_install_deep_link, InstallReceipt,
};
use crate::plugin_v2::manifest::PluginPackageManifestV2;
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;
use crate::registry::Registry;
use connector_runtime::{ConnectorRuntimeManager, ConnectorRuntimeSpec};
use service_runtime::{ServiceRuntimeManager, ServiceRuntimeSpec};
use walkdir::WalkDir;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PackageDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub enabled: bool,
    pub path: String,
    pub exports: PackageExportsDescriptor,
    pub imports: PackageImportsDescriptor,
    pub effective_permissions: EffectivePackagePermissionsV2,
    pub settings: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct PackageExportsDescriptor {
    pub visuals: Vec<VisualExportDescriptor>,
    pub components: Vec<NamedExportDescriptor>,
    pub services: Vec<ServiceExportDescriptor>,
    pub connectors: Vec<NamedExportDescriptor>,
    pub assets: Vec<NamedExportDescriptor>,
    pub schemas: Vec<NamedExportDescriptor>,
    pub pages: Vec<PageExportDescriptor>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisualExportDescriptor {
    pub name: String,
    pub entry: String,
    pub default_width: f64,
    pub default_height: f64,
    pub settings: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NamedExportDescriptor {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceExportDescriptor {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageExportDescriptor {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentExportSource {
    pub package_id: String,
    pub export_name: String,
    pub entry: String,
    pub source: String,
    pub props_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreparedPackageInstall {
    pub path: String,
    pub source: String,
    pub inspection: BundleInspection,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct PackageImportsDescriptor {
    pub components: Vec<String>,
    pub services: Vec<String>,
}

#[derive(Debug)]
struct PackageRecord {
    descriptor: PackageDescriptor,
    manifest: PluginPackageManifestV2,
}

pub struct PluginHost {
    app_handle: AppHandle,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    packages_dir: PathBuf,
    state_path: PathBuf,
    overlays_path: PathBuf,
    pages_path: PathBuf,
    app_settings_path: PathBuf,
    package_settings_path: PathBuf,
    records: Mutex<HashMap<String, PackageRecord>>,
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
            overlays_path: app_data.join("overlay_layouts.json"),
            pages_path: app_data.join("pages.json"),
            app_settings_path: app_data.join("app_settings.json"),
            package_settings_path: app_data.join("package_settings.json"),
            records: Mutex::new(HashMap::new()),
            service_runtimes: ServiceRuntimeManager::default(),
            connector_runtimes: ConnectorRuntimeManager::default(),
        })
    }

    pub fn initialize(&self) {
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
        let service_specs = service_specs_for_records(&records);
        self.service_runtimes
            .reload(service_specs, self.bus.clone(), self.registry.clone());
        let connector_specs = connector_specs_for_records(&records);
        self.connector_runtimes.reload(
            connector_specs,
            self.bus.clone(),
            self.registry.clone(),
            self.service_runtimes.router(),
        );
        *self.records.lock().unwrap() = records;
        let packages = self.list_packages();
        self.emit_packages_changed(&packages);
        packages
    }

    pub fn inspect_package_bundle(&self, path: String) -> Result<BundleInspection, String> {
        inspect_bundle_file(Path::new(&path))
    }

    pub fn install_package_from_file(&self, path: String) -> Result<InstallReceipt, String> {
        let receipt = install_bundle_from_file(
            Path::new(&path),
            &self.packages_dir,
            &self.packages_dir.join(".staging"),
            format!("file:{path}"),
        )?;
        self.save_package_enabled(&receipt.package_id, true)?;
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
        let receipt = install_bundle_from_file(
            &bundle_path,
            &self.packages_dir,
            &self.packages_dir.join(".staging"),
            source,
        )?;
        self.save_package_enabled(&receipt.package_id, true)?;
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
        {
            let records = self.records.lock().unwrap();
            if !records.contains_key(&package_id) {
                return Err(format!("Package '{package_id}' is not installed."));
            }
        }

        let mut state = self.load_state();
        state.enabled.insert(package_id, enabled);
        self.save_state(&state)?;
        Ok(self.reload_packages())
    }

    pub fn remove_package(&self, package_id: String) -> Result<Vec<PackageDescriptor>, String> {
        let target = self.packages_dir.join(&package_id);
        if !target.exists() {
            return Err(format!("Package '{package_id}' is not installed."));
        }
        fs::remove_dir_all(&target).map_err(|e| format!("Unable to remove package: {e}"))?;
        let mut state = self.load_state();
        state.enabled.remove(&package_id);
        self.save_state(&state)?;
        Ok(self.reload_packages())
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

    pub fn read_package_file(
        &self,
        package_id: &str,
        relative_path: &str,
    ) -> Result<Vec<u8>, String> {
        let records = self.records.lock().unwrap();
        let record = records
            .get(package_id)
            .ok_or_else(|| format!("Package '{package_id}' is not installed."))?;
        if !record.descriptor.enabled {
            return Err(format!("Package '{package_id}' is disabled."));
        }
        let safe_path = safe_package_relative_path(relative_path)?;
        fs::read(Path::new(&record.descriptor.path).join(safe_path))
            .map_err(|e| format!("Unable to read package file: {e}"))
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
        if !record.descriptor.enabled {
            return Err(format!("Package '{package_id}' is disabled."));
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
        let raw = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize app settings: {e}"))?;
        fs::write(&self.app_settings_path, raw)
            .map_err(|e| format!("Failed to write app settings: {e}"))?;
        self.apply_overlay_window_settings(&settings);
        Ok(settings)
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
        Ok(merge_settings(
            record.descriptor.settings.as_deref(),
            Path::new(&record.descriptor.path),
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
        if !values.is_object() {
            return Err("Package settings must be a JSON object.".to_string());
        }
        {
            let records = self.records.lock().unwrap();
            if !records.contains_key(&package_id) {
                return Err(format!("Package '{package_id}' is not installed."));
            }
        }
        let mut settings = self.load_package_settings();
        settings.values.insert(package_id.clone(), values);
        self.save_package_settings_file(&settings)?;
        self.emit_package_settings_changed(&package_id);
        self.get_package_settings(&package_id)
    }

    pub fn get_overlay_layouts(&self) -> OverlayLayoutsFile {
        self.load_overlay_layouts()
    }

    pub fn save_overlay_layout(&self, layout: OverlayLayout) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let mut layout = layout;
        normalize_overlay_layout(&mut layout);
        match file.layouts.iter_mut().find(|entry| entry.id == layout.id) {
            Some(existing) => *existing = layout,
            None => file.layouts.push(layout),
        }
        normalize_overlay_layouts(&mut file);
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
        Ok(file)
    }

    pub fn create_overlay_layout(&self, name: String) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        let id = unique_id("overlay");
        file.layouts.push(OverlayLayout {
            id: id.clone(),
            name: if name.trim().is_empty() {
                "Untitled Overlay".to_string()
            } else {
                name
            },
            width: 1920.0,
            height: 1080.0,
            layers: default_layers(),
            items: Vec::new(),
        });
        file.active_layout_id = id;
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
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
            .ok_or_else(|| format!("Overlay layout '{layout_id}' does not exist."))?;
        let new_id = unique_id("overlay");
        let mut duplicated = source;
        duplicated.id = new_id.clone();
        duplicated.name = format!("{} Copy", duplicated.name);
        for layer in &mut duplicated.layers {
            layer.id = unique_id("layer");
            for item in &mut layer.items {
                item.id = unique_id("item");
            }
        }
        duplicated.items.clear();
        file.layouts.push(duplicated);
        file.active_layout_id = new_id;
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
        Ok(file)
    }

    pub fn set_active_overlay_layout(
        &self,
        layout_id: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if !file.layouts.iter().any(|layout| layout.id == layout_id) {
            return Err(format!("Overlay layout '{layout_id}' does not exist."));
        }
        file.active_layout_id = layout_id;
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
        Ok(file)
    }

    pub fn set_stream_overlay_layout(
        &self,
        layout_id: String,
    ) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if !file.layouts.iter().any(|layout| layout.id == layout_id) {
            return Err(format!("Overlay layout '{layout_id}' does not exist."));
        }
        file.stream_layout_id = layout_id;
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
        Ok(file)
    }

    pub fn delete_overlay_layout(&self, layout_id: String) -> Result<OverlayLayoutsFile, String> {
        let mut file = self.load_overlay_layouts();
        if file.layouts.len() <= 1 {
            return Err("At least one overlay layout is required.".to_string());
        }
        file.layouts.retain(|layout| layout.id != layout_id);
        ensure_active_layout_ids(&mut file);
        self.save_overlay_layouts(&file)?;
        self.emit_overlays_changed(&file);
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

    pub fn create_page(&self, name: String) -> Result<PagesFile, String> {
        let mut file = self.load_pages();
        let now = now_ms();
        let mut page = PageLayout {
            id: unique_id("page"),
            name: if name.trim().is_empty() {
                "Untitled Page".to_string()
            } else {
                name
            },
            width: 1440.0,
            height: 900.0,
            background: PageBackground::default(),
            settings: PageSettings::default(),
            layers: default_page_layers(),
            created_at_ms: now,
            updated_at_ms: now,
            template_source: None,
        };
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
            .title(format!("BakingRL - {}", page.name))
            .inner_size(page.width.max(480.0), page.height.max(320.0))
            .min_inner_size(480.0, 320.0)
            .resizable(true)
            .visible(true)
            .build()
            .map_err(|error| error.to_string())?;
            let _ = window.set_focus();
            return Ok(());
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

    fn load_app_settings(&self) -> AppSettings {
        read_json_or_default(&self.app_settings_path)
    }

    fn load_package_settings(&self) -> PackageSettingsFile {
        read_json_or_default(&self.package_settings_path)
    }

    fn save_package_settings_file(&self, file: &PackageSettingsFile) -> Result<(), String> {
        write_json(&self.package_settings_path, file)
    }

    fn ensure_overlay_layouts(&self) {
        if !self.overlays_path.exists() {
            let _ = self.save_overlay_layouts(&default_overlay_layouts());
        } else {
            let mut file = self.load_overlay_layouts();
            normalize_overlay_layouts(&mut file);
            let _ = self.save_overlay_layouts(&file);
        }
    }

    fn load_overlay_layouts(&self) -> OverlayLayoutsFile {
        let mut file = fs::read_to_string(&self.overlays_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(default_overlay_layouts);
        normalize_overlay_layouts(&mut file);
        file
    }

    fn save_overlay_layouts(&self, file: &OverlayLayoutsFile) -> Result<(), String> {
        write_json(&self.overlays_path, file)
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

    fn emit_package_settings_changed(&self, package_id: &str) {
        let _ = self
            .app_handle
            .emit("bakingrl-package-settings-changed", package_id);
    }

    fn emit_overlays_changed(&self, file: &OverlayLayoutsFile) {
        let _ = self.app_handle.emit("bakingrl-overlays-changed", file);
    }

    fn emit_pages_changed(&self, file: &PagesFile) {
        let _ = self.app_handle.emit("bakingrl-pages-changed", file);
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

fn descriptor_for_manifest(
    manifest: &PluginPackageManifestV2,
    path: String,
    enabled: bool,
    effective_permissions: EffectivePackagePermissionsV2,
) -> PackageDescriptor {
    PackageDescriptor {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        enabled,
        path,
        exports: PackageExportsDescriptor {
            visuals: manifest
                .exports
                .visuals
                .iter()
                .map(|(name, export)| {
                    let [default_width, default_height] =
                        export.default_size.unwrap_or([320.0, 120.0]);
                    VisualExportDescriptor {
                        name: name.clone(),
                        entry: export.entry.clone(),
                        default_width,
                        default_height,
                        settings: export.settings.clone(),
                    }
                })
                .collect(),
            components: manifest
                .exports
                .components
                .keys()
                .map(|name| NamedExportDescriptor { name: name.clone() })
                .collect(),
            services: manifest
                .exports
                .services
                .iter()
                .map(|(name, export)| ServiceExportDescriptor {
                    name: name.clone(),
                    methods: export.methods.clone(),
                })
                .collect(),
            connectors: manifest
                .exports
                .connectors
                .keys()
                .map(|name| NamedExportDescriptor { name: name.clone() })
                .collect(),
            assets: manifest
                .exports
                .assets
                .keys()
                .map(|name| NamedExportDescriptor { name: name.clone() })
                .collect(),
            schemas: manifest
                .exports
                .schemas
                .keys()
                .map(|name| NamedExportDescriptor { name: name.clone() })
                .collect(),
            pages: manifest
                .exports
                .pages
                .iter()
                .map(|(name, export)| PageExportDescriptor {
                    name: name.clone(),
                    path: export.path.clone(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                })
                .collect(),
        },
        imports: PackageImportsDescriptor {
            components: manifest.imports.components.clone(),
            services: manifest.imports.services.clone(),
        },
        effective_permissions,
        settings: manifest.settings.clone(),
        error: None,
    }
}

fn apply_graph_diagnostics(records: &mut HashMap<String, PackageRecord>) {
    let component_exports: std::collections::HashSet<String> = records
        .values()
        .flat_map(|record| {
            record
                .manifest
                .exports
                .components
                .keys()
                .map(|name| format!("{}/{}", record.manifest.id, name))
                .collect::<Vec<_>>()
        })
        .collect();
    let service_exports: std::collections::HashSet<String> = records
        .values()
        .flat_map(|record| {
            record
                .manifest
                .exports
                .services
                .keys()
                .map(|name| format!("{}/{}", record.manifest.id, name))
                .collect::<Vec<_>>()
        })
        .collect();

    for record in records.values_mut() {
        let mut missing = Vec::new();
        for import in &record.manifest.imports.components {
            if !component_exports.contains(import) {
                missing.push(format!("component {import}"));
            }
        }
        for import in &record.manifest.imports.services {
            if !service_exports.contains(import) {
                missing.push(format!("service {import}"));
            }
        }
        record.descriptor.error = if missing.is_empty() {
            None
        } else {
            Some(format!("Missing imports: {}", missing.join(", ")))
        };
    }
}

fn service_specs_for_records(records: &HashMap<String, PackageRecord>) -> Vec<ServiceRuntimeSpec> {
    let service_methods: HashMap<String, Vec<String>> = records
        .values()
        .flat_map(|record| {
            record
                .manifest
                .exports
                .services
                .iter()
                .map(|(name, export)| {
                    (
                        format!("{}/{}", record.manifest.id, name),
                        export.methods.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .flat_map(|record| {
            record
                .manifest
                .exports
                .services
                .iter()
                .map(|(name, export)| ServiceRuntimeSpec {
                    package_id: record.manifest.id.clone(),
                    service_name: name.clone(),
                    entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                    storage_root: Path::new(&record.descriptor.path)
                        .join(".bakingrl")
                        .join("storage"),
                    service_imports: record.manifest.imports.services.clone(),
                    service_methods: service_methods.clone(),
                    permissions: record.descriptor.effective_permissions.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn connector_specs_for_records(
    records: &HashMap<String, PackageRecord>,
) -> Vec<ConnectorRuntimeSpec> {
    let service_methods: HashMap<String, Vec<String>> = records
        .values()
        .flat_map(|record| {
            record
                .manifest
                .exports
                .services
                .iter()
                .map(|(name, export)| {
                    (
                        format!("{}/{}", record.manifest.id, name),
                        export.methods.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .flat_map(|record| {
            record
                .manifest
                .exports
                .connectors
                .iter()
                .map(|(name, export)| ConnectorRuntimeSpec {
                    package_id: record.manifest.id.clone(),
                    connector_name: name.clone(),
                    entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                    storage_root: Path::new(&record.descriptor.path)
                        .join(".bakingrl")
                        .join("storage"),
                    service_imports: record.manifest.imports.services.clone(),
                    service_methods: service_methods.clone(),
                    permissions: record.descriptor.effective_permissions.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn merge_settings(
    schema_path: Option<&str>,
    package_root: &Path,
    values: serde_json::Value,
) -> serde_json::Value {
    let mut merged = serde_json::Map::new();
    if let Some(schema_path) = schema_path {
        let schema = package_root
            .join(schema_path)
            .canonicalize()
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
        if let Some(fields) = schema
            .as_ref()
            .and_then(|schema| schema.get("fields"))
            .and_then(|fields| fields.as_array())
        {
            for field in fields {
                if let (Some(key), Some(default_value)) = (
                    field.get("key").and_then(|key| key.as_str()),
                    field.get("default"),
                ) {
                    merged.insert(key.to_string(), default_value.clone());
                }
            }
        }
    }
    if let Some(values) = values.as_object() {
        for (key, value) in values {
            merged.insert(key.clone(), value.clone());
        }
    }
    serde_json::Value::Object(merged)
}

fn parse_export_ref(value: &str) -> Result<(&str, &str), String> {
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

fn safe_package_relative_path(relative_path: &str) -> Result<PathBuf, String> {
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

fn read_package_file(package_root: &Path, relative_path: &str) -> Result<String, String> {
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
    fs::read_to_string(resolved)
        .map_err(|e| format!("Unable to read package file '{relative_path}': {e}"))
}

fn read_json_package_file(
    package_root: &Path,
    relative_path: &str,
) -> Result<serde_json::Value, String> {
    let raw = read_package_file(package_root, relative_path)?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("Package JSON file '{relative_path}' is invalid: {e}"))
}

fn read_json_or_default<T>(path: &Path) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: serde::Serialize,
{
    let raw = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    fs::write(path, raw).map_err(|e| format!("Failed to write JSON: {e}"))
}

fn ensure_active_layout_ids(file: &mut OverlayLayoutsFile) {
    if !file
        .layouts
        .iter()
        .any(|layout| layout.id == file.active_layout_id)
    {
        file.active_layout_id = file
            .layouts
            .first()
            .map(|layout| layout.id.clone())
            .unwrap_or_default();
    }
    if !file
        .layouts
        .iter()
        .any(|layout| layout.id == file.stream_layout_id)
    {
        file.stream_layout_id = file
            .layouts
            .first()
            .map(|layout| layout.id.clone())
            .unwrap_or_default();
    }
}

fn normalize_overlay_layouts(file: &mut OverlayLayoutsFile) {
    for layout in &mut file.layouts {
        normalize_overlay_layout(layout);
    }
}

fn normalize_overlay_layout(layout: &mut OverlayLayout) {
    if layout.layers.is_empty() {
        let mut layers = default_layers();
        if !layout.items.is_empty() {
            layers[0].items = std::mem::take(&mut layout.items);
        }
        layout.layers = layers;
    } else if !layout.items.is_empty() {
        let legacy_items = std::mem::take(&mut layout.items);
        match layout
            .layers
            .iter_mut()
            .find(|layer| layer.kind == "normal")
        {
            Some(layer) => layer.items.extend(legacy_items),
            None => layout.layers.push(OverlayLayer {
                id: unique_id("layer"),
                name: "Main".to_string(),
                kind: "normal".to_string(),
                visible: true,
                locked: false,
                order: 0,
                items: legacy_items,
            }),
        }
    }

    layout.layers.sort_by(|a, b| {
        if a.kind == "event" && b.kind != "event" {
            std::cmp::Ordering::Greater
        } else if a.kind != "event" && b.kind == "event" {
            std::cmp::Ordering::Less
        } else {
            a.order.cmp(&b.order)
        }
    });

    let mut event_seen = false;
    for (index, layer) in layout.layers.iter_mut().enumerate() {
        if layer.id.trim().is_empty() {
            layer.id = unique_id("layer");
        }
        if layer.name.trim().is_empty() {
            layer.name = if layer.kind == "event" {
                "Events".to_string()
            } else {
                "Layer".to_string()
            };
        }
        if layer.kind != "event" {
            layer.kind = "normal".to_string();
        } else if event_seen {
            layer.kind = "normal".to_string();
        } else {
            event_seen = true;
        }
        layer.order = index as i32;
        for item in &mut layer.items {
            normalize_overlay_item(item);
        }
    }

    if !event_seen {
        layout.layers.push(OverlayLayer {
            id: unique_id("layer"),
            name: "Events".to_string(),
            kind: "event".to_string(),
            visible: true,
            locked: false,
            order: layout.layers.len() as i32,
            items: Vec::new(),
        });
    }
}

fn normalize_overlay_item(item: &mut OverlayItem) {
    if item.name.trim().is_empty() {
        item.name = item.export_name.clone();
    }
    if item.opacity < 0.0 {
        item.opacity = 0.0;
    } else if item.opacity > 1.0 {
        item.opacity = 1.0;
    }
}

fn normalize_pages(file: &mut PagesFile) {
    for page in &mut file.pages {
        normalize_page(page, false);
    }
    file.pages.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn normalize_page(page: &mut PageLayout, touch_updated: bool) {
    let now = now_ms();
    if page.id.trim().is_empty() {
        page.id = unique_id("page");
    }
    if page.name.trim().is_empty() {
        page.name = "Untitled Page".to_string();
    }
    if page.width <= 0.0 {
        page.width = 1440.0;
    }
    if page.height <= 0.0 {
        page.height = 900.0;
    }
    if page.background.kind != "image" {
        page.background.kind = "color".to_string();
    }
    if page.background.color.trim().is_empty() {
        page.background.color = "#0f172a".to_string();
    }
    if page.background.fit != "contain" && page.background.fit != "stretch" {
        page.background.fit = "cover".to_string();
    }
    if page.settings.open_target != "window" {
        page.settings.open_target = "app".to_string();
    }
    if page.created_at_ms == 0 {
        page.created_at_ms = now;
    }
    if page.updated_at_ms == 0 || touch_updated {
        page.updated_at_ms = now;
    }
    if page.layers.is_empty() {
        page.layers = default_page_layers();
    }
    page.layers.sort_by(|a, b| a.order.cmp(&b.order));
    for (index, layer) in page.layers.iter_mut().enumerate() {
        if layer.id.trim().is_empty() {
            layer.id = unique_id("layer");
        }
        if layer.name.trim().is_empty() {
            layer.name = "Layer".to_string();
        }
        layer.kind = "normal".to_string();
        layer.order = index as i32;
        for item in &mut layer.items {
            normalize_page_item(item);
        }
    }
}

fn normalize_page_item(item: &mut PageItem) {
    if item.kind.trim().is_empty() {
        item.kind = if item
            .package_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && item
                .export_name
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            "visual".to_string()
        } else {
            "text".to_string()
        };
    }
    if item.kind != "visual" && item.kind != "text" && item.kind != "image" && item.kind != "shape"
    {
        item.kind = "visual".to_string();
    }
    if item.kind == "visual" {
        item.package_id = item
            .package_id
            .as_ref()
            .map(|value| value.trim().to_string());
        item.export_name = item
            .export_name
            .as_ref()
            .map(|value| value.trim().to_string());
    } else {
        item.package_id = None;
        item.export_name = None;
    }
    if item.name.trim().is_empty() {
        item.name = match item.kind.as_str() {
            "text" => "Text".to_string(),
            "image" => "Image".to_string(),
            "shape" => "Shape".to_string(),
            _ => item
                .export_name
                .clone()
                .unwrap_or_else(|| "Visual".to_string()),
        };
    }
    if item.width <= 0.0 {
        item.width = 320.0;
    }
    if item.height <= 0.0 {
        item.height = 120.0;
    }
    if item.opacity < 0.0 {
        item.opacity = 0.0;
    } else if item.opacity > 1.0 {
        item.opacity = 1.0;
    }
    if !item.settings.is_object() {
        item.settings = serde_json::json!({});
    }
}

fn default_layers() -> Vec<OverlayLayer> {
    vec![
        OverlayLayer {
            id: "main".to_string(),
            name: "Main".to_string(),
            kind: "normal".to_string(),
            visible: true,
            locked: false,
            order: 0,
            items: Vec::new(),
        },
        OverlayLayer {
            id: "events".to_string(),
            name: "Events".to_string(),
            kind: "event".to_string(),
            visible: true,
            locked: false,
            order: 1,
            items: Vec::new(),
        },
    ]
}

fn default_page_layers() -> Vec<PageLayer> {
    vec![PageLayer {
        id: "content".to_string(),
        name: "Content".to_string(),
        kind: "normal".to_string(),
        visible: true,
        locked: false,
        order: 0,
        items: Vec::new(),
    }]
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

fn find_first_bundle(root: &Path) -> Result<PathBuf, String> {
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

fn format_command_error(command: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("{command} failed")
    } else {
        format!("{command} failed: {stderr}")
    }
}

fn default_overlay_layouts() -> OverlayLayoutsFile {
    OverlayLayoutsFile {
        active_layout_id: "default".to_string(),
        stream_layout_id: "default".to_string(),
        layouts: vec![OverlayLayout {
            id: "default".to_string(),
            name: "Default".to_string(),
            width: 1920.0,
            height: 1080.0,
            layers: default_layers(),
            items: Vec::<OverlayItem>::new(),
        }],
    }
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
pub fn reload_packages(host: State<'_, Arc<PluginHost>>) -> Vec<PackageDescriptor> {
    host.reload_packages()
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
    host.remove_package(package_id)
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
pub fn save_package_settings(
    host: State<'_, Arc<PluginHost>>,
    package_id: String,
    values: serde_json::Value,
) -> Result<serde_json::Value, String> {
    host.save_package_settings(package_id, values)
}

#[tauri::command]
pub fn get_overlay_layouts(host: State<'_, Arc<PluginHost>>) -> OverlayLayoutsFile {
    host.get_overlay_layouts()
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
) -> Result<OverlayLayoutsFile, String> {
    host.create_overlay_layout(name)
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
pub fn get_pages(host: State<'_, Arc<PluginHost>>) -> PagesFile {
    host.get_pages()
}

#[tauri::command]
pub fn save_page(host: State<'_, Arc<PluginHost>>, page: PageLayout) -> Result<PagesFile, String> {
    host.save_page(page)
}

#[tauri::command]
pub fn create_page(host: State<'_, Arc<PluginHost>>, name: String) -> Result<PagesFile, String> {
    host.create_page(name)
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
