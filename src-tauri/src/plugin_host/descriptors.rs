use std::collections::HashMap;

use crate::plugin_v2::bundle::BundleInspection;
use crate::plugin_v2::manifest::PluginPackageManifestV2;
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;

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
    pub settings: Option<String>,
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
