use std::collections::HashMap;
use std::path::Path;

use crate::plugin_package::bundle::BundleInspection;
use crate::plugin_package::manifest::{
    parse_runtime_api_version, PluginPackageManifest, PluginRuntimeV4, HOST_RUNTIME_API_VERSION,
};

use super::package_files::read_json_package_file;
use super::settings_contract::secret_key_set;
use super::PackageRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PackageDescriptor {
    #[serde(rename = "manifestSchema")]
    pub manifest_schema: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub runtime: Option<PluginRuntimeV4>,
    pub contributes: Option<serde_json::Value>,
    pub enabled: bool,
    pub status: PackageStatus,
    pub path: String,
    pub contributions: PackageContributionsDescriptor,
    pub compatibility: PackageCompatibilityDescriptor,
    pub settings: Option<String>,
    pub has_public_settings: bool,
    pub has_secrets: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageStatus {
    Installed,
    Deleting,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct PackageContributionsDescriptor {
    pub commands: Vec<NamedContributionDescriptor>,
    pub visuals: Vec<VisualContributionDescriptor>,
    pub services: Vec<ServiceContributionDescriptor>,
    pub views: Vec<WebviewContributionDescriptor>,
    pub assets: Vec<NamedContributionDescriptor>,
    pub schemas: Vec<NamedContributionDescriptor>,
    pub pages: Vec<PageContributionDescriptor>,
    pub overlays: Vec<OverlayContributionDescriptor>,
    pub webviews: Vec<WebviewContributionDescriptor>,
    pub configuration: Option<ConfigurationContributionDescriptor>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisualContributionDescriptor {
    pub name: String,
    pub entry: String,
    pub default_width: f64,
    pub default_height: f64,
    pub settings: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NamedContributionDescriptor {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceContributionDescriptor {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageContributionDescriptor {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OverlayContributionDescriptor {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WebviewContributionDescriptor {
    pub name: String,
    pub entry: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub configuration: Option<String>,
    pub route: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfigurationContributionDescriptor {
    pub title: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub visuals: Vec<VisualContributionDescriptor>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageCompatibilityStatus {
    Compatible,
    Incompatible,
    RequiresNewerHost,
    UnknownRuntimeApi,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCompatibilityDescriptor {
    pub status: PackageCompatibilityStatus,
    pub bakingrl_api: Option<String>,
    pub sdk: Option<String>,
    pub host_runtime_api: String,
    pub supported_runtime_api: String,
    pub message: Option<String>,
}

impl PackageCompatibilityDescriptor {
    pub fn is_compatible(&self) -> bool {
        self.status == PackageCompatibilityStatus::Compatible
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreparedPackageInstall {
    pub path: String,
    pub source: String,
    pub inspection: BundleInspection,
}

pub(super) fn descriptor_for_manifest(
    manifest: &PluginPackageManifest,
    path: String,
    enabled: bool,
) -> PackageDescriptor {
    let compatibility = compatibility_for_manifest(manifest);
    let enabled = enabled && compatibility.is_compatible();
    let (has_public_settings, has_secrets) = package_settings_capabilities(manifest, &path);
    let contributes = manifest.contributes_v4();
    PackageDescriptor {
        manifest_schema: manifest.manifest_schema().to_string(),
        id: manifest.id().to_string(),
        name: manifest.name().to_string(),
        version: manifest.version().to_string(),
        author: manifest.author().map(ToOwned::to_owned),
        runtime: manifest.runtime_v4().cloned(),
        contributes: manifest.contributes_value(),
        enabled,
        status: PackageStatus::Installed,
        path,
        contributions: PackageContributionsDescriptor {
            commands: contributes
                .commands
                .iter()
                .map(|command| NamedContributionDescriptor {
                    name: command.id.clone(),
                })
                .collect(),
            visuals: contributes
                .visuals
                .iter()
                .map(|export| {
                    let [default_width, default_height] =
                        export.default_size.unwrap_or([320.0, 120.0]);
                    VisualContributionDescriptor {
                        name: export.id.clone(),
                        entry: export.entry.clone(),
                        default_width,
                        default_height,
                        settings: export.instance_settings.clone(),
                    }
                })
                .collect(),
            services: contributes
                .services
                .iter()
                .map(|export| ServiceContributionDescriptor {
                    name: export.id.clone(),
                    methods: export.methods.clone(),
                })
                .collect(),
            views: Vec::new(),
            assets: Vec::new(),
            schemas: Vec::new(),
            pages: Vec::new(),
            overlays: Vec::new(),
            webviews: Vec::new(),
            configuration: None,
        },
        compatibility,
        settings: manifest.settings().map(ToOwned::to_owned),
        has_public_settings,
        has_secrets,
        error: None,
    }
}

fn package_settings_capabilities(manifest: &PluginPackageManifest, path: &str) -> (bool, bool) {
    let settings_path = manifest.settings().map(ToOwned::to_owned);
    let Some(settings_path) = settings_path else {
        return (false, false);
    };
    let Ok(schema) = read_json_package_file(Path::new(path), &settings_path) else {
        return (false, false);
    };
    let secret_keys = secret_key_set(Some(&schema));
    let has_public_settings = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .map(|properties| properties.keys().any(|key| !secret_keys.contains(key)))
        .or_else(|| {
            schema
                .get("fields")
                .and_then(serde_json::Value::as_array)
                .map(|fields| !fields.is_empty())
        })
        .unwrap_or(false);
    (has_public_settings, !secret_keys.is_empty())
}

pub(super) fn compatibility_for_manifest(
    manifest: &PluginPackageManifest,
) -> PackageCompatibilityDescriptor {
    let host_runtime_api = parse_runtime_api_version(HOST_RUNTIME_API_VERSION)
        .expect("HOST_RUNTIME_API_VERSION must be a semver version");
    let bakingrl_api = manifest.bakingrl_api();
    let (status, message) = match parse_runtime_api_version(bakingrl_api) {
        None => (
            PackageCompatibilityStatus::UnknownRuntimeApi,
            Some(
                "Package has an invalid bakingrlApi; rebuild it with the current SDK."
                    .to_string(),
            ),
        ),
        Some((major, _, _)) if major == host_runtime_api.0 => {
            (PackageCompatibilityStatus::Compatible, None)
        }
        Some((major, _, _)) if major < host_runtime_api.0 => (
            PackageCompatibilityStatus::Incompatible,
            Some(format!(
                "Package targets runtime API {bakingrl_api}; update the package to {HOST_RUNTIME_API_VERSION}."
            )),
        ),
        Some(_) => (
            PackageCompatibilityStatus::RequiresNewerHost,
            Some(format!(
                "Package targets runtime API {bakingrl_api}; update BakingRL to a host that supports it."
            )),
        ),
    };
    PackageCompatibilityDescriptor {
        status,
        bakingrl_api: Some(bakingrl_api.to_string()),
        sdk: None,
        host_runtime_api: HOST_RUNTIME_API_VERSION.to_string(),
        supported_runtime_api: HOST_RUNTIME_API_VERSION.to_string(),
        message,
    }
}

pub(super) fn apply_graph_diagnostics(records: &mut HashMap<String, PackageRecord>) {
    let _ = records;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4_manifest(raw: serde_json::Value) -> PluginPackageManifest {
        PluginPackageManifest::parse(&raw.to_string()).unwrap()
    }

    #[test]
    fn descriptor_derives_catalog_from_v4_contributes() {
        let manifest = v4_manifest(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.catalog",
            "name": "Catalog",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "runtime": {
                "node": {
                    "entry": "dist/runtime.js"
                },
                "sidecars": [
                    {
                        "id": "stats",
                        "bin": "dist/stats.js",
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {
                "commands": [
                    {
                        "id": "openSettings",
                        "title": "Open settings"
                    }
                ],
                "visuals": [
                    {
                        "id": "scoreboard",
                        "entry": "dist/visuals/scoreboard.js",
                        "defaultSize": [760, 128],
                        "instanceSettings": "schemas/scoreboard-settings.json"
                    }
                ],
                "services": [
                    {
                        "id": "matchStats",
                        "runtime": "node",
                        "methods": ["snapshot"]
                    }
                ],
                "settings": {
                    "schema": "schemas/plugin-settings.json"
                    }
            }
        }));

        let descriptor =
            descriptor_for_manifest(&manifest, "/tmp/com.example.catalog".to_string(), true);

        assert!(descriptor.enabled);
        assert_eq!(
            descriptor.compatibility.status,
            PackageCompatibilityStatus::Compatible
        );
        assert_eq!(
            descriptor.compatibility.bakingrl_api.as_deref(),
            Some("2.0.0")
        );
        assert_eq!(
            descriptor
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.node.as_ref())
                .map(|node| node.entry.as_str()),
            Some("dist/runtime.js")
        );
        assert_eq!(
            descriptor
                .runtime
                .as_ref()
                .map(|runtime| runtime.sidecars.len()),
            Some(1)
        );
        assert_eq!(descriptor.contributions.visuals[0].name, "scoreboard");
        assert_eq!(
            descriptor.contributions.visuals[0].entry,
            "dist/visuals/scoreboard.js"
        );
        assert_eq!(descriptor.contributions.visuals[0].default_width, 760.0);
        assert_eq!(descriptor.contributions.commands[0].name, "openSettings");
        assert_eq!(descriptor.contributions.services[0].name, "matchStats");
        assert_eq!(
            descriptor.contributions.services[0].methods,
            vec!["snapshot"]
        );
        assert!(descriptor.contributions.pages.is_empty());
        assert!(descriptor.contributions.overlays.is_empty());
        assert_eq!(
            descriptor.settings.as_deref(),
            Some("schemas/plugin-settings.json")
        );
        assert!(descriptor.contributions.views.is_empty());
        assert!(descriptor
            .contributes
            .as_ref()
            .and_then(|value| value.get("visuals"))
            .is_some());
    }

    #[test]
    fn package_settings_capabilities_reads_v4_settings_schema() {
        let package_root = std::env::temp_dir()
            .join("brl-descriptors-settings-capabilities")
            .join("v4");
        let schema_path = package_root.join("schemas").join("plugin-settings.json");
        let _ = std::fs::remove_dir_all(&package_root);
        std::fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
        std::fs::write(
            &schema_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "type": "object",
                "properties": {
                    "publicField": {
                        "type": "string"
                    },
                    "apiToken": {
                        "type": "string",
                        "x-bakingrl-secret": true
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let manifest = v4_manifest(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.catalog",
            "name": "Catalog",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "contributes": {
                "settings": {
                    "schema": "schemas/plugin-settings.json"
                }
            }
        }));
        let (has_public_settings, has_secrets) =
            package_settings_capabilities(&manifest, &package_root.to_string_lossy());

        assert!(has_public_settings);
        assert!(has_secrets);
        let _ = std::fs::remove_dir_all(&package_root);
    }
}
