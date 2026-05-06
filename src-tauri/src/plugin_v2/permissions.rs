use serde::{Deserialize, Serialize};

use super::manifest::{NetworkPermissionsV2, PluginPackageManifestV2};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectivePackagePermissionsV2 {
    pub bus: EffectiveBusPermissionsV2,
    pub registry: EffectiveRegistryPermissionsV2,
    pub network: NetworkPermissionsV2,
    pub storage: EffectiveStoragePermissionsV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveBusPermissionsV2 {
    pub read: Vec<String>,
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveRegistryPermissionsV2 {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveStoragePermissionsV2 {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

impl EffectivePackagePermissionsV2 {
    pub fn for_manifest(manifest: &PluginPackageManifestV2) -> Result<Self, String> {
        manifest.validate()?;
        let plugin_scope = format!("plugin.{}.*", manifest.id);
        let storage_scope = "plugin://self/*".to_string();

        let requested_bus = manifest.permissions.bus.clone().unwrap_or_default();
        let requested_registry = manifest.permissions.registry.clone().unwrap_or_default();
        let network = manifest.permissions.network.clone().unwrap_or_default();
        let storage_requested = manifest
            .permissions
            .storage
            .iter()
            .any(|value| value == &storage_scope);

        Ok(Self {
            bus: EffectiveBusPermissionsV2 {
                read: requested_bus.read,
                publish: requested_bus
                    .publish
                    .into_iter()
                    .filter(|pattern| pattern == &plugin_scope)
                    .collect(),
            },
            registry: EffectiveRegistryPermissionsV2 {
                read: requested_registry.read,
                write: requested_registry
                    .write
                    .into_iter()
                    .filter(|pattern| pattern == &plugin_scope)
                    .collect(),
            },
            network,
            storage: EffectiveStoragePermissionsV2 {
                read: if storage_requested {
                    vec![storage_scope.clone()]
                } else {
                    Vec::new()
                },
                write: if storage_requested {
                    vec![storage_scope]
                } else {
                    Vec::new()
                },
            },
        })
    }

    pub fn can_read_bus(&self, event_name: &str) -> bool {
        matches_pattern(&self.bus.read, event_name)
    }

    pub fn can_publish_bus(&self, event_name: &str) -> bool {
        matches_pattern(&self.bus.publish, event_name)
    }

    pub fn can_read_registry(&self, key: &str) -> bool {
        matches_pattern(&self.registry.read, key)
    }

    pub fn can_write_registry(&self, key: &str) -> bool {
        matches_pattern(&self.registry.write, key)
    }

    pub fn can_use_http_host(&self, host: &str) -> bool {
        self.network.http.iter().any(|allowed| allowed == host)
    }

    pub fn can_use_websocket_host(&self, host: &str) -> bool {
        self.network.websocket.iter().any(|allowed| allowed == host)
    }
}

pub fn matches_pattern(patterns: &[String], value: &str) -> bool {
    patterns.iter().any(|pattern| {
        pattern == "*"
            || pattern == value
            || pattern
                .strip_suffix(".*")
                .is_some_and(|prefix| value.starts_with(prefix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_effective_permissions() {
        let manifest: PluginPackageManifestV2 = serde_json::from_value(serde_json::json!({
            "schema": "bakingrl.plugin/2",
            "id": "com.example.stats",
            "name": "Stats",
            "version": "1.0.0",
            "exports": {
                "services": {
                    "stats": { "entry": "dist/services/stats.js" }
                }
            },
            "permissions": {
                "bus": {
                    "read": ["UpdateState"],
                    "publish": ["plugin.com.example.stats.*"]
                },
                "registry": {
                    "write": ["plugin.com.example.stats.*"]
                },
                "storage": ["plugin://self/*"]
            }
        }))
        .unwrap();

        let permissions = EffectivePackagePermissionsV2::for_manifest(&manifest).unwrap();
        assert_eq!(permissions.bus.read, vec!["UpdateState"]);
        assert_eq!(permissions.bus.publish, vec!["plugin.com.example.stats.*"]);
        assert_eq!(
            permissions.storage.write,
            vec!["plugin://self/*".to_string()]
        );
    }
}
