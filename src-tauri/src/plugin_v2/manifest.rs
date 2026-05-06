use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PLUGIN_SCHEMA_V2: &str = "bakingrl.plugin/2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageManifestV2 {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    #[serde(default)]
    pub exports: PluginExportsV2,
    #[serde(default)]
    pub imports: PluginImportsV2,
    #[serde(default)]
    pub permissions: PluginPermissionsV2,
    pub settings: Option<String>,
}

impl PluginPackageManifestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PLUGIN_SCHEMA_V2 {
            return Err(format!(
                "unsupported plugin schema '{}', expected '{}'",
                self.schema, PLUGIN_SCHEMA_V2
            ));
        }
        validate_package_id(&self.id)?;
        validate_non_empty("name", &self.name)?;
        validate_non_empty("version", &self.version)?;
        self.exports.validate()?;
        self.imports.validate()?;
        self.permissions.validate(&self.id)?;
        if let Some(settings) = &self.settings {
            validate_relative_plugin_path("settings", settings)?;
        }
        Ok(())
    }

    pub fn export_count(&self) -> usize {
        self.exports.visuals.len()
            + self.exports.components.len()
            + self.exports.services.len()
            + self.exports.connectors.len()
            + self.exports.pages.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginExportsV2 {
    #[serde(default)]
    pub visuals: BTreeMap<String, VisualExportV2>,
    #[serde(default)]
    pub components: BTreeMap<String, ComponentExportV2>,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceExportV2>,
    #[serde(default)]
    pub connectors: BTreeMap<String, ConnectorExportV2>,
    #[serde(default)]
    pub assets: BTreeMap<String, AssetExportV2>,
    #[serde(default)]
    pub schemas: BTreeMap<String, SchemaExportV2>,
    #[serde(default)]
    pub pages: BTreeMap<String, PageExportV2>,
}

impl PluginExportsV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.visuals.is_empty()
            && self.components.is_empty()
            && self.services.is_empty()
            && self.connectors.is_empty()
            && self.assets.is_empty()
            && self.schemas.is_empty()
            && self.pages.is_empty()
        {
            return Err("plugin must export at least one capability".to_string());
        }
        for (name, export) in &self.visuals {
            validate_export_name("visual", name)?;
            export.validate()?;
        }
        for (name, export) in &self.components {
            validate_export_name("component", name)?;
            export.validate()?;
        }
        for (name, export) in &self.services {
            validate_export_name("service", name)?;
            export.validate()?;
        }
        for (name, export) in &self.connectors {
            validate_export_name("connector", name)?;
            export.validate()?;
        }
        for (name, export) in &self.assets {
            validate_export_name("asset", name)?;
            export.validate()?;
        }
        for (name, export) in &self.schemas {
            validate_export_name("schema", name)?;
            export.validate()?;
        }
        for (name, export) in &self.pages {
            validate_export_name("page", name)?;
            export.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisualExportV2 {
    pub entry: String,
    pub default_size: Option<[f64; 2]>,
    pub settings: Option<String>,
}

impl VisualExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_js_entry("visual.entry", &self.entry)?;
        if let Some(size) = self.default_size {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err("visual.defaultSize must contain positive dimensions".to_string());
            }
        }
        if let Some(settings) = &self.settings {
            validate_relative_plugin_path("visual.settings", settings)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentExportV2 {
    pub entry: String,
    pub props: Option<String>,
}

impl ComponentExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_js_entry("component.entry", &self.entry)?;
        if let Some(props) = &self.props {
            validate_relative_plugin_path("component.props", props)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceExportV2 {
    pub entry: String,
    #[serde(default)]
    pub methods: Vec<String>,
    pub schema: Option<String>,
}

impl ServiceExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_js_entry("service.entry", &self.entry)?;
        for method in &self.methods {
            validate_export_name("service method", method)?;
        }
        if let Some(schema) = &self.schema {
            validate_relative_plugin_path("service.schema", schema)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectorExportV2 {
    pub entry: String,
}

impl ConnectorExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_js_entry("connector.entry", &self.entry)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetExportV2 {
    pub path: String,
}

impl AssetExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_relative_plugin_path("asset.path", &self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaExportV2 {
    pub path: String,
}

impl SchemaExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_relative_plugin_path("schema.path", &self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageExportV2 {
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

impl PageExportV2 {
    fn validate(&self) -> Result<(), String> {
        validate_relative_plugin_path("page.path", &self.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginImportsV2 {
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
}

impl PluginImportsV2 {
    fn validate(&self) -> Result<(), String> {
        for import in &self.components {
            validate_import_ref("component import", import)?;
        }
        for import in &self.services {
            validate_import_ref("service import", import)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PluginPermissionsV2 {
    pub bus: Option<BusPermissionsV2>,
    pub registry: Option<RegistryPermissionsV2>,
    pub network: Option<NetworkPermissionsV2>,
    #[serde(default)]
    pub storage: Vec<String>,
}

impl PluginPermissionsV2 {
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
pub struct BusPermissionsV2 {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RegistryPermissionsV2 {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NetworkPermissionsV2 {
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

fn validate_package_id(value: &str) -> Result<(), String> {
    validate_non_empty("id", value)?;
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

fn validate_import_ref(field: &str, value: &str) -> Result<(), String> {
    let Some((package_id, export_name)) = value.split_once('/') else {
        return Err(format!(
            "{field} '{value}' must use '<package-id>/<export>'"
        ));
    };
    validate_package_id(package_id)?;
    validate_export_name(field, export_name)
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
    fn validates_full_package_manifest() {
        let manifest: PluginPackageManifestV2 = serde_json::from_value(serde_json::json!({
            "schema": "bakingrl.plugin/2",
            "id": "com.example.match-tools",
            "name": "Match Tools",
            "version": "1.0.0",
            "exports": {
                "visuals": {
                    "scoreboard": {
                        "entry": "dist/visuals/scoreboard.js",
                        "defaultSize": [600, 90]
                    }
                },
                "components": {
                    "TeamBadge": {
                        "entry": "dist/components/team-badge.js",
                        "props": "schemas/team-badge.json"
                    }
                },
                "services": {
                    "matchStats": {
                        "entry": "dist/services/match-stats.js",
                        "methods": ["snapshot"]
                    }
                },
                "connectors": {
                    "twitch": {
                        "entry": "dist/connectors/twitch.js"
                    }
                }
            },
            "imports": {
                "components": ["com.bakingrl.ui/TeamBadge"],
                "services": ["com.bakingrl.stats/matchStats"]
            },
            "permissions": {
                "bus": {
                    "read": ["UpdateState"],
                    "publish": ["plugin.com.example.match-tools.*"]
                },
                "registry": {
                    "read": ["plugin.com.bakingrl.*"],
                    "write": ["plugin.com.example.match-tools.*"]
                },
                "network": {
                    "http": ["api.twitch.tv"],
                    "websocket": []
                },
                "storage": ["plugin://self/*"]
            }
        }))
        .unwrap();

        manifest.validate().unwrap();
        assert_eq!(manifest.export_count(), 4);
    }

    #[test]
    fn rejects_path_traversal_entries() {
        let manifest: PluginPackageManifestV2 = serde_json::from_value(serde_json::json!({
            "schema": "bakingrl.plugin/2",
            "id": "com.example.bad",
            "name": "Bad",
            "version": "1.0.0",
            "exports": {
                "visuals": {
                    "bad": { "entry": "../outside.js" }
                }
            }
        }))
        .unwrap();

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_write_permissions_outside_plugin_namespace() {
        let manifest: PluginPackageManifestV2 = serde_json::from_value(serde_json::json!({
            "schema": "bakingrl.plugin/2",
            "id": "com.example.bad",
            "name": "Bad",
            "version": "1.0.0",
            "exports": {
                "services": {
                    "bad": { "entry": "dist/services/bad.js" }
                }
            },
            "permissions": {
                "registry": {
                    "write": ["plugin.com.other.*"]
                }
            }
        }))
        .unwrap();

        assert!(manifest.validate().is_err());
    }
}
