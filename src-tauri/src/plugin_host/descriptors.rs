use std::collections::HashMap;
use std::path::Path;

use crate::plugin_package::bundle::BundleInspection;
use crate::plugin_package::manifest::{
    parse_runtime_api_version, PluginActivationV3, PluginDiagnosticsV3, PluginPackageManifest,
    PluginRuntimeV3, PluginSafeModeV3, HOST_RUNTIME_API_RANGE, HOST_RUNTIME_API_VERSION,
};
use crate::plugin_package::permissions::EffectivePackagePermissions;

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
    pub kind: Option<String>,
    pub activation: Option<PluginActivationV3>,
    pub runtime: Option<PluginRuntimeV3>,
    pub contributes: Option<serde_json::Value>,
    pub capabilities: Option<serde_json::Value>,
    pub diagnostics: Option<PluginDiagnosticsV3>,
    #[serde(rename = "safeMode")]
    pub safe_mode: Option<PluginSafeModeV3>,
    pub enabled: bool,
    pub status: PackageStatus,
    pub path: String,
    pub contributions: PackageContributionsDescriptor,
    pub effective_permissions: EffectivePackagePermissions,
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
    pub runtime_api: Option<String>,
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
    effective_permissions: EffectivePackagePermissions,
) -> PackageDescriptor {
    let compatibility = compatibility_for_manifest(manifest);
    let enabled = enabled && compatibility.is_compatible();
    let (has_public_settings, has_secrets) = package_settings_capabilities(manifest, &path);
    let contributes = manifest.normalized_contributes_v3();
    PackageDescriptor {
        manifest_schema: manifest.manifest_schema().to_string(),
        id: manifest.id().to_string(),
        name: manifest.name().to_string(),
        version: manifest.version().to_string(),
        author: manifest.author().map(ToOwned::to_owned),
        kind: manifest.kind().map(ToOwned::to_owned),
        activation: manifest.activation().cloned(),
        runtime: manifest.runtime().cloned(),
        contributes: manifest.contributes_value(),
        capabilities: manifest.capabilities().cloned(),
        diagnostics: manifest.diagnostics().cloned(),
        safe_mode: manifest.safe_mode().cloned(),
        enabled,
        status: PackageStatus::Installed,
        path,
        contributions: PackageContributionsDescriptor {
            commands: contributes
                .commands
                .keys()
                .map(|name| NamedContributionDescriptor { name: name.clone() })
                .collect(),
            visuals: contributes
                .visuals
                .iter()
                .map(|(name, export)| {
                    let [default_width, default_height] =
                        export.default_size.unwrap_or([320.0, 120.0]);
                    VisualContributionDescriptor {
                        name: name.clone(),
                        entry: export.entry.clone(),
                        default_width,
                        default_height,
                        settings: export.settings.clone(),
                    }
                })
                .collect(),
            services: contributes
                .services
                .iter()
                .map(|(name, export)| ServiceContributionDescriptor {
                    name: name.clone(),
                    methods: export.methods.clone(),
                })
                .collect(),
            views: contributes
                .views
                .iter()
                .map(|(name, export)| WebviewContributionDescriptor {
                    name: name.clone(),
                    entry: export.entry.clone(),
                    path: export.path.clone(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                    icon: export.icon.clone(),
                    configuration: export.configuration.clone(),
                    route: export.route.clone(),
                })
                .collect(),
            assets: contributes
                .assets
                .keys()
                .map(|name| NamedContributionDescriptor { name: name.clone() })
                .collect(),
            schemas: contributes
                .schemas
                .keys()
                .map(|name| NamedContributionDescriptor { name: name.clone() })
                .collect(),
            pages: contributes
                .pages
                .iter()
                .map(|(name, export)| PageContributionDescriptor {
                    name: name.clone(),
                    path: export
                        .path
                        .clone()
                        .or_else(|| export.entry.clone())
                        .unwrap_or_default(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                })
                .collect(),
            overlays: contributes
                .overlays
                .iter()
                .map(|(name, export)| OverlayContributionDescriptor {
                    name: name.clone(),
                    path: export
                        .path
                        .clone()
                        .or_else(|| export.entry.clone())
                        .unwrap_or_default(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                })
                .collect(),
            webviews: contributes
                .webviews
                .iter()
                .map(|(name, export)| WebviewContributionDescriptor {
                    name: name.clone(),
                    entry: export.entry.clone(),
                    path: export.path.clone(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                    icon: export.icon.clone(),
                    configuration: export.configuration.clone(),
                    route: export.route.clone(),
                })
                .collect(),
            configuration: contributes
                .configuration
                .values()
                .find(|configuration| configuration.path.is_some())
                .map(|configuration| ConfigurationContributionDescriptor {
                    title: configuration.title.clone(),
                    description: configuration.description.clone(),
                    path: configuration.path.clone().unwrap_or_default(),
                    visuals: configuration
                        .visuals
                        .iter()
                        .map(|(name, export)| {
                            let [default_width, default_height] =
                                export.default_size.unwrap_or([1200.0, 740.0]);
                            VisualContributionDescriptor {
                                name: name.clone(),
                                entry: export.entry.clone(),
                                default_width,
                                default_height,
                                settings: export.settings.clone(),
                            }
                        })
                        .collect(),
                }),
        },
        effective_permissions,
        compatibility,
        settings: manifest.settings().map(ToOwned::to_owned).or_else(|| {
            manifest
                .normalized_contributes_v3()
                .configuration
                .values()
                .next()
                .map(|configuration| configuration.schema.clone())
        }),
        has_public_settings,
        has_secrets,
        error: None,
    }
}

fn package_settings_capabilities(manifest: &PluginPackageManifest, path: &str) -> (bool, bool) {
    let settings_path = manifest.settings().map(ToOwned::to_owned).or_else(|| {
        manifest
            .normalized_contributes_v3()
            .configuration
            .values()
            .next()
            .map(|configuration| configuration.schema.clone())
    });
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
    let runtime_api = manifest
        .compatibility()
        .and_then(|compatibility| compatibility.runtime_api.clone());
    let sdk = manifest
        .compatibility()
        .and_then(|compatibility| compatibility.sdk.clone());
    let (status, message) = match runtime_api
        .as_deref()
        .and_then(parse_runtime_api_version)
    {
        None => (
            PackageCompatibilityStatus::UnknownRuntimeApi,
            Some("Package does not declare compatibility.runtimeApi; rebuild it with the current SDK.".to_string()),
        ),
        Some((major, _, _)) if major == host_runtime_api.0 => {
            (PackageCompatibilityStatus::Compatible, None)
        }
        Some((major, _, _)) if major < host_runtime_api.0 => (
            PackageCompatibilityStatus::Incompatible,
            Some(format!(
                "Package targets runtime API {}; update the package to {}.",
                runtime_api.as_deref().unwrap_or("unknown"),
                HOST_RUNTIME_API_VERSION
            )),
        ),
        Some(_) => (
            PackageCompatibilityStatus::RequiresNewerHost,
            Some(format!(
                "Package targets runtime API {}; update BakingRL to a host that supports it.",
                runtime_api.as_deref().unwrap_or("unknown")
            )),
        ),
    };
    PackageCompatibilityDescriptor {
        status,
        runtime_api,
        sdk,
        host_runtime_api: HOST_RUNTIME_API_VERSION.to_string(),
        supported_runtime_api: HOST_RUNTIME_API_RANGE.to_string(),
        message,
    }
}

pub(super) fn apply_graph_diagnostics(records: &mut HashMap<String, PackageRecord>) {
    let _ = records;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3_manifest(raw: serde_json::Value) -> PluginPackageManifest {
        PluginPackageManifest::parse(&raw.to_string()).unwrap()
    }

    #[test]
    fn descriptor_derives_catalog_from_v3_contributes() {
        let manifest = v3_manifest(serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.catalog",
            "name": "Catalog",
            "version": "1.0.0",
            "contributes": {
                "visuals": {
                    "scoreboard": {
                        "entry": "dist/visuals/scoreboard.js",
                        "defaultSize": [760, 128],
                        "settings": "schemas/scoreboard-settings.json"
                    }
                },
                "services": {
                    "matchStats": {
                        "entry": "dist/services/match-stats.js",
                        "methods": ["snapshot"]
                    }
                },
                "pages": {
                    "dashboard": {
                        "path": "pages/dashboard.json",
                        "title": "Dashboard"
                    }
                },
                "overlays": {
                    "stream": {
                        "path": "overlays/stream.json",
                        "title": "Stream"
                    }
                }
            },
            "compatibility": {
                "runtimeApi": "1.0.0"
            }
        }));

        let descriptor = descriptor_for_manifest(
            &manifest,
            "/tmp/com.example.catalog".to_string(),
            true,
            EffectivePackagePermissions::default(),
        );

        assert!(descriptor.enabled);
        assert_eq!(
            descriptor.compatibility.status,
            PackageCompatibilityStatus::Compatible
        );
        assert_eq!(
            descriptor.compatibility.runtime_api.as_deref(),
            Some("1.0.0")
        );
        assert_eq!(descriptor.contributions.visuals[0].name, "scoreboard");
        assert_eq!(
            descriptor.contributions.visuals[0].entry,
            "dist/visuals/scoreboard.js"
        );
        assert_eq!(descriptor.contributions.visuals[0].default_width, 760.0);
        assert_eq!(descriptor.contributions.services[0].name, "matchStats");
        assert_eq!(
            descriptor.contributions.services[0].methods,
            vec!["snapshot"]
        );
        assert_eq!(
            descriptor.contributions.pages[0].path,
            "pages/dashboard.json"
        );
        assert_eq!(
            descriptor.contributions.overlays[0].path,
            "overlays/stream.json"
        );
        assert!(descriptor
            .contributes
            .as_ref()
            .and_then(|value| value.get("visuals"))
            .is_some());
    }
}
