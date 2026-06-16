use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::models::PackageSettingsFile;
use crate::plugin_package::manifest::{
    PluginPackageManifest, PluginRuntimeSidecarActivationV3, PluginRuntimeSidecarProtocolV3,
    PluginRuntimeSidecarV3,
};

use super::extension_host_runtime::{ExtensionHostRuntimeSpec, ExtensionHostWebviewSpec};
use super::sidecar_runtime::{SidecarProtocol, SidecarRuntimeSpec};
use super::{merge_settings, PackageRecord};

fn service_methods_for_records(
    records: &HashMap<String, PackageRecord>,
) -> HashMap<String, Vec<String>> {
    records
        .values()
        .flat_map(|record| {
            let contributes = record.manifest.normalized_contributes_v3();
            contributes
                .services
                .iter()
                .map(|(name, export)| {
                    (
                        format!("{}/{}", record.manifest.id(), name),
                        export.methods.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn extension_host_specs_for_records(
    records: &HashMap<String, PackageRecord>,
    package_settings: &PackageSettingsFile,
    package_settings_path: &Path,
) -> Vec<ExtensionHostRuntimeSpec> {
    let service_methods = service_methods_for_records(records);
    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .filter_map(|record| {
            let runtime = record.manifest.runtime()?;
            let extension_host = runtime.extension_host.as_ref()?;
            let entry = extension_host.entry.as_deref()?;
            let package_root = Path::new(&record.descriptor.path);
            let storage_root = package_root.join(".bakingrl").join("storage");
            let settings = merge_settings(
                record.descriptor.settings.as_deref(),
                package_root,
                package_settings
                    .values
                    .get(record.manifest.id())
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            );
            let contributes = record.manifest.normalized_contributes_v3();
            let webviews = contributes
                .webviews
                .iter()
                .map(|(name, webview)| {
                    (
                        name.clone(),
                        ExtensionHostWebviewSpec {
                            title: webview.title.clone(),
                            entry: webview.entry.clone(),
                            path: webview.path.clone(),
                            route: webview.route.clone(),
                        },
                    )
                })
                .collect();
            let mut sidecars = sidecar_specs_for_manifest(
                &record.manifest,
                package_root,
                runtime
                    .sidecars
                    .iter()
                    .chain(extension_host.sidecars.iter()),
                |_| true,
            );
            sidecars.sort_by(|a, b| a.sidecar_name.cmp(&b.sidecar_name));
            Some(ExtensionHostRuntimeSpec {
                package_id: record.manifest.id().to_string(),
                runtime_api: runtime_api_req(&record.manifest),
                package_root: package_root.to_path_buf(),
                entry_path: package_root.join(entry),
                storage_root,
                package_settings_path: package_settings_path.to_path_buf(),
                service_imports: Vec::new(),
                service_methods: service_methods.clone(),
                permissions: record.descriptor.effective_permissions.clone(),
                settings,
                sidecars,
                webviews,
                node_path: None,
                args: Vec::new(),
                env: HashMap::new(),
            })
        })
        .collect()
}

pub(super) fn sidecar_specs_for_records(
    records: &HashMap<String, PackageRecord>,
) -> Vec<SidecarRuntimeSpec> {
    let mut specs = Vec::new();
    for record in records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
    {
        let Some(runtime) = record.manifest.runtime() else {
            continue;
        };
        let package_root = Path::new(&record.descriptor.path);
        specs.extend(sidecar_specs_for_manifest(
            &record.manifest,
            package_root,
            runtime.sidecars.iter(),
            |sidecar| sidecar.activation == PluginRuntimeSidecarActivationV3::OnStartup,
        ));
        if let Some(extension_host) = runtime.extension_host.as_ref() {
            specs.extend(sidecar_specs_for_manifest(
                &record.manifest,
                package_root,
                extension_host.sidecars.iter(),
                |sidecar| sidecar.activation == PluginRuntimeSidecarActivationV3::OnStartup,
            ));
        }
    }
    specs.sort_by(|a, b| {
        a.package_id
            .cmp(&b.package_id)
            .then_with(|| a.sidecar_name.cmp(&b.sidecar_name))
    });
    specs.dedup_by(|a, b| a.package_id == b.package_id && a.sidecar_name == b.sidecar_name);
    specs
}

fn sidecar_specs_for_manifest<'a>(
    manifest: &PluginPackageManifest,
    package_root: &Path,
    sidecars: impl Iterator<Item = (&'a String, &'a PluginRuntimeSidecarV3)>,
    include: impl Fn(&PluginRuntimeSidecarV3) -> bool,
) -> Vec<SidecarRuntimeSpec> {
    sidecars
        .filter_map(|(sidecar_id, sidecar)| {
            if !include(sidecar) {
                return None;
            }
            let protocol = match sidecar.protocol {
                PluginRuntimeSidecarProtocolV3::JsonRpcStdio => SidecarProtocol::JsonRpcStdio,
            };
            let binary_path = sidecar_binary_path(package_root, sidecar)?;
            Some(SidecarRuntimeSpec {
                package_id: manifest.id().to_string(),
                sidecar_name: sidecar_id.clone(),
                runtime_api: runtime_api_req(manifest),
                package_root: package_root.to_path_buf(),
                binary_path,
                protocol,
                args: sidecar.args.clone(),
                env: sidecar
                    .env
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                activation: sidecar.activation.clone(),
            })
        })
        .collect()
}

fn sidecar_binary_path(package_root: &Path, sidecar: &PluginRuntimeSidecarV3) -> Option<PathBuf> {
    Some(package_root.join(&sidecar.command))
}

fn runtime_api_req(manifest: &PluginPackageManifest) -> Option<semver::VersionReq> {
    manifest
        .compatibility()
        .and_then(|compatibility| compatibility.runtime_api.as_deref())
        .and_then(|version| semver::VersionReq::parse(&format!("={version}")).ok())
}
