use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::plugin_package::bundle::BundleInspection;
use crate::plugin_package::manifest::{
    parse_runtime_api_version, PluginContributesV4, PluginPackageManifest, PluginRuntimeV4,
    HOST_RUNTIME_API_VERSION,
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
    pub dependencies: Vec<PackageDependencyDescriptor>,
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
    pub extension_points: Vec<ExtensionPointContributionDescriptor>,
    pub contributions: Vec<PluginContributionDescriptor>,
    pub resources: Vec<ResourceContributionDescriptor>,
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
pub struct ExtensionPointContributionDescriptor {
    pub name: String,
    pub version: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub schema: Option<String>,
    pub service: Option<String>,
    pub reference: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginContributionDescriptor {
    pub name: String,
    pub target: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub data_schema: Option<String>,
    pub visual: Option<String>,
    pub service: Option<String>,
    pub resources: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceContributionDescriptor {
    pub name: String,
    pub paths: Vec<String>,
    pub resource_type: Option<String>,
    pub visibility: String,
    pub public: bool,
    pub metadata: Option<serde_json::Value>,
    pub reference: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageDependencyStatus {
    Pending,
    Satisfied,
    OptionalMissing,
    Missing,
    Disabled,
    Incompatible,
    VersionMismatch,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PackageDependencyDescriptor {
    pub package_id: String,
    pub version: Option<String>,
    pub optional: bool,
    pub status: PackageDependencyStatus,
    pub message: Option<String>,
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
    let configuration = manifest
        .settings_ui_visual()
        .and_then(|ui| configuration_descriptor_for_visuals(ui, &contributes.visuals));
    PackageDescriptor {
        manifest_schema: manifest.manifest_schema().to_string(),
        id: manifest.id().to_string(),
        name: manifest.name().to_string(),
        version: manifest.version().to_string(),
        author: manifest.author().map(ToOwned::to_owned),
        runtime: manifest.runtime_v4().cloned(),
        contributes: manifest.contributes_value(),
        dependencies: manifest
            .dependencies_v4()
            .iter()
            .map(|dependency| PackageDependencyDescriptor {
                package_id: dependency.package_id.clone(),
                version: dependency.version.clone(),
                optional: dependency.optional,
                status: PackageDependencyStatus::Pending,
                message: None,
            })
            .collect(),
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
            extension_points: extension_point_descriptors(manifest.id(), contributes),
            contributions: contributes
                .contributions
                .iter()
                .map(|contribution| PluginContributionDescriptor {
                    name: contribution.id.clone(),
                    target: contribution.target.clone(),
                    kind: contribution.kind.clone(),
                    title: contribution.title.clone(),
                    description: contribution.description.clone(),
                    data_schema: contribution.data_schema.clone(),
                    visual: contribution.visual.clone(),
                    service: contribution.service.clone(),
                    resources: contribution.resources.clone(),
                    metadata: contribution.metadata.clone(),
                })
                .collect(),
            resources: resource_descriptors(manifest.id(), contributes),
            views: Vec::new(),
            assets: Vec::new(),
            schemas: Vec::new(),
            pages: Vec::new(),
            overlays: Vec::new(),
            webviews: contributes
                .webviews
                .iter()
                .map(|webview| WebviewContributionDescriptor {
                    name: webview.id.clone(),
                    entry: Some(webview.entry.clone()),
                    path: None,
                    title: webview.title.clone(),
                    description: None,
                    icon: None,
                    configuration: None,
                    route: None,
                })
                .collect(),
            configuration,
        },
        compatibility,
        settings: manifest.settings().map(ToOwned::to_owned),
        has_public_settings: has_public_settings || manifest.settings_ui_visual().is_some(),
        has_secrets,
        error: None,
    }
}

fn extension_point_descriptors(
    package_id: &str,
    contributes: &PluginContributesV4,
) -> Vec<ExtensionPointContributionDescriptor> {
    contributes
        .extension_points
        .iter()
        .map(|point| ExtensionPointContributionDescriptor {
            name: point.id.clone(),
            version: point.version.clone(),
            title: point.title.clone(),
            description: point.description.clone(),
            schema: point.schema.clone(),
            service: point.service.clone(),
            reference: format!("{package_id}/{}", point.id),
        })
        .collect()
}

fn resource_descriptors(
    package_id: &str,
    contributes: &PluginContributesV4,
) -> Vec<ResourceContributionDescriptor> {
    contributes
        .resources
        .iter()
        .map(|resource| {
            let paths = resource
                .path
                .iter()
                .cloned()
                .chain(resource.paths.iter().cloned())
                .collect::<Vec<_>>();
            let visibility = match resource.visibility {
                crate::plugin_package::manifest::PluginResourceVisibilityV4::Public => "public",
                crate::plugin_package::manifest::PluginResourceVisibilityV4::Private => "private",
            };
            ResourceContributionDescriptor {
                name: resource.id.clone(),
                paths,
                resource_type: resource.resource_type.clone(),
                visibility: visibility.to_string(),
                public: visibility == "public",
                metadata: resource.metadata.clone(),
                reference: format!("{package_id}/{}", resource.id),
            }
        })
        .collect()
}

fn configuration_descriptor_for_visuals(
    ui: &str,
    visuals: &[crate::plugin_package::manifest::PluginVisualContributionV4],
) -> Option<ConfigurationContributionDescriptor> {
    let visual = visuals
        .iter()
        .find(|visual| visual.id == ui && visual.kind.as_deref() == Some("config"))?;
    let [default_width, default_height] = visual.default_size.unwrap_or([1200.0, 740.0]);
    Some(ConfigurationContributionDescriptor {
        title: None,
        description: None,
        path: visual.entry.clone(),
        visuals: vec![VisualContributionDescriptor {
            name: visual.id.clone(),
            entry: visual.entry.clone(),
            default_width,
            default_height,
            settings: visual.instance_settings.clone(),
        }],
    })
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
    let dependency_snapshot = graph_snapshot(records);
    for record in records.values_mut() {
        let mut graph_error = None;

        for dependency_descriptor in &mut record.descriptor.dependencies {
            let (status, message) = dependency_status(dependency_descriptor, &dependency_snapshot);
            dependency_descriptor.status = status.clone();
            dependency_descriptor.message = message.clone();
            if !dependency_descriptor.optional
                && !matches!(status, PackageDependencyStatus::Satisfied)
                && graph_error.is_none()
            {
                graph_error = message;
            }
        }

        if let Some(error) = graph_error {
            record.descriptor.enabled = false;
            record.descriptor.error = Some(error);
        }
    }

    let contribution_snapshot = graph_snapshot(records);
    for record in records.values_mut() {
        if record.descriptor.error.is_some() {
            continue;
        }
        let package_id = record.manifest.id().to_string();
        let dependency_package_ids = record
            .manifest
            .dependencies_v4()
            .iter()
            .map(|dependency| dependency.package_id.as_str())
            .collect::<HashSet<_>>();
        let mut graph_error = None;

        for contribution in &record.manifest.contributes_v4().contributions {
            let Some((target_package_id, target_extension_point)) =
                contribution.target.split_once('/')
            else {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' has invalid target '{}'.",
                        contribution.id, contribution.target
                    )
                });
                continue;
            };
            if target_package_id != package_id
                && !dependency_package_ids.contains(target_package_id)
            {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' targets '{}' but the package does not declare a dependency on '{}'.",
                        contribution.id, contribution.target, target_package_id
                    )
                });
                continue;
            }
            let Some(target) = contribution_snapshot.get(target_package_id) else {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' targets missing package '{}'.",
                        contribution.id, target_package_id
                    )
                });
                continue;
            };
            if !target.enabled {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' targets disabled package '{}'.",
                        contribution.id, target_package_id
                    )
                });
                continue;
            }
            if !target.compatible {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' targets incompatible package '{}'.",
                        contribution.id, target_package_id
                    )
                });
                continue;
            }
            if !target
                .extension_points
                .iter()
                .any(|point| point == target_extension_point)
            {
                graph_error.get_or_insert_with(|| {
                    format!(
                        "Contribution '{}' targets unknown extension point '{}'.",
                        contribution.id, contribution.target
                    )
                });
            }
        }

        if let Some(error) = graph_error {
            record.descriptor.enabled = false;
            record.descriptor.error = Some(error);
        }
    }
}

fn graph_snapshot(
    records: &HashMap<String, PackageRecord>,
) -> HashMap<String, GraphPackageSnapshot> {
    records
        .iter()
        .map(|(package_id, record)| {
            (
                package_id.clone(),
                GraphPackageSnapshot {
                    version: record.manifest.version().to_string(),
                    enabled: record.descriptor.enabled,
                    compatible: record.descriptor.compatibility.is_compatible(),
                    extension_points: record
                        .manifest
                        .contributes_v4()
                        .extension_points
                        .iter()
                        .map(|point| point.id.clone())
                        .collect(),
                },
            )
        })
        .collect()
}

struct GraphPackageSnapshot {
    version: String,
    enabled: bool,
    compatible: bool,
    extension_points: Vec<String>,
}

fn dependency_status(
    dependency: &PackageDependencyDescriptor,
    records: &HashMap<String, GraphPackageSnapshot>,
) -> (PackageDependencyStatus, Option<String>) {
    let Some(record) = records.get(&dependency.package_id) else {
        if dependency.optional {
            return (
                PackageDependencyStatus::OptionalMissing,
                Some(format!(
                    "Optional dependency '{}' is not installed.",
                    dependency.package_id
                )),
            );
        }
        return (
            PackageDependencyStatus::Missing,
            Some(format!("Missing dependency '{}'.", dependency.package_id)),
        );
    };
    if !record.enabled {
        return (
            PackageDependencyStatus::Disabled,
            Some(format!(
                "Dependency '{}' is disabled.",
                dependency.package_id
            )),
        );
    }
    if !record.compatible {
        return (
            PackageDependencyStatus::Incompatible,
            Some(format!(
                "Dependency '{}' is incompatible with this host.",
                dependency.package_id
            )),
        );
    }
    if let Some(requirement) = dependency.version.as_deref() {
        match (
            semver::VersionReq::parse(requirement),
            semver::Version::parse(record.version.trim_start_matches('v')),
        ) {
            (Ok(requirement), Ok(version)) if requirement.matches(&version) => {}
            (Ok(_), Ok(_)) => {
                return (
                    PackageDependencyStatus::VersionMismatch,
                    Some(format!(
                        "Dependency '{}' does not satisfy version requirement '{}'.",
                        dependency.package_id, requirement
                    )),
                );
            }
            _ => {
                return (
                    PackageDependencyStatus::VersionMismatch,
                    Some(format!(
                        "Dependency '{}' has invalid version metadata.",
                        dependency.package_id
                    )),
                );
            }
        }
    }
    (PackageDependencyStatus::Satisfied, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4_manifest(raw: serde_json::Value) -> PluginPackageManifest {
        PluginPackageManifest::parse(&raw.to_string()).unwrap()
    }

    fn package_record(raw: serde_json::Value, enabled: bool) -> (String, PackageRecord) {
        let manifest = v4_manifest(raw);
        let id = manifest.id().to_string();
        let descriptor = descriptor_for_manifest(&manifest, format!("/tmp/{id}"), enabled);
        (
            id,
            PackageRecord {
                descriptor,
                manifest,
            },
        )
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
                    },
                    {
                        "id": "settingsPanel",
                        "kind": "config",
                        "entry": "dist/visuals/settings-panel.js",
                        "defaultSize": [1000, 620]
                    }
                ],
                "services": [
                    {
                        "id": "matchStats",
                        "runtime": "node",
                        "methods": ["snapshot"]
                    }
                ],
                "extensionPoints": [
                    {
                        "id": "overlay.visual",
                        "version": "1.0.0",
                        "title": "Overlay Visual",
                        "schema": "schemas/extension-point.json",
                        "service": "matchStats"
                    }
                ],
                "resources": [
                    {
                        "id": "presetData",
                        "paths": ["data/a.json", "data/b.json"],
                        "type": "application/json",
                        "visibility": "public"
                    }
                ],
                "contributions": [
                    {
                        "id": "scoreboardBinding",
                        "target": "com.example.catalog/overlay.visual",
                        "kind": "visual",
                        "title": "Scoreboard Binding",
                        "visual": "scoreboard",
                        "resources": ["presetData"]
                    }
                ],
                "webviews": [
                    {
                        "id": "inspector",
                        "entry": "dist/webviews/inspector.js",
                        "title": "Inspector"
                    }
                ],
                "settings": {
                    "schema": "schemas/plugin-settings.json",
                    "ui": "settingsPanel"
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
        assert_eq!(
            descriptor.contributions.extension_points[0].reference,
            "com.example.catalog/overlay.visual"
        );
        assert_eq!(
            descriptor.contributions.contributions[0].target,
            "com.example.catalog/overlay.visual"
        );
        assert_eq!(
            descriptor.contributions.resources[0].paths,
            vec!["data/a.json", "data/b.json"]
        );
        assert!(descriptor.contributions.resources[0].public);
        assert_eq!(
            descriptor.contributions.webviews[0].entry.as_deref(),
            Some("dist/webviews/inspector.js")
        );
        assert!(descriptor.contributions.pages.is_empty());
        assert!(descriptor.contributions.overlays.is_empty());
        assert_eq!(
            descriptor.settings.as_deref(),
            Some("schemas/plugin-settings.json")
        );
        assert!(descriptor.contributions.views.is_empty());
        let configuration = descriptor.contributions.configuration.as_ref().unwrap();
        assert_eq!(configuration.path, "dist/visuals/settings-panel.js");
        assert_eq!(configuration.visuals[0].name, "settingsPanel");
        assert_eq!(configuration.visuals[0].default_width, 1000.0);
        assert!(descriptor
            .contributes
            .as_ref()
            .and_then(|value| value.get("visuals"))
            .is_some());
    }

    #[test]
    fn graph_diagnostics_accepts_declared_dependency_contribution() {
        let mut records = HashMap::from([
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.provider",
                    "name": "Provider",
                    "version": "1.2.3",
                    "bakingrlApi": "2.1.0",
                    "contributes": {
                        "extensionPoints": [
                            {
                                "id": "overlay.visual",
                                "version": "1.0.0"
                            }
                        ]
                    }
                }),
                true,
            ),
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.consumer",
                    "name": "Consumer",
                    "version": "1.0.0",
                    "bakingrlApi": "2.1.0",
                    "dependencies": [
                        {
                            "packageId": "bakingrl.provider",
                            "version": "^1.2.0"
                        }
                    ],
                    "contributes": {
                        "contributions": [
                            {
                                "id": "scoreboardBinding",
                                "target": "bakingrl.provider/overlay.visual",
                                "kind": "visual"
                            }
                        ]
                    }
                }),
                true,
            ),
        ]);

        apply_graph_diagnostics(&mut records);

        let consumer = records.get("bakingrl.consumer").unwrap();
        assert!(consumer.descriptor.enabled);
        assert_eq!(consumer.descriptor.error, None);
        assert_eq!(
            consumer.descriptor.dependencies[0].status,
            PackageDependencyStatus::Satisfied
        );
    }

    #[test]
    fn graph_diagnostics_rejects_required_dependency_version_mismatch() {
        let mut records = HashMap::from([
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.provider",
                    "name": "Provider",
                    "version": "1.2.3",
                    "bakingrlApi": "2.1.0",
                    "contributes": {}
                }),
                true,
            ),
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.consumer",
                    "name": "Consumer",
                    "version": "1.0.0",
                    "bakingrlApi": "2.1.0",
                    "dependencies": [
                        {
                            "packageId": "bakingrl.provider",
                            "version": "^2.0.0"
                        }
                    ]
                }),
                true,
            ),
        ]);

        apply_graph_diagnostics(&mut records);

        let consumer = records.get("bakingrl.consumer").unwrap();
        assert!(!consumer.descriptor.enabled);
        assert_eq!(
            consumer.descriptor.dependencies[0].status,
            PackageDependencyStatus::VersionMismatch
        );
        assert!(consumer
            .descriptor
            .error
            .as_ref()
            .is_some_and(|error| error.contains("does not satisfy version requirement")));
    }

    #[test]
    fn graph_diagnostics_rejects_contribution_without_declared_dependency() {
        let mut records = HashMap::from([
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.provider",
                    "name": "Provider",
                    "version": "1.0.0",
                    "bakingrlApi": "2.1.0",
                    "contributes": {
                        "extensionPoints": [
                            {
                                "id": "overlay.visual"
                            }
                        ]
                    }
                }),
                true,
            ),
            package_record(
                serde_json::json!({
                    "schemaVersion": "bakingrl.plugin/4",
                    "id": "bakingrl.consumer",
                    "name": "Consumer",
                    "version": "1.0.0",
                    "bakingrlApi": "2.1.0",
                    "contributes": {
                        "contributions": [
                            {
                                "id": "scoreboardBinding",
                                "target": "bakingrl.provider/overlay.visual"
                            }
                        ]
                    }
                }),
                true,
            ),
        ]);

        apply_graph_diagnostics(&mut records);

        let consumer = records.get("bakingrl.consumer").unwrap();
        assert!(!consumer.descriptor.enabled);
        assert!(consumer
            .descriptor
            .error
            .as_ref()
            .is_some_and(|error| error.contains("does not declare a dependency")));
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
