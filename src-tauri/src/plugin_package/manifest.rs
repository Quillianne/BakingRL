use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PLUGIN_SCHEMA_V4: &str = "bakingrl.plugin/4";
pub const HOST_RUNTIME_API_VERSION: &str = "2.4.0";
pub const MIN_SUPPORTED_RUNTIME_API_VERSION: &str = "2.3.0";

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

        let mut v4: PluginPackageManifestV4 = serde_json::from_value(value)
            .map_err(|error| format!("plugin manifest {PLUGIN_SCHEMA_V4} is invalid: {error}"))?;

        v4.normalize()?;
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

    pub fn settings_ui_webview(&self) -> Option<&str> {
        self.v4
            .contributes
            .settings
            .as_ref()
            .and_then(|settings| settings.ui.as_deref())
    }

    pub fn runtime_v4(&self) -> Option<&PluginRuntimeV4> {
        self.v4.runtime.as_ref()
    }

    pub fn dependencies_v4(&self) -> &[PluginDependencyV4] {
        &self.v4.dependencies
    }

    pub fn contributes_v4(&self) -> &PluginContributesV4 {
        &self.v4.contributes
    }

    pub fn permissions_v4(&self) -> Option<&PluginPermissionsV4> {
        self.v4.permissions.as_ref()
    }

    pub fn presentation_v4(&self) -> Option<&PluginPresentationV4> {
        self.v4.presentation.as_ref()
    }

    pub fn contributes_value(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.v4.contributes).ok()
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
    pub dependencies: Vec<PluginDependencyV4>,
    #[serde(default)]
    pub runtime: Option<PluginRuntimeV4>,
    #[serde(default)]
    pub permissions: Option<PluginPermissionsV4>,
    #[serde(default)]
    pub presentation: Option<PluginPresentationV4>,
    #[serde(default)]
    pub contributes: PluginContributesV4,
}

fn reject_legacy_manifest_fields(
    value: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    const REJECTED: [&str; 7] = [
        "schema",
        "compatibility",
        "capabilities",
        "kind",
        "activation",
        "settings",
        "externalSurfaces",
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
        for field in ["pages", "views", "overlays", "configuration", "visuals"] {
            if object.contains_key(field) {
                return Err(format!(
                    "host-owned contributes.{field} is not supported in {PLUGIN_SCHEMA_V4}"
                ));
            }
        }
    }

    Ok(())
}

impl PluginPackageManifestV4 {
    fn normalize(&mut self) -> Result<(), String> {
        if let Some(permissions) = &mut self.permissions {
            permissions.normalize()?;
        }
        Ok(())
    }

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
        validate_duplicate_package_ids(
            "dependencies",
            self.dependencies
                .iter()
                .map(|dependency| dependency.package_id.as_str()),
        )?;
        for dependency in &self.dependencies {
            dependency.validate(&self.id)?;
        }
        validate_package_id(&self.id)?;
        validate_non_empty("name", &self.name)?;
        validate_non_empty("version", &self.version)?;
        validate_semver("bakingrlApi", &self.bakingrl_api)?;
        if let Some(permissions) = &self.permissions {
            permissions.validate()?;
        }
        self.contributes.validate()?;
        if let Some(presentation) = &self.presentation {
            let runtime_api = parse_runtime_api_version(&self.bakingrl_api)
                .expect("bakingrlApi was validated as an exact semver");
            if runtime_api < (2, 4, 0) {
                return Err("presentation requires bakingrlApi 2.4.0 or newer".to_string());
            }
            presentation.validate(&self.contributes)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginPresentationV4 {
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub primary_action: Option<PluginPrimaryActionV4>,
}

impl PluginPresentationV4 {
    fn validate(&self, contributes: &PluginContributesV4) -> Result<(), String> {
        let mut categories = std::collections::HashSet::new();
        for category in &self.categories {
            validate_export_name("presentation.categories", category)?;
            if !categories.insert(category) {
                return Err(format!(
                    "presentation.categories contains duplicate category '{category}'"
                ));
            }
        }
        if let Some(primary_action) = &self.primary_action {
            primary_action.validate(contributes)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginPrimaryActionV4 {
    pub kind: String,
    pub target: Option<String>,
}

impl PluginPrimaryActionV4 {
    fn validate(&self, contributes: &PluginContributesV4) -> Result<(), String> {
        match self.kind.as_str() {
            "webview" => {
                let target = self.target.as_deref().ok_or_else(|| {
                    "presentation.primaryAction.target is required for kind 'webview'".to_string()
                })?;
                validate_export_name("presentation.primaryAction.target", target)?;
                if !contributes
                    .webviews
                    .iter()
                    .any(|webview| webview.id == target)
                {
                    return Err(format!(
                        "presentation.primaryAction.target references unknown contributes.webviews id '{target}'"
                    ));
                }
            }
            "settings" => {
                if self.target.is_some() {
                    return Err(
                        "presentation.primaryAction.target is not allowed for kind 'settings'"
                            .to_string(),
                    );
                }
                let Some(settings) = &contributes.settings else {
                    return Err(
                        "presentation.primaryAction kind 'settings' requires contributes.settings.schema or contributes.settings.ui"
                            .to_string(),
                    );
                };
                if settings.schema.is_none() && settings.ui.is_none() {
                    return Err(
                        "presentation.primaryAction kind 'settings' requires contributes.settings.schema or contributes.settings.ui"
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err(
                    "presentation.primaryAction.kind must be webview or settings".to_string(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginPermissionsV4 {
    #[serde(default)]
    pub bus: PluginBusPermissionsV4,
    #[serde(default)]
    pub registry: PluginRegistryPermissionsV4,
    #[serde(default)]
    pub network: PluginNetworkPermissionsV4,
    #[serde(default)]
    pub storage: PluginStoragePermissionsV4,
}

impl PluginPermissionsV4 {
    fn normalize(&mut self) -> Result<(), String> {
        for pattern in self
            .storage
            .read
            .iter_mut()
            .chain(self.storage.write.iter_mut())
        {
            *pattern = normalize_storage_permission_pattern(pattern)?;
        }
        self.network.normalize()
    }

    fn validate(&self) -> Result<(), String> {
        validate_permission_patterns("permissions.bus.read", &self.bus.read)?;
        validate_permission_patterns("permissions.bus.publish", &self.bus.publish)?;
        validate_permission_patterns("permissions.registry.read", &self.registry.read)?;
        validate_permission_patterns("permissions.registry.write", &self.registry.write)?;
        validate_permission_patterns("permissions.storage.read", &self.storage.read)?;
        validate_permission_patterns("permissions.storage.write", &self.storage.write)?;
        self.network.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginBusPermissionsV4 {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginRegistryPermissionsV4 {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginStoragePermissionsV4 {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginNetworkPermissionsV4 {
    #[serde(default)]
    pub http: Vec<PluginNetworkEndpointV4>,
    #[serde(default)]
    pub websocket: Vec<PluginNetworkEndpointV4>,
    #[serde(default)]
    pub listen: Vec<PluginListenEndpointV4>,
}

impl PluginNetworkPermissionsV4 {
    fn normalize(&mut self) -> Result<(), String> {
        for endpoint in self.http.iter_mut().chain(self.websocket.iter_mut()) {
            endpoint.normalize()?;
        }
        for endpoint in &mut self.listen {
            endpoint.normalize()?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        for endpoint in &self.http {
            endpoint.validate("permissions.network.http", &["http", "https"])?;
        }
        for endpoint in &self.websocket {
            endpoint.validate("permissions.network.websocket", &["ws", "wss"])?;
        }
        for endpoint in &self.listen {
            endpoint.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginNetworkEndpointV4 {
    pub scheme: String,
    pub host: String,
    pub ports: PluginNetworkPortsV4,
    #[serde(default, rename = "pathPrefixes")]
    pub path_prefixes: Vec<String>,
}

impl PluginNetworkEndpointV4 {
    fn normalize(&mut self) -> Result<(), String> {
        self.scheme = self.scheme.to_ascii_lowercase();
        self.host = normalize_network_host(&self.host)?;
        self.ports.normalize();
        for prefix in &mut self.path_prefixes {
            *prefix = normalize_network_path_prefix(prefix)?;
        }
        Ok(())
    }

    fn validate(&self, field: &str, schemes: &[&str]) -> Result<(), String> {
        if !schemes.contains(&self.scheme.as_str()) {
            return Err(format!(
                "{field}.scheme must be one of {}",
                schemes.join(", ")
            ));
        }
        validate_non_empty(&format!("{field}.host"), &self.host)?;
        self.ports.validate(&format!("{field}.ports"))?;
        for prefix in &self.path_prefixes {
            if !prefix.starts_with('/') {
                return Err(format!("{field}.pathPrefixes must start with '/'"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginListenEndpointV4 {
    pub transport: String,
    pub host: String,
    pub ports: PluginNetworkPortsV4,
}

impl PluginListenEndpointV4 {
    fn normalize(&mut self) -> Result<(), String> {
        self.transport = self.transport.to_ascii_lowercase();
        self.host = normalize_network_host(&self.host)?;
        self.ports.normalize();
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.transport.as_str(),
            "http" | "https" | "ws" | "wss" | "tcp"
        ) {
            return Err(
                "permissions.network.listen.transport must be http, https, ws, wss, or tcp"
                    .to_string(),
            );
        }
        validate_non_empty("permissions.network.listen.host", &self.host)?;
        self.ports.validate("permissions.network.listen.ports")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PluginNetworkPortsV4 {
    Any(String),
    Ports(Vec<u16>),
}

impl PluginNetworkPortsV4 {
    fn normalize(&mut self) {
        if let Self::Ports(ports) = self {
            ports.sort_unstable();
            ports.dedup();
        }
    }

    fn validate(&self, field: &str) -> Result<(), String> {
        match self {
            Self::Any(value) if value == "*" => Ok(()),
            Self::Any(_) => Err(format!("{field} must be '*' or an array of ports")),
            Self::Ports(ports) if ports.is_empty() => {
                Err(format!("{field} must contain at least one port"))
            }
            Self::Ports(ports) if ports.contains(&0) => {
                Err(format!("{field} cannot contain port 0"))
            }
            Self::Ports(_) => Ok(()),
        }
    }
}

fn normalize_network_host(host: &str) -> Result<String, String> {
    let host = host.trim().to_ascii_lowercase();
    validate_non_empty("permissions.network host", &host)?;
    url::Host::parse(&host)
        .map(|host| host.to_string())
        .map_err(|error| format!("permissions.network host '{host}' is invalid: {error}"))
}

fn normalize_network_path_prefix(prefix: &str) -> Result<String, String> {
    if !prefix.starts_with('/') || prefix.contains('?') || prefix.contains('#') {
        return Err(format!(
            "permissions.network path prefix '{prefix}' must be an absolute URL path without query or fragment"
        ));
    }
    let url = url::Url::parse(&format!("https://permission.invalid{prefix}")).map_err(|error| {
        format!("permissions.network path prefix '{prefix}' is invalid: {error}")
    })?;
    Ok(url.path().to_string())
}

fn validate_permission_patterns(field: &str, patterns: &[String]) -> Result<(), String> {
    for pattern in patterns {
        validate_permission_pattern(field, pattern)?;
    }
    Ok(())
}

fn validate_permission_pattern(field: &str, pattern: &str) -> Result<(), String> {
    validate_non_empty(field, pattern)?;
    let wildcard_count = pattern.bytes().filter(|byte| *byte == b'*').count();
    if wildcard_count > 1 || (wildcard_count == 1 && !pattern.ends_with('*')) {
        return Err(format!(
            "{field} pattern '{pattern}' may contain only one terminal '*'"
        ));
    }
    Ok(())
}

fn normalize_storage_permission_pattern(pattern: &str) -> Result<String, String> {
    validate_permission_pattern("permissions.storage", pattern)?;
    if pattern == "*" {
        return Ok(pattern.to_string());
    }
    let has_wildcard = pattern.ends_with('*');
    let path = pattern.strip_suffix('*').unwrap_or(pattern);
    if path.starts_with('/') || path.contains('\\') {
        return Err(format!(
            "permissions.storage pattern '{pattern}' must be a relative '/' path"
        ));
    }
    let mut normalized = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "permissions.storage pattern '{pattern}' cannot contain '..'"
                ))
            }
            component => normalized.push(component),
        }
    }
    let normalized = normalized.join("/");
    if normalized.is_empty() {
        return Err(format!(
            "permissions.storage pattern '{pattern}' must name a path"
        ));
    }
    Ok(if has_wildcard {
        format!("{normalized}*")
    } else {
        normalized
    })
}

pub fn permission_pattern_matches(pattern: &str, value: &str) -> bool {
    pattern == "*"
        || pattern == value
        || pattern
            .strip_suffix('*')
            .is_some_and(|prefix| value.starts_with(prefix))
}

pub fn permission_pattern_covers(pattern: &str, requested: &str) -> bool {
    if requested.ends_with('*') {
        pattern == "*"
            || pattern
                .strip_suffix('*')
                .is_some_and(|prefix| requested.starts_with(prefix))
    } else {
        permission_pattern_matches(pattern, requested)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginDependencyV4 {
    #[serde(rename = "packageId")]
    pub package_id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

impl PluginDependencyV4 {
    fn validate(&self, package_id: &str) -> Result<(), String> {
        validate_package_id(&self.package_id)?;
        if self.package_id == package_id {
            return Err("dependencies must not reference the package itself".to_string());
        }
        if let Some(version) = &self.version {
            validate_semver_req("dependencies.version", version)?;
        }
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
    #[serde(rename = "healthCheck")]
    pub health_check: Option<PluginRuntimeSidecarHealthCheckV4>,
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
        if let Some(health_check) = &self.health_check {
            health_check.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeSidecarHealthCheckV4 {
    pub method: String,
    #[serde(rename = "intervalMs")]
    pub interval_ms: Option<u64>,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: Option<u64>,
}

impl PluginRuntimeSidecarHealthCheckV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("runtime.sidecars.healthCheck.method", &self.method)?;
        if self.interval_ms.is_some_and(|value| value < 500) {
            return Err("runtime.sidecars.healthCheck.intervalMs must be at least 500".to_string());
        }
        if self.timeout_ms.is_some_and(|value| value < 100) {
            return Err("runtime.sidecars.healthCheck.timeoutMs must be at least 100".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginContributesV4 {
    #[serde(default)]
    pub settings: Option<PluginSettingsContributionV4>,
    #[serde(default)]
    pub services: Vec<PluginServiceContributionV4>,
    #[serde(default)]
    pub commands: Vec<PluginCommandContributionV4>,
    #[serde(default, rename = "extensionPoints")]
    pub extension_points: Vec<PluginExtensionPointContributionV4>,
    #[serde(default)]
    pub contributions: Vec<PluginContributionBindingV4>,
    #[serde(default)]
    pub resources: Vec<PluginResourceContributionV4>,
    #[serde(default)]
    pub webviews: Vec<PluginWebviewContributionV4>,
}

impl PluginContributesV4 {
    fn validate(&self) -> Result<(), String> {
        validate_duplicate_ids(
            "contributes.services",
            self.services.iter().map(|service| service.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.commands",
            self.commands.iter().map(|command| command.id.as_str()),
        )?;
        validate_duplicate_extension_point_ids(
            "contributes.extensionPoints",
            self.extension_points.iter().map(|point| point.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.contributions",
            self.contributions
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.resources",
            self.resources.iter().map(|resource| resource.id.as_str()),
        )?;
        validate_duplicate_ids(
            "contributes.webviews",
            self.webviews.iter().map(|webview| webview.id.as_str()),
        )?;

        for service in &self.services {
            service.validate()?;
        }
        for command in &self.commands {
            command.validate()?;
        }
        for extension_point in &self.extension_points {
            extension_point.validate(&self.services)?;
        }
        for contribution in &self.contributions {
            contribution.validate(&self.services, &self.resources)?;
        }
        for resource in &self.resources {
            resource.validate()?;
        }
        for webview in &self.webviews {
            webview.validate()?;
        }
        if let Some(settings) = &self.settings {
            settings.validate()?;
            if let Some(ui) = &settings.ui {
                let references_settings_webview = self.webviews.iter().any(|webview| {
                    webview.id == *ui && webview.kind.as_deref() == Some("settings")
                });
                if !references_settings_webview {
                    return Err(format!(
                        "contributes.settings.ui must reference an existing contributes.webviews id with kind 'settings' (missing '{ui}')"
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
pub struct PluginExtensionPointContributionV4 {
    pub id: String,
    pub version: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub schema: Option<String>,
    pub service: Option<String>,
}

impl PluginExtensionPointContributionV4 {
    fn validate(&self, services: &[PluginServiceContributionV4]) -> Result<(), String> {
        validate_extension_point_id("contributes.extensionPoints.id", &self.id)?;
        if let Some(version) = &self.version {
            validate_semver("contributes.extensionPoints.version", version)?;
        }
        if let Some(title) = &self.title {
            validate_non_empty("contributes.extensionPoints.title", title)?;
        }
        if let Some(description) = &self.description {
            validate_non_empty("contributes.extensionPoints.description", description)?;
        }
        if let Some(schema) = &self.schema {
            validate_relative_plugin_path("contributes.extensionPoints.schema", schema)?;
        }
        if let Some(service) = &self.service {
            validate_export_name("contributes.extensionPoints.service", service)?;
            if !services.iter().any(|candidate| candidate.id == *service) {
                return Err(format!(
                    "contributes.extensionPoints.service references unknown contributes.services id '{service}'"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginContributionBindingV4 {
    pub id: String,
    pub target: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "dataSchema")]
    pub data_schema: Option<String>,
    pub service: Option<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

impl PluginContributionBindingV4 {
    fn validate(
        &self,
        services: &[PluginServiceContributionV4],
        resources: &[PluginResourceContributionV4],
    ) -> Result<(), String> {
        validate_export_name("contributes.contributions.id", &self.id)?;
        validate_extension_ref("contributes.contributions.target", &self.target)?;
        if let Some(kind) = &self.kind {
            validate_non_empty("contributes.contributions.kind", kind)?;
        }
        if let Some(title) = &self.title {
            validate_non_empty("contributes.contributions.title", title)?;
        }
        if let Some(description) = &self.description {
            validate_non_empty("contributes.contributions.description", description)?;
        }
        if let Some(data_schema) = &self.data_schema {
            validate_relative_plugin_path("contributes.contributions.dataSchema", data_schema)?;
        }
        if let Some(service) = &self.service {
            validate_export_name("contributes.contributions.service", service)?;
            if !services.iter().any(|candidate| candidate.id == *service) {
                return Err(format!(
                    "contributes.contributions.service references unknown contributes.services id '{service}'"
                ));
            }
        }
        for resource in &self.resources {
            validate_export_name("contributes.contributions.resources", resource)?;
            if !resources.iter().any(|candidate| candidate.id == *resource) {
                return Err(format!(
                    "contributes.contributions.resources references unknown contributes.resources id '{resource}'"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginResourceVisibilityV4 {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "private")]
    Private,
}

impl Default for PluginResourceVisibilityV4 {
    fn default() -> Self {
        Self::Private
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginResourceContributionV4 {
    pub id: String,
    pub path: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub visibility: PluginResourceVisibilityV4,
    pub metadata: Option<serde_json::Value>,
}

impl PluginResourceContributionV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("contributes.resources.id", &self.id)?;
        let has_path = self
            .path
            .as_ref()
            .is_some_and(|path| !path.trim().is_empty());
        let has_paths = !self.paths.is_empty();
        if has_path == has_paths {
            return Err(
                "contributes.resources must declare exactly one of path or paths".to_string(),
            );
        }
        if let Some(path) = &self.path {
            validate_relative_plugin_path("contributes.resources.path", path)?;
        }
        for path in &self.paths {
            validate_relative_plugin_path("contributes.resources.paths", path)?;
        }
        if let Some(resource_type) = &self.resource_type {
            validate_non_empty("contributes.resources.type", resource_type)?;
        } else if self.visibility == PluginResourceVisibilityV4::Public {
            return Err("contributes.resources.type is required for public resources".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginWebviewContributionV4 {
    pub id: String,
    pub entry: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    #[serde(rename = "defaultSize")]
    pub default_size: Option<[f64; 2]>,
    pub surface: Option<PluginSurfaceOptionsV4>,
}

impl PluginWebviewContributionV4 {
    fn validate(&self) -> Result<(), String> {
        validate_export_name("contributes.webviews.id", &self.id)?;
        validate_js_entry("contributes.webviews.entry", &self.entry)?;
        if let Some(title) = &self.title {
            validate_non_empty("contributes.webviews.title", title)?;
        }
        if let Some(kind) = &self.kind {
            if !matches!(kind.as_str(), "tool" | "settings" | "panel" | "surface") {
                return Err(
                    "contributes.webviews.kind must be tool, settings, panel, or surface"
                        .to_string(),
                );
            }
        }
        if let Some(size) = self.default_size {
            if size[0] <= 0.0 || size[1] <= 0.0 {
                return Err(
                    "contributes.webviews.defaultSize must contain positive dimensions".to_string(),
                );
            }
        }
        let is_surface = self.kind.as_deref() == Some("surface");
        if is_surface {
            if self.default_size.is_none() {
                return Err(
                    "contributes.webviews.defaultSize is required for kind 'surface'".to_string(),
                );
            }
            self.surface.as_ref().ok_or_else(|| {
                "contributes.webviews.surface is required for kind 'surface'".to_string()
            })?;
        } else if self.surface.is_some() {
            return Err(
                "contributes.webviews.surface is only valid for kind 'surface'".to_string(),
            );
        }
        if let Some(surface) = &self.surface {
            surface.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct PluginSurfaceOptionsV4 {
    pub default_position: Option<[f64; 2]>,
    pub default_screen: Option<String>,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default)]
    pub click_through: bool,
    #[serde(default = "default_surface_resizable")]
    pub resizable: bool,
}

impl PluginSurfaceOptionsV4 {
    fn validate(&self) -> Result<(), String> {
        if let Some(position) = self.default_position {
            if !position.iter().all(|value| value.is_finite()) {
                return Err(
                    "contributes.webviews.surface.defaultPosition must contain finite coordinates"
                        .to_string(),
                );
            }
        }
        if let Some(screen) = &self.default_screen {
            validate_non_empty("contributes.webviews.surface.defaultScreen", screen)?;
        }
        Ok(())
    }
}

fn default_surface_resizable() -> bool {
    true
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

fn validate_duplicate_package_ids<'a>(
    field: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(format!("{field} cannot contain duplicate packageId '{id}'"));
        }
        validate_package_id(id)?;
    }
    Ok(())
}

fn validate_duplicate_extension_point_ids<'a>(
    field: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(format!("{field} cannot contain duplicate id '{id}'"));
        }
        validate_extension_point_id(field, id)?;
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

fn validate_semver_req(field: &str, value: &str) -> Result<(), String> {
    semver::VersionReq::parse(value)
        .map(|_| ())
        .map_err(|err| format!("{field} must be a valid semver requirement: {err}"))
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

fn validate_extension_point_id(kind: &str, value: &str) -> Result<(), String> {
    validate_non_empty(kind, value)?;
    if value == "." || value == ".." || value.starts_with('.') || value.ends_with('.') {
        return Err(format!(
            "{kind} must not contain empty or dot-only path segments"
        ));
    }
    if value.split('.').any(|segment| segment.is_empty()) {
        return Err(format!(
            "{kind} must not contain empty dot-separated segments"
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_')
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

fn validate_extension_ref(field: &str, value: &str) -> Result<(), String> {
    let Some((package_id, extension_point_id)) = value.split_once('/') else {
        return Err(format!(
            "{field} must use '<package-id>/<extension-point-id>'"
        ));
    };
    validate_package_id(package_id)?;
    validate_extension_point_id(field, extension_point_id)
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
            "bakingrlApi": "2.3.0",
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
            Some("2.3.0")
        );
        assert_eq!(
            manifest
                .runtime_v4()
                .and_then(|runtime| runtime.node.as_ref())
                .map(|node| node.entry.as_str()),
            Some("dist/extension-host.js")
        );
        assert_eq!(manifest.runtime_v4().unwrap().sidecars.len(), 1);
        assert_eq!(manifest.contributes_v4().services.len(), 1);
        assert_eq!(manifest.contributes_v4().commands.len(), 1);
    }

    #[test]
    fn accepts_v4_package_presentation_with_primary_webview() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.studio",
            "name": "Example Studio",
            "version": "1.0.0",
            "bakingrlApi": "2.4.0",
            "presentation": {
                "categories": ["layouts", "productivity"],
                "primaryAction": {
                    "kind": "webview",
                    "target": "studio"
                }
            },
            "contributes": {
                "webviews": [
                    {
                        "id": "studio",
                        "entry": "dist/studio.js",
                        "kind": "tool"
                    }
                ]
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();
        let presentation = manifest.presentation_v4().unwrap();
        assert_eq!(presentation.categories, ["layouts", "productivity"]);
        assert_eq!(
            presentation
                .primary_action
                .as_ref()
                .and_then(|action| action.target.as_deref()),
            Some("studio")
        );
    }

    #[test]
    fn rejects_v4_package_presentation_with_unknown_primary_webview() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.studio",
            "name": "Example Studio",
            "version": "1.0.0",
            "bakingrlApi": "2.4.0",
            "presentation": {
                "categories": ["layouts"],
                "primaryAction": {
                    "kind": "webview",
                    "target": "missing"
                }
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("references unknown contributes.webviews id 'missing'"));
    }

    #[test]
    fn rejects_v4_package_presentation_before_runtime_api_2_4() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.legacy-presentation",
            "name": "Legacy Presentation",
            "version": "1.0.0",
            "bakingrlApi": "2.3.99",
            "presentation": {
                "categories": ["layouts"]
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("presentation requires bakingrlApi 2.4.0 or newer"));
    }

    #[test]
    fn accepts_v4_package_presentation_with_primary_settings_schema() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings-presentation",
            "name": "Settings Presentation",
            "version": "1.0.0",
            "bakingrlApi": "2.4.0",
            "presentation": {
                "primaryAction": {
                    "kind": "settings"
                }
            },
            "contributes": {
                "settings": {
                    "schema": "schemas/settings.json"
                }
            }
        })
        .to_string();

        PluginPackageManifest::parse(&raw).unwrap();
    }

    #[test]
    fn rejects_v4_package_presentation_with_empty_primary_settings() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.empty-settings-presentation",
            "name": "Empty Settings Presentation",
            "version": "1.0.0",
            "bakingrlApi": "2.4.0",
            "presentation": {
                "primaryAction": {
                    "kind": "settings"
                }
            },
            "contributes": {
                "settings": {}
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains(
            "primaryAction kind 'settings' requires contributes.settings.schema or contributes.settings.ui"
        ));
    }

    #[test]
    fn accepts_v4_dependencies_extensions_resources_webviews_and_sidecar_health() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.platform-extension",
            "name": "Platform Extension",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "bakingrl.overlay-studio",
                    "version": "^1.2.0"
                },
                {
                    "packageId": "bakingrl.optional-provider",
                    "optional": true
                }
            ],
            "runtime": {
                "sidecars": [
                    {
                        "id": "worker",
                        "bin": "bin/worker",
                        "protocol": "jsonrpc-stdio",
                        "healthCheck": {
                            "method": "ping",
                            "intervalMs": 1000,
                            "timeoutMs": 250
                        }
                    }
                ]
            },
            "contributes": {
                "services": [
                    {
                        "id": "catalog",
                        "runtime": "sidecar:worker",
                        "methods": ["list"]
                    }
                ],
                "extensionPoints": [
                    {
                        "id": "overlay-studio.visual",
                        "version": "1.0.0",
                        "title": "Overlay Visual",
                        "schema": "schemas/extension-point.json",
                        "service": "catalog"
                    }
                ],
                "resources": [
                    {
                        "id": "sampleData",
                        "path": "data/sample.json",
                        "type": "application/json",
                        "visibility": "public"
                    }
                ],
                "contributions": [
                    {
                        "id": "scoreboardBinding",
                        "target": "bakingrl.overlay-studio/overlay-studio.visual",
                        "kind": "widget",
                        "title": "Scoreboard Binding",
                        "dataSchema": "schemas/binding.json",
                        "service": "catalog",
                        "resources": ["sampleData"],
                        "metadata": {
                            "category": "match",
                            "renderer": "inspector"
                        }
                    }
                ],
                "webviews": [
                    {
                        "id": "inspector",
                        "entry": "dist/webviews/inspector.js",
                        "title": "Inspector",
                        "kind": "panel",
                        "defaultSize": [640, 480]
                    }
                ]
            }
        })
        .to_string();

        let manifest = PluginPackageManifest::parse(&raw).unwrap();
        assert_eq!(manifest.dependencies_v4().len(), 2);
        assert_eq!(
            manifest.dependencies_v4()[0].package_id,
            "bakingrl.overlay-studio"
        );
        assert_eq!(
            manifest.dependencies_v4()[0].version.as_deref(),
            Some("^1.2.0")
        );
        assert!(manifest.dependencies_v4()[1].optional);

        let sidecar = &manifest.runtime_v4().unwrap().sidecars[0];
        assert_eq!(
            sidecar
                .health_check
                .as_ref()
                .map(|health| health.method.as_str()),
            Some("ping")
        );
        assert_eq!(
            manifest.contributes_v4().extension_points[0].id,
            "overlay-studio.visual"
        );
        assert_eq!(
            manifest.contributes_v4().contributions[0].target,
            "bakingrl.overlay-studio/overlay-studio.visual"
        );
        assert_eq!(manifest.contributes_v4().resources[0].id, "sampleData");
        assert_eq!(manifest.contributes_v4().webviews[0].id, "inspector");
    }

    #[test]
    fn rejects_v4_self_dependency() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.self",
            "name": "Self Dependency",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "dependencies": [
                {
                    "packageId": "com.example.self"
                }
            ]
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("dependencies must not reference the package itself"));
    }

    #[test]
    fn accepts_v4_settings_ui_referencing_settings_webview() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings-ui",
            "name": "Settings UI",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settingsPanel",
                        "kind": "settings",
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
        assert_eq!(manifest.settings_ui_webview(), Some("settingsPanel"));
    }

    #[test]
    fn rejects_v4_settings_ui_without_settings_webview() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings-ui",
            "name": "Settings UI",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "contributes": {
                "webviews": [
                    {
                        "id": "settingsPanel",
                        "kind": "panel",
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
            "bakingrlApi": "2.3.0",
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
    fn rejects_v4_external_surfaces_field() {
        let raw = serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.external-missing-runtime",
            "name": "External Surface Missing Runtime",
            "version": "1.0.0",
            "bakingrlApi": "2.3.0",
            "externalSurfaces": {
                "broadcast": {}
            }
        })
        .to_string();

        let error = PluginPackageManifest::parse(&raw).unwrap_err();
        assert!(error.contains("manifest field 'externalSurfaces'"));
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
                "bakingrlApi": "2.3.0",
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
        for field in ["pages", "views", "overlays", "configuration", "visuals"] {
            let mut contributes = serde_json::Map::new();
            contributes.insert(field.to_string(), serde_json::json!([]));
            let raw = serde_json::json!({
                "schemaVersion": "bakingrl.plugin/4",
                "id": "com.example.legacy",
                "name": "Legacy",
                "version": "1.0.0",
                "bakingrlApi": "2.3.0",
                "contributes": contributes
            })
            .to_string();

            let error = PluginPackageManifest::parse(&raw).unwrap_err();
            assert!(
                error.contains(&format!("host-owned contributes.{field}")),
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
            "bakingrlApi": "2.3.0",
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
            "bakingrlApi": "2.3.0",
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
            "bakingrlApi": "2.3.0",
            "runtime": {
                "node": {
                    "entry": "dist/extension-host.ts"
                }
            },
            "contributes": {}
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
                "bakingrlApi": "2.3.0",
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
