use std::collections::HashMap;
use std::path::Path;

use crate::plugin_v2::bundle::BundleInspection;
use crate::plugin_v2::manifest::{
    parse_runtime_api_version, PluginPackageManifestV2, HOST_RUNTIME_API_RANGE,
    HOST_RUNTIME_API_VERSION,
};
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;

use super::package_files::read_json_package_file;
use super::settings_contract::secret_key_set;
use super::PackageRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PackageDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub enabled: bool,
    pub status: PackageStatus,
    pub path: String,
    pub exports: PackageExportsDescriptor,
    pub imports: PackageImportsDescriptor,
    pub effective_permissions: EffectivePackagePermissionsV2,
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
pub struct PackageExportsDescriptor {
    pub visuals: Vec<VisualExportDescriptor>,
    pub components: Vec<NamedExportDescriptor>,
    pub services: Vec<ServiceExportDescriptor>,
    pub connectors: Vec<NamedExportDescriptor>,
    pub assets: Vec<NamedExportDescriptor>,
    pub schemas: Vec<NamedExportDescriptor>,
    pub pages: Vec<PageExportDescriptor>,
    pub layouts: Vec<LayoutTemplateExportDescriptor>,
    pub configuration: Option<ConfigurationExportDescriptor>,
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
pub struct LayoutTemplateExportDescriptor {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfigurationExportDescriptor {
    pub title: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub visuals: Vec<VisualExportDescriptor>,
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

pub(super) fn descriptor_for_manifest(
    manifest: &PluginPackageManifestV2,
    path: String,
    enabled: bool,
    effective_permissions: EffectivePackagePermissionsV2,
) -> PackageDescriptor {
    let compatibility = compatibility_for_manifest(manifest);
    let enabled = enabled && compatibility.is_compatible();
    let (has_public_settings, has_secrets) = package_settings_capabilities(manifest, &path);
    PackageDescriptor {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        enabled,
        status: PackageStatus::Installed,
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
            layouts: manifest
                .exports
                .layouts
                .iter()
                .map(|(name, export)| LayoutTemplateExportDescriptor {
                    name: name.clone(),
                    path: export.path.clone(),
                    title: export.title.clone(),
                    description: export.description.clone(),
                })
                .collect(),
            configuration: manifest
                .exports
                .configuration
                .as_ref()
                .map(|configuration| ConfigurationExportDescriptor {
                    title: configuration.title.clone(),
                    description: configuration.description.clone(),
                    path: configuration.path.clone(),
                    visuals: configuration
                        .visuals
                        .iter()
                        .map(|(name, export)| {
                            let [default_width, default_height] =
                                export.default_size.unwrap_or([1200.0, 740.0]);
                            VisualExportDescriptor {
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
        imports: PackageImportsDescriptor {
            components: manifest.imports.components.clone(),
            services: manifest.imports.services.clone(),
        },
        effective_permissions,
        compatibility,
        settings: manifest.settings.clone(),
        has_public_settings,
        has_secrets,
        error: None,
    }
}

fn package_settings_capabilities(manifest: &PluginPackageManifestV2, path: &str) -> (bool, bool) {
    let Some(settings_path) = manifest.settings.as_deref() else {
        return (false, false);
    };
    let Ok(schema) = read_json_package_file(Path::new(path), settings_path) else {
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
    manifest: &PluginPackageManifestV2,
) -> PackageCompatibilityDescriptor {
    let host_runtime_api = parse_runtime_api_version(HOST_RUNTIME_API_VERSION)
        .expect("HOST_RUNTIME_API_VERSION must be a semver version");
    let host_runtime_api_minor = (host_runtime_api.0, host_runtime_api.1);
    let runtime_api = manifest
        .compatibility
        .as_ref()
        .and_then(|compatibility| compatibility.runtime_api.clone());
    let sdk = manifest
        .compatibility
        .as_ref()
        .and_then(|compatibility| compatibility.sdk.clone());
    let (status, message) = match runtime_api
        .as_deref()
        .and_then(parse_runtime_api_version)
    {
        None => (
            PackageCompatibilityStatus::UnknownRuntimeApi,
            Some("Package does not declare compatibility.runtimeApi; rebuild it with the current SDK.".to_string()),
        ),
        Some((major, minor, _)) if (major, minor) == host_runtime_api_minor => {
            (PackageCompatibilityStatus::Compatible, None)
        }
        Some((major, minor, _)) if (major, minor) < host_runtime_api_minor => (
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
