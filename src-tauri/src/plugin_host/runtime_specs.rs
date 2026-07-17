use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::PackageSettingsFile;
use crate::plugin_package::manifest::{
    PluginPackageManifest, PluginRuntimeSidecarActivationV4, PluginRuntimeSidecarProtocolV4,
    PluginRuntimeSidecarV4,
};

use super::extension_host_runtime::ExtensionHostRuntimeSpec;
use super::extension_host_runtime::ExtensionHostWebviewSpec;
use super::package_files::read_json_package_file;
use super::plugin_storage::PluginStorage;
use super::settings_contract::secret_key_set;
use super::sidecar_runtime::{SidecarProtocol, SidecarRuntimeSpec};
use super::{merge_settings, PackageRecord};

fn service_methods_for_records(
    records: &HashMap<String, PackageRecord>,
) -> HashMap<String, Vec<String>> {
    records
        .values()
        .flat_map(|record| {
            let contributes = record.manifest.contributes_v4();
            contributes
                .services
                .iter()
                .map(|service| {
                    (
                        format!("{}/{}", record.manifest.id(), service.id),
                        service.methods.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SidecarServiceRuntimeSpec {
    pub package_id: String,
    pub sidecar_name: String,
    pub methods: Vec<String>,
}

pub(crate) fn sidecar_service_specs_for_records(
    records: &HashMap<String, PackageRecord>,
) -> HashMap<String, SidecarServiceRuntimeSpec> {
    records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
        .filter_map(|record| {
            let runtime = record.manifest.runtime_v4()?;
            let sidecars = runtime
                .sidecars
                .iter()
                .map(|sidecar| sidecar.id.as_str())
                .collect::<Vec<_>>();
            let sidecar_services =
                record
                    .manifest
                    .contributes_v4()
                    .services
                    .iter()
                    .filter_map(|service| {
                        let runtime_ref = service
                            .runtime
                            .as_deref()
                            .and_then(extract_sidecar_runtime_id)?;
                        if !sidecars.contains(&runtime_ref) {
                            return None;
                        }
                        let service_ref = format!("{}/{}", record.manifest.id(), service.id);
                        Some((
                            service_ref,
                            SidecarServiceRuntimeSpec {
                                package_id: record.descriptor.id.clone(),
                                sidecar_name: runtime_ref.to_string(),
                                methods: service.methods.clone(),
                            },
                        ))
                    });
            Some(sidecar_services.collect::<Vec<_>>())
        })
        .flatten()
        .collect()
}

pub(super) fn extension_host_specs_for_records(
    records: &HashMap<String, PackageRecord>,
    package_settings: &PackageSettingsFile,
    package_settings_path: &Path,
    storage_base: &Path,
) -> Vec<ExtensionHostRuntimeSpec> {
    let service_methods = service_methods_for_records(records);
    ordered_extension_host_package_ids(records)
        .into_iter()
        .filter_map(|package_id| records.get(&package_id))
        .filter_map(|record| {
            let runtime = record.manifest.runtime_v4()?;
            let node = runtime.node.as_ref()?;
            let package_root = Path::new(&record.descriptor.path);
            let settings = merge_settings(
                record.descriptor.settings.as_deref(),
                package_root,
                package_settings
                    .values
                    .get(record.manifest.id())
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            );
            let mut sidecars = sidecar_specs_for_manifest(
                &record.manifest,
                package_root,
                runtime.sidecars.iter(),
                |_| true,
            );
            sidecars.sort_by(|a, b| a.sidecar_name.cmp(&b.sidecar_name));
            Some(ExtensionHostRuntimeSpec {
                package_id: record.manifest.id().to_string(),
                required_node_dependencies: required_node_dependencies(record, records),
                runtime_api: runtime_api_req(&record.manifest),
                package_root: package_root.to_path_buf(),
                entry_path: package_root.join(&node.entry),
                storage: PluginStorage::new(storage_base.join(record.manifest.id())),
                permissions: record
                    .manifest
                    .permissions_v4()
                    .cloned()
                    .unwrap_or_default(),
                package_settings_path: package_settings_path.to_path_buf(),
                secret_keys: secret_keys_for_package_settings(
                    record.descriptor.settings.as_deref(),
                    package_root,
                ),
                service_imports: Vec::new(),
                service_methods: service_methods.clone(),
                settings,
                sidecars,
                webviews: webview_specs_for_manifest(&record.manifest),
                node_path: None,
                args: Vec::new(),
                env: HashMap::new(),
            })
        })
        .collect()
}

fn required_node_dependencies(
    record: &PackageRecord,
    records: &HashMap<String, PackageRecord>,
) -> Vec<String> {
    let mut dependencies = record
        .manifest
        .dependencies_v4()
        .iter()
        .filter(|dependency| !dependency.optional)
        .filter_map(|dependency| {
            records
                .get(&dependency.package_id)
                .and_then(|record| record.manifest.runtime_v4())
                .and_then(|runtime| runtime.node.as_ref())
                .map(|_| dependency.package_id.clone())
        })
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn ordered_extension_host_package_ids(records: &HashMap<String, PackageRecord>) -> Vec<String> {
    let candidates = records
        .iter()
        .filter(|(_, record)| {
            record.descriptor.enabled
                && record.descriptor.error.is_none()
                && record
                    .manifest
                    .runtime_v4()
                    .and_then(|runtime| runtime.node.as_ref())
                    .is_some()
        })
        .map(|(package_id, _)| package_id.clone())
        .collect::<BTreeSet<_>>();
    let mut remaining_dependencies = candidates
        .iter()
        .map(|package_id| (package_id.clone(), 0usize))
        .collect::<HashMap<_, _>>();
    let mut dependents = HashMap::<String, BTreeSet<String>>::new();

    for package_id in &candidates {
        let Some(record) = records.get(package_id) else {
            continue;
        };
        for dependency in record
            .manifest
            .dependencies_v4()
            .iter()
            .filter(|dependency| !dependency.optional)
        {
            if !candidates.contains(&dependency.package_id) {
                continue;
            }
            *remaining_dependencies.get_mut(package_id).unwrap() += 1;
            dependents
                .entry(dependency.package_id.clone())
                .or_default()
                .insert(package_id.clone());
        }
    }

    let mut ready = remaining_dependencies
        .iter()
        .filter_map(|(package_id, remaining)| (*remaining == 0).then_some(package_id.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(candidates.len());
    while let Some(package_id) = ready.pop_first() {
        ordered.push(package_id.clone());
        let Some(package_dependents) = dependents.get(&package_id) else {
            continue;
        };
        for dependent in package_dependents {
            let remaining = remaining_dependencies.get_mut(dependent).unwrap();
            *remaining -= 1;
            if *remaining == 0 {
                ready.insert(dependent.clone());
            }
        }
    }

    ordered
}

fn secret_keys_for_package_settings(
    schema_path: Option<&str>,
    package_root: &Path,
) -> HashSet<String> {
    let Some(schema_path) = schema_path else {
        return HashSet::new();
    };
    read_json_package_file(package_root, schema_path)
        .map(|schema| secret_key_set(Some(&schema)))
        .unwrap_or_default()
}

pub(super) fn sidecar_specs_for_records(
    records: &HashMap<String, PackageRecord>,
) -> Vec<SidecarRuntimeSpec> {
    let mut specs = Vec::new();
    for record in records
        .values()
        .filter(|record| record.descriptor.enabled && record.descriptor.error.is_none())
    {
        let Some(runtime) = record.manifest.runtime_v4() else {
            continue;
        };
        let package_root = Path::new(&record.descriptor.path);
        let include_on_enable_sidecars = runtime.node.is_none();
        specs.extend(sidecar_specs_for_manifest(
            &record.manifest,
            package_root,
            runtime.sidecars.iter(),
            |sidecar| {
                if include_on_enable_sidecars {
                    sidecar.activation != PluginRuntimeSidecarActivationV4::Manual
                } else {
                    sidecar.activation == PluginRuntimeSidecarActivationV4::OnStartup
                }
            },
        ));
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
    sidecars: impl Iterator<Item = &'a PluginRuntimeSidecarV4>,
    include: impl Fn(&PluginRuntimeSidecarV4) -> bool,
) -> Vec<SidecarRuntimeSpec> {
    sidecars
        .filter_map(|sidecar| {
            if !include(sidecar) || !sidecar_supports_platform(sidecar, current_runtime_platform())
            {
                return None;
            }
            let protocol = match sidecar.protocol {
                PluginRuntimeSidecarProtocolV4::JsonRpcStdio => SidecarProtocol::JsonRpcStdio,
            };
            let binary_path = sidecar_binary_path(package_root, sidecar)?;
            Some(SidecarRuntimeSpec {
                package_id: manifest.id().to_string(),
                sidecar_name: sidecar.id.clone(),
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
                health_check: sidecar.health_check.clone(),
            })
        })
        .collect()
}

fn webview_specs_for_manifest(
    manifest: &PluginPackageManifest,
) -> HashMap<String, ExtensionHostWebviewSpec> {
    manifest
        .contributes_v4()
        .webviews
        .iter()
        .map(|webview| {
            (
                webview.id.clone(),
                ExtensionHostWebviewSpec {
                    title: webview.title.clone(),
                    entry: Some(webview.entry.clone()),
                    path: None,
                    route: None,
                    kind: webview.kind.clone(),
                    default_size: webview.default_size.unwrap_or([960.0, 640.0]),
                    surface: webview.surface.clone(),
                },
            )
        })
        .collect()
}

fn sidecar_binary_path(package_root: &Path, sidecar: &PluginRuntimeSidecarV4) -> Option<PathBuf> {
    Some(package_root.join(&sidecar.bin))
}

fn extract_sidecar_runtime_id(runtime_ref: &str) -> Option<&str> {
    runtime_ref
        .strip_prefix("sidecar:")
        .filter(|id| !id.is_empty())
}

fn current_runtime_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("windows", "x86_64") => "windows-x64",
        _ => "unknown",
    }
}

fn sidecar_supports_platform(sidecar: &PluginRuntimeSidecarV4, platform: &str) -> bool {
    sidecar.platforms.is_empty()
        || sidecar
            .platforms
            .iter()
            .any(|candidate| candidate == platform)
}

fn runtime_api_req(manifest: &PluginPackageManifest) -> Option<semver::VersionReq> {
    semver::VersionReq::parse(&format!("={}", manifest.bakingrl_api())).ok()
}

#[cfg(test)]
mod tests {
    use super::super::descriptors::descriptor_for_manifest;
    use super::*;

    fn manifest_record(raw: serde_json::Value, enabled: bool) -> (String, PackageRecord) {
        let raw_string = raw.to_string();
        let manifest = PluginPackageManifest::parse(&raw_string).unwrap();
        (
            manifest.id().to_string(),
            PackageRecord {
                manifest: manifest.clone(),
                descriptor: descriptor_for_manifest(&manifest, "/tmp".to_string(), enabled),
            },
        )
    }

    fn sidecar(platforms: Vec<&str>) -> PluginRuntimeSidecarV4 {
        PluginRuntimeSidecarV4 {
            id: "helper".to_string(),
            bin: "bin/helper".to_string(),
            args: Vec::new(),
            env: std::collections::BTreeMap::new(),
            platforms: platforms.into_iter().map(ToOwned::to_owned).collect(),
            protocol: PluginRuntimeSidecarProtocolV4::JsonRpcStdio,
            activation: PluginRuntimeSidecarActivationV4::OnStartup,
            health_check: None,
        }
    }

    fn node_manifest(package_id: &str, dependencies: Vec<&str>) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": package_id,
            "name": package_id,
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": dependencies
                .into_iter()
                .map(|dependency| serde_json::json!({ "packageId": dependency }))
                .collect::<Vec<_>>(),
            "runtime": {
                "node": { "entry": "dist/extension-host.js" }
            }
        })
    }

    #[test]
    fn sidecar_v4_without_platforms_supports_current_host() {
        assert!(sidecar_supports_platform(&sidecar(Vec::new()), "linux-x64"));
    }

    #[test]
    fn sidecar_platforms_must_match_current_host() {
        assert!(sidecar_supports_platform(
            &sidecar(vec!["darwin-arm64", "linux-x64"]),
            "linux-x64"
        ));
        assert!(!sidecar_supports_platform(
            &sidecar(vec!["darwin-arm64", "windows-x64"]),
            "linux-x64"
        ));
    }

    #[test]
    fn extension_host_specs_are_deterministic_and_dependency_first() {
        let (consumer_id, consumer) = manifest_record(
            node_manifest("com.example.a-consumer", vec!["com.example.z-provider"]),
            true,
        );
        let (independent_id, independent) =
            manifest_record(node_manifest("com.example.b-independent", Vec::new()), true);
        let (provider_id, provider) =
            manifest_record(node_manifest("com.example.z-provider", Vec::new()), true);
        let records = HashMap::from([
            (consumer_id, consumer),
            (provider_id, provider),
            (independent_id, independent),
        ]);

        let specs = extension_host_specs_for_records(
            &records,
            &PackageSettingsFile::default(),
            Path::new("/tmp/package-settings.json"),
            Path::new("/tmp/plugin-storage"),
        );

        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.package_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "com.example.b-independent",
                "com.example.z-provider",
                "com.example.a-consumer",
            ]
        );
        assert_eq!(
            specs
                .iter()
                .find(|spec| spec.package_id == "com.example.a-consumer")
                .unwrap()
                .required_node_dependencies,
            vec!["com.example.z-provider".to_string()]
        );
    }

    #[test]
    fn extension_host_spec_order_excludes_unresolved_dependency_cycles() {
        let (first_id, first) = manifest_record(
            node_manifest("com.example.a-cycle", vec!["com.example.b-cycle"]),
            true,
        );
        let (second_id, second) = manifest_record(
            node_manifest("com.example.b-cycle", vec!["com.example.a-cycle"]),
            true,
        );
        let records = HashMap::from([(second_id, second), (first_id, first)]);

        assert!(ordered_extension_host_package_ids(&records).is_empty());
    }

    #[test]
    fn optional_node_dependencies_do_not_create_hard_runtime_edges() {
        let (consumer_id, consumer) = manifest_record(
            serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "com.example.a-consumer",
                "name": "Consumer",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "dependencies": [{
                    "packageId": "com.example.z-provider",
                    "optional": true
                }],
                "runtime": {
                    "node": { "entry": "dist/extension-host.js" }
                }
            }),
            true,
        );
        let (provider_id, provider) =
            manifest_record(node_manifest("com.example.z-provider", Vec::new()), true);
        let records = HashMap::from([(consumer_id, consumer), (provider_id, provider)]);

        let specs = extension_host_specs_for_records(
            &records,
            &PackageSettingsFile::default(),
            Path::new("/tmp/package-settings.json"),
            Path::new("/tmp/plugin-storage"),
        );

        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.package_id.as_str())
                .collect::<Vec<_>>(),
            vec!["com.example.a-consumer", "com.example.z-provider"]
        );
        assert!(specs[0].required_node_dependencies.is_empty());
    }

    #[test]
    fn sidecar_specs_for_records_uses_on_enable_when_node_is_missing() {
        let manifest = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.sidecar-only",
            "name": "Sidecar Only",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "helper",
                        "bin": "bin/helper",
                        "activation": "onStartup",
                        "protocol": "jsonrpc-stdio"
                    },
                    {
                        "id": "events",
                        "bin": "bin/events",
                        "activation": "onEnable",
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            }
        });
        let (_, record_manifest) = manifest_record(manifest, true);
        let record = record_manifest;
        let specs = sidecar_specs_for_records(&HashMap::from([(
            "com.example.sidecar-only".to_string(),
            record,
        )]));

        let mut refs = specs
            .iter()
            .map(|spec| spec.sidecar_name.clone())
            .collect::<Vec<_>>();
        refs.sort();
        assert_eq!(refs, vec!["events".to_string(), "helper".to_string()]);
    }

    #[test]
    fn sidecar_services_for_records_only_registers_sidecar_only_plugins() {
        let sidecar_only = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.sidecar-only",
            "name": "Sidecar Only",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "sidecars": [
                    {
                        "id": "helper",
                        "bin": "bin/helper",
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {
                "services": [
                    {
                        "id": "stats",
                        "runtime": "sidecar:helper",
                        "methods": ["snapshot", "count"]
                    },
                    {
                        "id": "cache",
                        "runtime": "node",
                        "methods": ["clear"]
                    }
                ]
            }
        });
        let with_node = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.with-node",
            "name": "With Node",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "runtime": {
                "node": {"entry": "dist/extension-host.js"},
                "sidecars": [
                    {
                        "id": "legacy",
                        "bin": "bin/legacy",
                        "protocol": "jsonrpc-stdio"
                    }
                ]
            },
            "contributes": {
                "services": [
                    {
                        "id": "stats",
                        "runtime": "sidecar:legacy",
                        "methods": ["snapshot"]
                    }
                ]
            }
        });

        let (_, sidecar_only_record) = manifest_record(sidecar_only, true);
        let (_, with_node_record) = manifest_record(with_node, true);
        let records = HashMap::from([
            ("com.example.sidecar-only".to_string(), sidecar_only_record),
            ("com.example.with-node".to_string(), with_node_record),
        ]);

        let services = sidecar_service_specs_for_records(&records);
        assert_eq!(
            services.get("com.example.sidecar-only/stats"),
            Some(&SidecarServiceRuntimeSpec {
                package_id: "com.example.sidecar-only".to_string(),
                sidecar_name: "helper".to_string(),
                methods: vec!["snapshot".to_string(), "count".to_string()],
            })
        );
        assert_eq!(
            services.get("com.example.with-node/stats"),
            Some(&SidecarServiceRuntimeSpec {
                package_id: "com.example.with-node".to_string(),
                sidecar_name: "legacy".to_string(),
                methods: vec!["snapshot".to_string()],
            })
        );
    }

    #[test]
    fn extension_host_specs_include_declared_secret_keys() {
        let package_root = std::env::temp_dir()
            .join("brl-extension-host-secret-keys")
            .join(format!(
                "{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let schema_path = package_root.join("schemas").join("settings.json");
        std::fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
        std::fs::write(
            &schema_path,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "apiKey": {
                        "type": "string",
                        "x-bakingrl-secret": true
                    },
                    "theme": {
                        "type": "string",
                        "default": "dark"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let manifest = PluginPackageManifest::parse(
            &serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "com.example.secrets",
                "name": "Secrets",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "runtime": {
                    "node": {
                        "entry": "dist/extension-host.js"
                    }
                },
                "contributes": {
                    "settings": {
                        "schema": "schemas/settings.json"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        let record = PackageRecord {
            descriptor: descriptor_for_manifest(
                &manifest,
                package_root.to_string_lossy().to_string(),
                true,
            ),
            manifest,
        };

        let specs = extension_host_specs_for_records(
            &HashMap::from([("com.example.secrets".to_string(), record)]),
            &PackageSettingsFile::default(),
            &package_root.join("package_settings.json"),
            &package_root.join("plugin-storage"),
        );

        assert_eq!(specs.len(), 1);
        assert!(specs[0].secret_keys.contains("apiKey"));
        assert!(!specs[0].secret_keys.contains("theme"));

        let _ = std::fs::remove_dir_all(package_root);
    }
}
