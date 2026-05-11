use std::collections::HashMap;
use std::path::Path;

use crate::models::PluginRuntimeIsolation;

use super::connector_runtime::{ConnectorRuntimeModuleSpec, ConnectorRuntimeSpec};
use super::service_runtime::{ServiceRuntimeModuleSpec, ServiceRuntimeSpec};
use super::{merge_settings, PackageRecord};

fn service_methods_for_records(
    records: &HashMap<String, PackageRecord>,
) -> HashMap<String, Vec<String>> {
    records
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
        .collect()
}

pub(super) fn service_specs_for_records(
    records: &HashMap<String, PackageRecord>,
    runtime_isolation: &PluginRuntimeIsolation,
    package_settings: &HashMap<String, serde_json::Value>,
) -> Vec<ServiceRuntimeSpec> {
    let service_methods = service_methods_for_records(records);
    let mut specs = Vec::new();
    for record in records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
    {
        let storage_root = Path::new(&record.descriptor.path)
            .join(".bakingrl")
            .join("storage");
        let settings = merge_settings(
            record.descriptor.settings.as_deref(),
            Path::new(&record.descriptor.path),
            package_settings
                .get(&record.manifest.id)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        );
        match runtime_isolation {
            PluginRuntimeIsolation::Export => {
                for (name, export) in &record.manifest.exports.services {
                    specs.push(ServiceRuntimeSpec {
                        package_id: record.manifest.id.clone(),
                        service_name: name.clone(),
                        entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                        storage_root: storage_root.clone(),
                        service_imports: record.manifest.imports.services.clone(),
                        service_methods: service_methods.clone(),
                        permissions: record.descriptor.effective_permissions.clone(),
                        settings: settings.clone(),
                        additional_modules: Vec::new(),
                    });
                }
            }
            PluginRuntimeIsolation::Package => {
                let mut services = record.manifest.exports.services.iter();
                let Some((name, export)) = services.next() else {
                    continue;
                };
                let additional_modules = services
                    .map(|(service_name, service_export)| ServiceRuntimeModuleSpec {
                        service_name: service_name.clone(),
                        entry_path: Path::new(&record.descriptor.path).join(&service_export.entry),
                    })
                    .collect();
                specs.push(ServiceRuntimeSpec {
                    package_id: record.manifest.id.clone(),
                    service_name: name.clone(),
                    entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                    storage_root,
                    service_imports: record.manifest.imports.services.clone(),
                    service_methods: service_methods.clone(),
                    permissions: record.descriptor.effective_permissions.clone(),
                    settings,
                    additional_modules,
                });
            }
        }
    }
    specs
}

pub(super) fn connector_specs_for_records(
    records: &HashMap<String, PackageRecord>,
    runtime_isolation: &PluginRuntimeIsolation,
    package_settings: &HashMap<String, serde_json::Value>,
) -> Vec<ConnectorRuntimeSpec> {
    let service_methods = service_methods_for_records(records);
    let mut specs = Vec::new();
    for record in records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
    {
        let storage_root = Path::new(&record.descriptor.path)
            .join(".bakingrl")
            .join("storage");
        let settings = merge_settings(
            record.descriptor.settings.as_deref(),
            Path::new(&record.descriptor.path),
            package_settings
                .get(&record.manifest.id)
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        );
        match runtime_isolation {
            PluginRuntimeIsolation::Export => {
                for (name, export) in &record.manifest.exports.connectors {
                    specs.push(ConnectorRuntimeSpec {
                        package_id: record.manifest.id.clone(),
                        connector_name: name.clone(),
                        entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                        storage_root: storage_root.clone(),
                        service_imports: record.manifest.imports.services.clone(),
                        service_methods: service_methods.clone(),
                        permissions: record.descriptor.effective_permissions.clone(),
                        settings: settings.clone(),
                        additional_modules: Vec::new(),
                    });
                }
            }
            PluginRuntimeIsolation::Package => {
                let mut connectors = record.manifest.exports.connectors.iter();
                let Some((name, export)) = connectors.next() else {
                    continue;
                };
                let additional_modules = connectors
                    .map(
                        |(connector_name, connector_export)| ConnectorRuntimeModuleSpec {
                            connector_name: connector_name.clone(),
                            entry_path: Path::new(&record.descriptor.path)
                                .join(&connector_export.entry),
                        },
                    )
                    .collect();
                specs.push(ConnectorRuntimeSpec {
                    package_id: record.manifest.id.clone(),
                    connector_name: name.clone(),
                    entry_path: Path::new(&record.descriptor.path).join(&export.entry),
                    storage_root,
                    service_imports: record.manifest.imports.services.clone(),
                    service_methods: service_methods.clone(),
                    permissions: record.descriptor.effective_permissions.clone(),
                    settings,
                    additional_modules,
                });
            }
        }
    }
    specs
}
