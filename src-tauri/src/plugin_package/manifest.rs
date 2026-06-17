use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PLUGIN_SCHEMA_V4: &str = "bakingrl.plugin/4";
pub const HOST_RUNTIME_API_VERSION: &str = "2.0.0";

#[derive(Debug, Clone, PartialEq)]
pub struct PluginPackageManifest {
    v4: PluginPackageManifestV4,
    compatibility: PluginCompatibility,
}

impl Serialize for PluginPackageManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.v4.serialize(serializer)
    }
}

impl PluginPackageManifest {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| format!("plugin manifest is invalid JSON: {e}"))?;
        let raw = value
            .as_object()
            .ok_or_else(|| "plugin manifest must be a JSON object".to_string())?;
        reject_legacy_manifest_fields(raw)?;

        let v4: PluginPackageManifestV4 = serde_json::from_value(value)
            .map_err(|error| format!("plugin manifest {PLUGIN_SCHEMA_V4} is invalid: {error}"))?;

        v4.validate()?;
        let compatibility = PluginCompatibility {
            runtime_api: Some(v4.bakingrl_api.clone()),
            sdk: None,
        };
        Ok(Self { v4, compatibility })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.v4.validate()
    }

    pub fn manifest_schema(&self) -> &str {
        &self.v4.schema_version
    }

    pub fn id(&self) -> &str {
        &self.v4.id
    }

    pub fn name(&self) -> &str {
        &self.v4.name
    }

    pub fn version(&self) -> &str {
        &self.v4.version
    }

    pub fn author(&self) -> Option<&str> {
        self.v4.author.as_deref()
    }

    pub fn bakingrl_api(&self) -> &str {
        &self.v4.bakingrl_api
    }

    pub fn compatibility(&self) -> Option<&PluginCompatibility> {
        Some(&self.compatibility)
    }

    pub fn settings(&self) -> Option<&str> {
        self.settings_schema()
    }

    pub fn settings_schema(&self) -> Option<&str> {
        self.v4
            .contributes
            .settings
            .as_ref()
            .and_then(|settings| settings.schema.as_deref())
    }

    pub fn settings_ui_visual(&self) -> Option<&str> {
        self.v4
            .contributes
            .settings
            .as_ref()
            .and_then(|settings| settings.ui.as_deref())
    }

    pub fn runtime_v4(&self) -> Option<&PluginRuntimeV4> {
        self.v4.runtime.as_ref()
    }

    pub fn contributes_v4(&self) -> &PluginContributesV4 {
        &self.v4.contributes
    }

    pub fn contributes_value(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.v4.contributes).ok()
    }

    pub fn external_surfaces(&self) -> Option<&PluginExternalSurfacesV4> {
        self.v4.external_surfaces.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginPackageManifestV4 {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    #[serde(rename = "bakingrlApi")]
    pub bakingrl_api: String,
    #[serde(default)]
    pub runtime: Option<PluginRuntimeV4>,
    #[serde(default)]
    pub contributes: PluginContributesV4,
    #[serde(default, rename = "externalSurfaces")]
    pub external_surfaces: Option<PluginExternalSurfacesV4>,
}

fn reject_legacy_manifest_fields(
    value: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    const REJECTED: [&str; 6] = [
        "schema",
        "compatibility",
        "capabilities",
        "kind",
        "activation",
        "settings",
    ];
    for field in REJECTED {
        if value.contains_key(field) {
            return Err(format!(
                "manifest field '{field}' is not supported in {PLUGIN_SCHEMA_V4}"
            ));
        }
    }

    if let Some(value) = value.get("contributes") {
        let object = value
            .as_object()
            .ok_or_else(|| "contributes must be an object".to_string())?;
        for field in ["pages", "views", "overlays", "webviews", "configuration"] {
            if object.contains_key(field) {
                return Err(format!(
                    "legacy contributes.{field} is not supported in {PLUGIN_SCHEMA_V4}"
                ));
            }
        }
    }

    Ok(())
}

impl PluginPackageManifestV4 {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != PLUGIN_SCHEMA_V4 {
            return Err(format!(
                "unsupported plugin schema '{}', expected '{}'",
                self.schema_version, PLUGIN_SCHEMA_V4
            ));
        }
        self.runtime
            .as_ref()
            .map_or(Ok(()), |runtime| runtime.validate())?;
        self.contributes.validate()?;
        if let Some(external_surfaces) = &self.external_surfaces {
            external_surfaces.validate()?;
        }
        validate_package_id(&self.id)?;
        validate_non_empty("name", &self.name)?;
        validate_non_empty("version", &self.version)?;
        validate_semver("bakingrlApi", &self.bakingrl_api)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeV4 {
    pub node: Option<PluginRuntimeNodeV4>,
    #[serde(default)]
    pub sidecars: Vec<PluginRuntimeSidecarV4>,
}

impl PluginRuntimeV4 {
    fn validate(&self) -> Result<(), String> {
        validate_duplicate_ids(
            "runtime.sidecars",
            self.sidecars.iter().map(|sidecar| sidecar.id.as_str()),
        )?;

        if let Some(node) = &self.node {
            validate_js_entry("runtime.node.entry", &node.entry)?;
        }
        for sidecar in &self.sidecars {
            sidecar.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeNodeV4 {
    pub entry: String,
}

impl PluginRuntimeNodeV4 {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeSidecarV4 {
    pub id: String,
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub protocol: PluginRuntimeSidecarProtocolV4,
    #[serde(default = "default_sidecar_activation")]
    pub activation: PluginRuntimeSidecarActivationV4,
}

impl PluginRuntimeSidecarV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("runtime.sidecars.id", &self.id)?;
        validate_relative_plugin_path("runtime.sidecars.bin", &self.bin)?;

        for arg in &self.args {
            validate_non_empty("runtime.sidecars.args", arg)?;
        }
        for key in self.env.keys() {
            validate_non_empty("runtime.sidecars.env key", key)?;
        }
        for platform in &self.platforms {
            validate_non_empty("runtime.sidecars.platforms", platform)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginContributesV4 {
    #[serde(default)]
    pub visuals: Vec<PluginVisualContributionV4>,
    #[serde(default)]
    pub settings: Option<PluginSettingsContributionV4>,
    #[serde(default)]
    pub services: Vec<PluginServiceContributionV4>,
    #[serde(default)]
    pub commands: Vec<PluginCommandContributionV4>,
}

impl PluginContributesV4 {
    fn validate(&self) -> Result<(), String> {
        validate_duplicate_ids(
            "contributes.visuals",
            self.visuals.iter().map(|visual| visual.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.services",
            self.services.iter().map(|service| service.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.commands",
            self.commands.iter().map(|command| command.id.as_str()),
        )?;

        for visual in &self.visuals {
            visual.validate()?;
        }
        for service in &self.services {
            service.validate()?;
        }
        for command in &self.commands {
            command.validate()?;
        }
        if let Some(settings) = &self.settings {
            settings.validate()?;
            if let Some(ui) = &settings.ui {
                let references_config_visual = self
                    .visuals
                    .iter()
                    .any(|visual| visual.id == *ui && visual.kind.as_deref() == Some("config"));
                if !references_config_visual {
                    return Err(format!(
                        "contributes.settings.ui must reference an existing contributes.visuals id with kind 'config' (missing '{ui}')"
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginVisualContributionV4 {
    pub id: String,
    pub kind: Option<String>,
    pub entry: String,
    #[serde(rename = "defaultSize")]
    pub default_size: Option<[f64; 2]>,
    #[serde(rename = "instanceSettings")]
    pub instance_settings: Option<String>,
    #[serde(rename = "remoteCompatible")]
    pub remote_compatible: Option<bool>,
}

impl PluginVisualContributionV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("contributes.visuals", &self.id)?;
        if let Some(kind) = &self.kind {
            if !matches!(kind.as_str(), "overlay" | "config" | "external") {
                return Err(
                    "contributes.visuals.kind must be overlay, config, or external".to_string(),
                );
            }
        }
        validate_js_entry("contributes.visuals.entry", &self.entry)?;
        if let Some(size) = self.default_size {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(
                    "contributes.visuals.defaultSize must contain positive dimensions".to_string(),
                );
            }
        }
        if let Some(settings) = &self.instance_settings {
            validate_relative_plugin_path("contributes.visuals.instanceSettings", settings)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginServiceContributionV4 {
    pub id: String,
    pub runtime: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    pub schema: Option<String>,
}

impl PluginServiceContributionV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("contributes.services", &self.id)?;
        if let Some(runtime) = &self.runtime {
            validate_runtime_ref("contributes.services.runtime", runtime)?;
        }
        for method in &self.methods {
            validate_export_name("contributes.services.methods", method)?;
        }
        if let Some(schema) = &self.schema {
            validate_relative_plugin_path("contributes.services.schema", schema)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(transparent)]
pub struct PluginExternalSurfacesV4 {
    surfaces: BTreeMap<String, PluginExternalSurfaceV4>,
}

impl PluginExternalSurfacesV4 {
    pub fn get(&self, id: &str) -> Option<&PluginExternalSurfaceV4> {
        self.surfaces.get(id)
    }

    fn validate(&self) -> Result<(), String> {
        for (id, surface) in &self.surfaces {
            validate_export_name("externalSurfaces", id)?;
            surface.validate(&format!("externalSurfaces.{id}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginExternalSurfaceV4 {
    pub runtime: String,
}

impl PluginExternalSurfaceV4 {
    fn validate(&self, field: &str) -> Result<(), String> {
        validate_runtime_ref(&format!("{field}.runtime"), &self.runtime)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginCommandContributionV4 {
    pub id: String,
    pub title: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
}

impl PluginCommandContributionV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("contributes.commands", &self.id)?;
        if let Some(title) = &self.title {
            validate_non_empty("contributes.commands.title", title)?;
        }
        if let Some(category) = &self.category {
            validate_non_empty("contributes.commands.category", category)?;
        }
        if let Some(icon) = &self.icon {
            validate_non_empty("contributes.commands.icon", icon)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsContributionV4 {
    pub schema: Option<String>,
    pub ui: Option<String>,
}

impl PluginSettingsContributionV4 {
    fn validate(&self) -> Result<(), String> {
        if let Some(ui) = &self.ui {
            validate_export_name("contributes.settings.ui", ui)?;
        }
        if let Some(schema) = &self.schema {
            validate_relative_plugin_path("contributes.settings.schema", schema)?;
        }
        Ok(())
    }
}

fn validate_duplicate_ids<'a>(
    field: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(format!("{field} cannot contain duplicate id '{id}'"));
        }
        validate_export_name(field, id)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginRuntimeSidecarProtocolV4 {
    #[serde(rename = "jsonrpc-stdio")]
    JsonRpcStdio,
}
fn default_sidecar_activation() -> PluginRuntimeSidecarActivationV4 {
    PluginRuntimeSidecarActivationV4::OnEnable
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginRuntimeSidecarActivationV4 {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "onEnable")]
    OnEnable,
    #[serde(rename = "onStartup")]
    OnStartup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginCompatibility {
    pub runtime_api: Option<String>,
    pub sdk: Option<String>,
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

fn validate_runtime_ref(field: &str, value: &str) -> Result<(), String> {
    if value == "node" {
        return Ok(());
    }
    if let Some(sidecar_id) = sidecar_id_from_runtime_ref(value) {
        return validate_export_name(field, sidecar_id);
    }
    Err(format!("{field} must be 'node' or 'sidecar:<id>'"))
}

fn sidecar_id_from_runtime_ref(value: &str) -> Option<&str> {
    value
        .strip_prefix("sidecar:")
        .filter(|sidecar_id| !sidecar_id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_v4_manifest() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.v4",
            "name": "V4 Metadata",
            "version": "1.2.3",
            "bakingrlApi": "2.0.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension-host.js"
                },
                "sidecars": [
                    {
                        "id": "helper",
                        "bin": "bin/helper",
                        "args": ["--stdio"],
                        "protocol": "jsonrpc-stdio",
                        "activation": "onEnable",
                        "env": {
                            "LOG_LEVEL": "info"
                        },
                        "platforms": ["darwin-arm64"]
                    }
                ]
            },
            "contributes": {
                "visuals": [
                    {
                        "id": "scoreboard",
                        "entry": "dist/visuals/scoreboard.js",
                        "defaultSize": [640, 80],
                        "instanceSettings": "schemas/scoreboard-settings.json",
                        "remoteCompatible": true
                    }
                ],
                "settings": {
                    "schema": "schemas/plugin-settings.json"
                },
                "services": [
                    {
                        "id": "stats",
                        "runtime": "sidecar:helper",
                        "methods": ["snapshot", "update"],
                        "schema": "schemas/services/stats.json"
                    }
                ],
                "commands": [
                    {
                        "id": "open-matchup",
                        "title": "Open Matchup",
                        "category": "Match",
                        "icon": "match"
                    }
                ],
            },
            "externalSurfaces": {
                "broadcast": {
                    "runtime": "sidecar:helper"
                }
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();
        assert_eq!(manifest.id(), "com.example.v4");
        assert_eq!(manifest.name(), "V4 Metadata");
        assert_eq!(manifest.version(), "1.2.3");
        assert_eq!(manifest.author(), None);
        assert_eq!(manifest.manifest_schema(), PLUGIN_SCHEMA_V4);
        assert_eq!(
            manifest
                .compatibility()
                .and_then(|c| c.runtime_api.as_deref()),
            Some("2.0.0")
        );
        assert_eq!(
            manifest
                .runtime_v4()
                .and_then(|runtime| runtime.node.as_ref())
                .map(|node| node.entry.as_str()),
            Some("dist/extension-host.js")
        );
        assert_eq!(manifest.runtime_v4().unwrap().sidecars.len(), 1);
        assert_eq!(manifest.contributes_v4().visuals.len(), 1);
        assert_eq!(manifest.contributes_v4().services.len(), 1);
        assert_eq!(manifest.contributes_v4().commands.len(), 1);
        assert_eq!(
            manifest
                .external_surfaces()
                .and_then(|external| external.get("broadcast"))
                .map(|surface| surface.runtime.as_str()),
            Some("sidecar:helper")
        );
    }

    #[test]
    fn accepts_v4_settings_ui_referencing_config_visual() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings-ui",
            "name": "Settings UI",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "contributes": {
                "visuals": [
                    {
                        "id": "settingsPanel",
                        "kind": "config",
                        "entry": "dist/settings-panel.js"
                    }
                ],
                "settings": {
                    "ui": "settingsPanel"
                }
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();
        assert_eq!(manifest.settings_ui_visual(), Some("settingsPanel"));
    }

    #[test]
    fn rejects_v4_settings_ui_without_config_visual() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings-ui",
            "name": "Settings UI",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "contributes": {
                "visuals": [
                    {
                        "id": "settingsPanel",
                        "kind": "overlay",
                        "entry": "dist/settings-panel.js"
                    }
                ],
                "settings": {
                    "ui": "settingsPanel"
                }
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("contributes.settings.ui must reference"));
    }

    #[test]
    fn rejects_legacy_extension_host_runtime_field() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.runtime-legacy",
            "name": "Legacy Extension Host",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "runtime": {
                "extensionHost": {
                    "entry": "dist/extension-host.js"
                }
            },
            "contributes": {}
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("extensionHost"));
    }

    #[test]
    fn rejects_v4_external_surface_without_runtime() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.external-missing-runtime",
            "name": "External Surface Missing Runtime",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "externalSurfaces": {
                "broadcast": {}
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("runtime"));
    }

    #[test]
    fn rejects_v4_external_surface_invalid_id() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.external-invalid-id",
            "name": "External Surface Invalid Id",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "externalSurfaces": {
                "bad/id": {
                    "runtime": "node"
                }
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("externalSurfaces 'bad/id' contains unsupported characters"));
    }

    #[test]
    fn rejects_legacy_top_level_fields() {
        for field in [
            "schema",
            "compatibility",
            "capabilities",
            "kind",
            "activation",
            "settings",
        ] {
            let raw = serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "com.example.legacy",
                "name": "Legacy",
                "version": "1.0.0",
                "bakingrlApi": "2.0.0",
                "contributes": {}
            });
            let mut raw = raw;
            raw[field] = serde_json::json!("bad");
            let raw = raw.to_string();

            let error = PluginPackageManifest::parse(&raw).unwrap_err();
            assert!(
                error.contains(&format!("manifest field '{field}'")),
                "field '{field}' should be rejected"
            );
        }
    }

    #[test]
    fn rejects_legacy_contributions_sections() {
        for field in ["pages", "views", "overlays", "webviews", "configuration"] {
            let mut contributes = serde_json::Map::new();
            contributes.insert(field.to_string(), serde_json::json!([]));
            let raw = serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "com.example.legacy",
                "name": "Legacy",
                "version": "1.0.0",
                "bakingrlApi": "2.0.0",
                "contributes": contributes
            })
            .to_string();

            let error = PluginPackageManifest::parse(&raw).unwrap_err();
            assert!(
                error.contains(&format!("legacy contributes.{field}")),
                "contributes.{field} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_v3_schema_field() {
        let raw = serde_json::json!({
            "schema": "bakingrl.plugin/legacy",
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.dual",
            "name": "Dual Schema",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "contributes": {}
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("manifest field 'schema'"));
    }

    #[test]
    fn rejects_v4_duplicate_ids() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.duplicate",
            "name": "Duplicate",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "helper",
                        "bin": "bin/helper-a",
                        "protocol": "jsonrpc-stdio",
                        "activation": "manual"
                    },
                    {
                        "id": "helper",
                        "bin": "bin/helper-b",
                        "protocol": "jsonrpc-stdio",
                        "activation": "manual"
                    }
                ]
            },
            "contributes": {
                "visuals": [
                    {"id": "scoreboard", "entry": "dist/visuals/scoreboard.js"},
                    {"id": "scoreboard", "entry": "dist/visuals/scoreboard-2.js"}
                ],
                "services": [
                    {"id": "stats", "methods": ["snapshot"]},
                    {"id": "stats", "runtime": "helper"}
                ],
                "commands": [
                    {"id": "launch", "title": "Launch"},
                    {"id": "launch", "title": "Launch again"}
                ]
            }
        })
        .to_string();

        assert!(PluginPackageManifest::parse(&raw)
            .unwrap_err()
            .contains("cannot contain duplicate id"));
    }

    #[test]
    fn rejects_non_js_entries() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.entries",
            "name": "Entries",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension-host.ts"
                }
            },
            "contributes": {
                "visuals": [
                    {
                        "id": "scoreboard",
                        "entry": "dist/visuals/scoreboard.ts"
                    }
                ]
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains(".js"));
    }

    #[test]
    fn rejects_path_like_package_ids() {
        for id in [".", "..", ".com.example", "com.example.", "com..example"] {
            let raw = serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": id,
                "name": "Bad",
                "version": "1.0.0",
                "bakingrlApi": "2.0.0",
                "contributes": {}
            })
            .to_string();
            assert!(
                PluginPackageManifest::parse(&raw).is_err(),
                "{id} should be rejected"
            );
        }
    }
}
