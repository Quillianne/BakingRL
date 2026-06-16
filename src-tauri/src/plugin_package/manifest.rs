use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PLUGIN_SCHEMA_V3: &str = "bakingrl.plugin/3";
pub const HOST_RUNTIME_API_VERSION: &str = "1.0.0";
pub const HOST_RUNTIME_API_RANGE: &str = ">=1.0.0 <2.0.0";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(transparent)]
pub struct PluginPackageManifest(PluginPackageManifestV3);

impl PluginPackageManifest {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| format!("plugin manifest is invalid JSON: {e}"))?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "plugin manifest is missing schema".to_string())?;
        let manifest = match schema {
            PLUGIN_SCHEMA_V3 => {
                let manifest: PluginPackageManifestV3 = serde_json::from_value(value)
                    .map_err(|e| format!("plugin manifest {PLUGIN_SCHEMA_V3} is invalid: {e}"))?;
                Self(manifest)
            }
            other => {
                return Err(format!(
                    "unsupported plugin schema '{other}', expected '{PLUGIN_SCHEMA_V3}'"
                ));
            }
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.0.validate()
    }

    pub fn manifest_schema(&self) -> &str {
        &self.0.schema
    }

    pub fn id(&self) -> &str {
        &self.0.id
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn version(&self) -> &str {
        &self.0.version
    }

    pub fn author(&self) -> Option<&str> {
        self.0.author.as_deref()
    }

    pub fn compatibility(&self) -> Option<&PluginCompatibility> {
        self.0.compatibility.as_ref()
    }

    pub fn settings(&self) -> Option<&str> {
        self.0.settings.as_deref()
    }

    pub fn kind(&self) -> Option<&str> {
        self.0.kind.as_deref()
    }

    pub fn activation(&self) -> Option<&PluginActivationV3> {
        Some(&self.0.activation)
    }

    pub fn runtime(&self) -> Option<&PluginRuntimeV3> {
        self.0.runtime.as_ref()
    }

    pub fn contributes(&self) -> Option<&PluginContributesV3> {
        Some(&self.0.contributes)
    }

    pub fn contributes_value(&self) -> Option<serde_json::Value> {
        self.contributes()
            .and_then(|contributes| serde_json::to_value(contributes).ok())
    }

    pub fn normalized_contributes_v3(&self) -> PluginContributesV3 {
        self.0.contributes.clone()
    }

    pub fn capabilities(&self) -> Option<&serde_json::Value> {
        Some(&self.0.capabilities)
    }

    pub fn diagnostics(&self) -> Option<&PluginDiagnosticsV3> {
        self.0.diagnostics.as_ref()
    }

    pub fn safe_mode(&self) -> Option<&PluginSafeModeV3> {
        self.0.safe_mode.as_ref()
    }

    pub fn is_v3(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageManifestV3 {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub kind: Option<String>,
    #[serde(default)]
    pub activation: PluginActivationV3,
    pub runtime: Option<PluginRuntimeV3>,
    #[serde(default)]
    pub contributes: PluginContributesV3,
    #[serde(default = "empty_json_object")]
    pub capabilities: serde_json::Value,
    pub diagnostics: Option<PluginDiagnosticsV3>,
    #[serde(default)]
    pub safe_mode: Option<PluginSafeModeV3>,
    pub compatibility: Option<PluginCompatibility>,
    pub settings: Option<String>,
    #[serde(default, rename = "exports", skip_serializing)]
    rejected_exports_field: Option<serde_json::Value>,
}

impl PluginPackageManifestV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PLUGIN_SCHEMA_V3 {
            return Err(format!(
                "unsupported plugin schema '{}', expected '{}'",
                self.schema, PLUGIN_SCHEMA_V3
            ));
        }
        validate_package_id(&self.id)?;
        validate_non_empty("name", &self.name)?;
        validate_non_empty("version", &self.version)?;
        if let Some(kind) = &self.kind {
            validate_non_empty("kind", kind)?;
        }
        self.activation.validate()?;
        if let Some(runtime) = &self.runtime {
            runtime.validate()?;
        }
        if self.rejected_exports_field.is_some() {
            return Err(
                "bakingrl.plugin/3 manifests must declare package surfaces under contributes, not exports"
                    .to_string(),
            );
        }
        self.contributes.validate_allow_empty()?;
        validate_json_object("capabilities", &self.capabilities)?;
        if let Some(value) = self.capabilities.get("permissions") {
            serde_json::from_value::<PluginPermissions>(value.clone())
                .map_err(|error| format!("capabilities.permissions is invalid: {error}"))?
                .validate(&self.id)?;
        }
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.validate()?;
        }
        if let Some(safe_mode) = &self.safe_mode {
            safe_mode.validate()?;
        }
        let compatibility = self
            .compatibility
            .as_ref()
            .ok_or_else(|| "compatibility.runtimeApi is required".to_string())?;
        let runtime_api = compatibility
            .runtime_api
            .as_deref()
            .ok_or_else(|| "compatibility.runtimeApi is required".to_string())?;
        validate_semver("compatibility.runtimeApi", runtime_api)?;
        compatibility.validate()?;
        if let Some(settings) = &self.settings {
            validate_relative_plugin_path("settings", settings)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginActivationV3 {
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub on_startup: bool,
}

impl PluginActivationV3 {
    fn validate(&self) -> Result<(), String> {
        for event in &self.events {
            validate_non_empty("activation.events", event)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeV3 {
    pub extension_host: Option<PluginExtensionHostRuntimeV3>,
    #[serde(default)]
    pub sidecars: BTreeMap<String, PluginRuntimeSidecarV3>,
}

impl PluginRuntimeV3 {
    fn validate(&self) -> Result<(), String> {
        if let Some(extension_host) = &self.extension_host {
            extension_host.validate()?;
        }
        for (id, sidecar) in &self.sidecars {
            validate_export_name("runtime.sidecars", id)?;
            sidecar.validate(&format!("runtime.sidecars.{id}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginExtensionHostRuntimeV3 {
    pub entry: Option<String>,
    #[serde(default)]
    pub sidecars: BTreeMap<String, PluginRuntimeSidecarV3>,
}

impl PluginExtensionHostRuntimeV3 {
    fn validate(&self) -> Result<(), String> {
        if let Some(entry) = &self.entry {
            validate_js_entry("runtime.extensionHost.entry", entry)?;
        }
        for (id, sidecar) in &self.sidecars {
            validate_export_name("runtime.extensionHost.sidecars", id)?;
            sidecar.validate(&format!("runtime.extensionHost.sidecars.{id}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeSidecarV3 {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub protocol: PluginRuntimeSidecarProtocolV3,
    pub activation: PluginRuntimeSidecarActivationV3,
}

impl PluginRuntimeSidecarV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_relative_plugin_path(&format!("{field}.command"), &self.command)?;
        for arg in &self.args {
            validate_non_empty(&format!("{field}.args"), arg)?;
        }
        for key in self.env.keys() {
            validate_non_empty(&format!("{field}.env key"), key)?;
        }
        for platform in &self.platforms {
            validate_non_empty(&format!("{field}.platforms"), platform)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginRuntimeSidecarProtocolV3 {
    #[serde(rename = "jsonrpc-stdio")]
    JsonRpcStdio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginRuntimeSidecarActivationV3 {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "onActivation")]
    OnActivation,
    #[serde(rename = "onStartup")]
    OnStartup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiagnosticsV3 {
    #[serde(default)]
    pub enabled: bool,
    pub channel: Option<String>,
}

impl PluginDiagnosticsV3 {
    fn validate(&self) -> Result<(), String> {
        if let Some(channel) = &self.channel {
            validate_non_empty("diagnostics.channel", channel)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginSafeModeV3 {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub disable_runtime: bool,
}

impl PluginSafeModeV3 {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginCompatibility {
    pub runtime_api: Option<String>,
    pub sdk: Option<String>,
}

impl PluginCompatibility {
    fn validate(&self) -> Result<(), String> {
        if let Some(runtime_api) = &self.runtime_api {
            validate_semver("compatibility.runtimeApi", runtime_api)?;
        }
        if let Some(sdk) = &self.sdk {
            validate_semver("compatibility.sdk", sdk)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributesV3 {
    #[serde(default)]
    pub commands: BTreeMap<String, CommandContributionV3>,
    #[serde(default)]
    pub visuals: BTreeMap<String, VisualContributionV3>,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceContributionV3>,
    #[serde(default)]
    pub assets: BTreeMap<String, AssetContributionV3>,
    #[serde(default)]
    pub schemas: BTreeMap<String, SchemaContributionV3>,
    #[serde(default)]
    pub pages: BTreeMap<String, PageContributionV3>,
    #[serde(default)]
    pub views: BTreeMap<String, WebviewContributionV3>,
    #[serde(default)]
    pub overlays: BTreeMap<String, OverlayContributionV3>,
    #[serde(default)]
    pub webviews: BTreeMap<String, WebviewContributionV3>,
    #[serde(default)]
    pub configuration: BTreeMap<String, ConfigurationContributionV3>,
}

impl PluginContributesV3 {
    pub fn validate_allow_empty(&self) -> Result<(), String> {
        for (name, contribution) in &self.commands {
            validate_export_name("contributes.commands", name)?;
            contribution.validate("contributes.commands")?;
        }
        for (name, contribution) in &self.visuals {
            validate_export_name("contributes.visuals", name)?;
            contribution.validate("contributes.visuals")?;
        }
        for (name, contribution) in &self.services {
            validate_export_name("contributes.services", name)?;
            contribution.validate("contributes.services")?;
        }
        for (name, contribution) in &self.assets {
            validate_export_name("contributes.assets", name)?;
            contribution.validate("contributes.assets")?;
        }
        for (name, contribution) in &self.schemas {
            validate_export_name("contributes.schemas", name)?;
            contribution.validate("contributes.schemas")?;
        }
        for (name, contribution) in &self.pages {
            validate_export_name("contributes.pages", name)?;
            contribution.validate("contributes.pages")?;
        }
        for (name, contribution) in &self.views {
            validate_export_name("contributes.views", name)?;
            contribution.validate("contributes.views")?;
        }
        for (name, contribution) in &self.overlays {
            validate_export_name("contributes.overlays", name)?;
            contribution.validate("contributes.overlays")?;
        }
        for (name, contribution) in &self.webviews {
            validate_export_name("contributes.webviews", name)?;
            contribution.validate("contributes.webviews")?;
        }
        for (name, contribution) in &self.configuration {
            validate_export_name("contributes.configuration", name)?;
            contribution.validate("contributes.configuration")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CommandContributionV3 {
    pub title: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
}

impl CommandContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        if let Some(title) = &self.title {
            validate_non_empty(&format!("{field}.title"), title)?;
        }
        if let Some(category) = &self.category {
            validate_non_empty(&format!("{field}.category"), category)?;
        }
        if let Some(icon) = &self.icon {
            validate_non_empty(&format!("{field}.icon"), icon)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisualContributionV3 {
    pub entry: String,
    pub default_size: Option<[f64; 2]>,
    pub settings: Option<String>,
}

impl VisualContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_js_entry(&format!("{field}.entry"), &self.entry)?;
        if let Some(size) = self.default_size {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(format!(
                    "{field}.defaultSize must contain positive dimensions"
                ));
            }
        }
        if let Some(settings) = &self.settings {
            validate_relative_plugin_path(&format!("{field}.settings"), settings)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceContributionV3 {
    pub title: Option<String>,
    pub entry: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    pub sidecar: Option<String>,
    pub schema: Option<String>,
}

impl ServiceContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        if let Some(title) = &self.title {
            validate_non_empty(&format!("{field}.title"), title)?;
        }
        if let Some(entry) = &self.entry {
            validate_js_entry(&format!("{field}.entry"), entry)?;
        }
        if let Some(sidecar) = &self.sidecar {
            validate_export_name(&format!("{field}.sidecar"), sidecar)?;
        }
        for method in &self.methods {
            validate_export_name(&format!("{field}.methods"), method)?;
        }
        if let Some(schema) = &self.schema {
            validate_relative_plugin_path(&format!("{field}.schema"), schema)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetContributionV3 {
    pub path: String,
}

impl AssetContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_relative_plugin_path(&format!("{field}.path"), &self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaContributionV3 {
    pub path: String,
}

impl SchemaContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_relative_plugin_path(&format!("{field}.path"), &self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageContributionV3 {
    pub path: Option<String>,
    pub entry: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub configuration: Option<String>,
    pub route: Option<String>,
}

impl PageContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_entry_or_path(field, self.entry.as_deref(), self.path.as_deref())?;
        validate_optional_relative_path(field, "icon", self.icon.as_deref())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlayContributionV3 {
    pub path: Option<String>,
    pub entry: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub configuration: Option<String>,
    pub route: Option<String>,
    pub default_size: Option<[f64; 2]>,
}

impl OverlayContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_entry_or_path(field, self.entry.as_deref(), self.path.as_deref())?;
        validate_optional_relative_path(field, "icon", self.icon.as_deref())?;
        if let Some(size) = self.default_size {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(format!(
                    "{field}.defaultSize must contain positive dimensions"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebviewContributionV3 {
    pub entry: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub configuration: Option<String>,
    pub route: Option<String>,
}

impl WebviewContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_entry_or_path(field, self.entry.as_deref(), self.path.as_deref())?;
        validate_optional_relative_path(field, "icon", self.icon.as_deref())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigurationContributionV3 {
    pub schema: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub visuals: BTreeMap<String, VisualContributionV3>,
}

impl ConfigurationContributionV3 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_relative_plugin_path(&format!("{field}.schema"), &self.schema)?;
        validate_optional_relative_path(field, "path", self.path.as_deref())?;
        for (name, contribution) in &self.visuals {
            validate_export_name(&format!("{field}.visuals"), name)?;
            contribution.validate(&format!("{field}.visuals"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PluginPermissions {
    pub bus: Option<BusPermissions>,
    pub registry: Option<RegistryPermissions>,
    pub network: Option<NetworkPermissions>,
    #[serde(default)]
    pub storage: Vec<String>,
}

impl PluginPermissions {
    fn validate(&self, package_id: &str) -> Result<(), String> {
        if let Some(bus) = &self.bus {
            validate_patterns("permissions.bus.read", &bus.read)?;
            validate_package_write_patterns(package_id, "permissions.bus.publish", &bus.publish)?;
        }
        if let Some(registry) = &self.registry {
            validate_patterns("permissions.registry.read", &registry.read)?;
            validate_package_write_patterns(
                package_id,
                "permissions.registry.write",
                &registry.write,
            )?;
        }
        if let Some(network) = &self.network {
            validate_domains("permissions.network.http", &network.http)?;
            validate_domains("permissions.network.websocket", &network.websocket)?;
        }
        for storage in &self.storage {
            if storage != "plugin://self/*" {
                return Err(format!("permissions.storage cannot request '{storage}'"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BusPermissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RegistryPermissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NetworkPermissions {
    #[serde(default)]
    pub http: Vec<String>,
    #[serde(default)]
    pub websocket: Vec<String>,
}

pub fn validate_relative_plugin_path(field: &str, value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.trim().is_empty() {
        return Err(format!("{field} cannot be empty"));
    }
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(format!("{field} must be a relative path inside the plugin"));
    }
    Ok(())
}

fn validate_optional_relative_path(
    field: &str,
    name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    if let Some(value) = value {
        validate_relative_plugin_path(&format!("{field}.{name}"), value)?;
    }
    Ok(())
}

fn validate_entry_or_path(
    field: &str,
    entry: Option<&str>,
    path: Option<&str>,
) -> Result<(), String> {
    match (entry, path) {
        (Some(entry), _) => validate_js_entry(&format!("{field}.entry"), entry),
        (None, Some(path)) => validate_relative_plugin_path(&format!("{field}.path"), path),
        (None, None) => Err(format!("{field} must declare entry or path")),
    }
}

fn validate_js_entry(field: &str, value: &str) -> Result<(), String> {
    validate_relative_plugin_path(field, value)?;
    if !value.ends_with(".js") {
        return Err(format!("{field} must point to a built .js file"));
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(())
    }
}

pub fn parse_runtime_api_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn validate_semver(field: &str, value: &str) -> Result<(), String> {
    if parse_runtime_api_version(value).is_none() {
        return Err(format!("{field} must be a semver version like 1.0.0"));
    }
    Ok(())
}

fn empty_json_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

fn validate_json_object(field: &str, value: &serde_json::Value) -> Result<(), String> {
    if value.is_object() {
        Ok(())
    } else {
        Err(format!("{field} must be a JSON object"))
    }
}

fn validate_package_id(value: &str) -> Result<(), String> {
    validate_non_empty("id", value)?;
    if value == "." || value == ".." || value.starts_with('.') || value.ends_with('.') {
        return Err("id must not contain empty or dot-only path segments".to_string());
    }
    if value.split('.').any(|segment| segment.is_empty()) {
        return Err("id must not contain empty dot-separated segments".to_string());
    }
    if value.starts_with("plugin.") {
        return Err("id must not include the reserved 'plugin.' runtime prefix".to_string());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_')
    {
        return Err("id contains unsupported characters".to_string());
    }
    Ok(())
}

fn validate_export_name(kind: &str, value: &str) -> Result<(), String> {
    validate_non_empty(kind, value)?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!("{kind} '{value}' contains unsupported characters"));
    }
    Ok(())
}

fn validate_patterns(field: &str, values: &[String]) -> Result<(), String> {
    for value in values {
        if value.trim().is_empty() {
            return Err(format!("{field} contains an empty pattern"));
        }
    }
    Ok(())
}

fn validate_package_write_patterns(
    package_id: &str,
    field: &str,
    values: &[String],
) -> Result<(), String> {
    let expected = format!("plugin.{package_id}.*");
    for value in values {
        if value != &expected {
            return Err(format!("{field} must be limited to '{expected}'"));
        }
    }
    Ok(())
}

fn validate_domains(field: &str, values: &[String]) -> Result<(), String> {
    for value in values {
        if value.trim().is_empty() || value.contains('*') || value.contains('/') {
            return Err(format!("{field} contains invalid domain '{value}'"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v3_metadata_only_manifest() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.v3",
            "name": "V3 Metadata",
            "version": "1.0.0",
            "kind": "trusted",
            "activation": {
                "events": ["onInstalled"]
            },
            "runtime": {
                "extensionHost": {
                    "entry": "dist/extension-host.js",
                    "sidecars": {
                        "helper": {
                            "command": "bin/helper",
                            "protocol": "jsonrpc-stdio",
                            "activation": "manual"
                        }
                    }
                }
            },
            "contributes": {},
            "capabilities": {
                "diagnostics": true
            },
            "diagnostics": {
                "enabled": true,
                "channel": "default"
            },
            "safeMode": {
                "enabled": true,
                "disableRuntime": true
            },
            "compatibility": {
                "runtimeApi": "1.0.1"
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();

        assert!(manifest.is_v3());
        assert_eq!(manifest.id(), "com.example.v3");
        assert_eq!(manifest.kind(), Some("trusted"));
        assert_eq!(manifest.normalized_contributes_v3().services.len(), 0);
    }

    #[test]
    fn parses_v3_runtime_sidecars_object() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.sidecar",
            "name": "Sidecar",
            "version": "1.0.0",
            "runtime": {
                "sidecars": {
                    "helper": {
                        "command": "bin/helper",
                        "args": ["--stdio"],
                        "env": {
                            "LOG_LEVEL": "debug"
                        },
                        "platforms": ["darwin-arm64"],
                        "protocol": "jsonrpc-stdio",
                        "activation": "onActivation"
                    }
                }
            },
            "contributes": {},
            "compatibility": {
                "runtimeApi": "1.0.0"
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();
        let runtime = manifest.runtime().unwrap();
        let sidecar = runtime.sidecars.get("helper").unwrap();

        assert_eq!(sidecar.command, "bin/helper");
        assert_eq!(sidecar.args, vec!["--stdio"]);
        assert_eq!(
            sidecar.activation,
            PluginRuntimeSidecarActivationV3::OnActivation
        );
    }

    #[test]
    fn rejects_v3_sidecars_array() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.sidecar",
            "name": "Sidecar",
            "version": "1.0.0",
            "runtime": {
                "sidecars": [{
                    "command": "bin/helper",
                    "protocol": "jsonrpc-stdio",
                    "activation": "manual"
                }]
            },
            "contributes": {},
            "compatibility": {
                "runtimeApi": "1.0.0"
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("bakingrl.plugin/3"));
    }

    #[test]
    fn rejects_unknown_manifest_schema_at_parse_boundary() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/old",
            "id": "com.example.unsupported",
            "name": "Unsupported",
            "version": "1.0.0",
            "contributes": {}
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("bakingrl.plugin/old"));
        assert!(error.contains("bakingrl.plugin/3"));
    }

    #[test]
    fn rejects_v3_without_runtime_api() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.no-runtime-api",
            "name": "No Runtime API",
            "version": "1.0.0",
            "contributes": {}
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("compatibility.runtimeApi"));
    }

    #[test]
    fn rejects_v3_exports_field() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.legacy-exports",
            "name": "Legacy Exports",
            "version": "1.0.0",
            "exports": {
                "visuals": {
                    "scoreboard": {
                        "entry": "dist/visuals/scoreboard.js"
                    }
                }
            },
            "compatibility": {
                "runtimeApi": "1.0.0"
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("contributes"));
        assert!(error.contains("not exports"));
    }

    #[test]
    fn rejects_path_like_package_ids() {
        for id in [".", "..", ".com.example", "com.example.", "com..example"] {
            let raw = serde_json::json!({
                "schema": "bakingrl.plugin/3",
                "id": id,
                "name": "Bad",
                "version": "1.0.0",
                "contributes": {},
                "compatibility": {
                    "runtimeApi": "1.0.0"
                }
            })
            .to_string();

            assert!(
                PluginPackageManifest::parse(&raw).is_err(),
                "{id} should be rejected"
            );
        }
    }
}
