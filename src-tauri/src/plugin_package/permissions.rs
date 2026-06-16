use serde::{Deserialize, Serialize};

use super::manifest::{NetworkPermissions, PluginPackageManifest, PluginPermissions};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectivePackagePermissions {
    pub bus: EffectiveBusPermissions,
    pub registry: EffectiveRegistryPermissions,
    pub network: NetworkPermissions,
    pub storage: EffectiveStoragePermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveBusPermissions {
    pub read: Vec<String>,
    pub publish: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveRegistryPermissions {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveStoragePermissions {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

impl EffectivePackagePermissions {
    pub fn for_package_manifest(manifest: &PluginPackageManifest) -> Result<Self, String> {
        manifest.validate()?;
        let permissions = manifest
            .capabilities()
            .and_then(|capabilities| capabilities.get("permissions"))
            .map(|value| {
                serde_json::from_value::<PluginPermissions>(value.clone()).map_err(|error| {
                    format!(
                        "capabilities.permissions is invalid for '{}': {error}",
                        manifest.id()
                    )
                })
            })
            .transpose()?
            .unwrap_or_default();

        Ok(Self::from_declared_permissions(manifest.id(), permissions))
    }

    fn from_declared_permissions(package_id: &str, permissions: PluginPermissions) -> Self {
        let plugin_scope = format!("plugin.{package_id}.*");
        let storage_scope = "plugin://self/*".to_string();

        let requested_bus = permissions.bus.unwrap_or_default();
        let requested_registry = permissions.registry.unwrap_or_default();
        let network = permissions.network.unwrap_or_default();
        let storage_requested = permissions
            .storage
            .iter()
            .any(|value| value == &storage_scope);

        let mut bus_read = requested_bus.read;
        push_unique(&mut bus_read, plugin_scope.clone());
        let mut bus_publish = requested_bus.publish;
        push_unique(&mut bus_publish, plugin_scope.clone());

        let mut registry_read = requested_registry.read;
        push_unique(&mut registry_read, plugin_scope.clone());
        let mut registry_write = requested_registry.write;
        push_unique(&mut registry_write, plugin_scope);

        Self {
            bus: EffectiveBusPermissions {
                read: bus_read,
                publish: bus_publish,
            },
            registry: EffectiveRegistryPermissions {
                read: registry_read,
                write: registry_write,
            },
            network,
            storage: EffectiveStoragePermissions {
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
        }
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

impl Default for EffectivePackagePermissions {
    fn default() -> Self {
        Self {
            bus: EffectiveBusPermissions {
                read: Vec::new(),
                publish: Vec::new(),
            },
            registry: EffectiveRegistryPermissions {
                read: Vec::new(),
                write: Vec::new(),
            },
            network: NetworkPermissions::default(),
            storage: EffectiveStoragePermissions {
                read: Vec::new(),
                write: Vec::new(),
            },
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
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
    fn computes_effective_permissions_from_capabilities() {
        let raw_manifest = serde_json::json!({
            "schema": "bakingrl.plugin/3",
            "id": "com.example.stats",
            "name": "Stats",
            "version": "1.0.0",
            "kind": "trusted",
            "compatibility": {
                "runtimeApi": "1.0.0"
            },
            "capabilities": {
                "permissions": {
                    "bus": {
                        "read": ["UpdateState", "plugin.com.example.stats.state"],
                        "publish": ["plugin.com.example.stats.*"]
                    },
                    "registry": {
                        "read": ["plugin.com.example.stats.state"],
                        "write": ["plugin.com.example.stats.*"]
                    },
                    "storage": ["plugin://self/*"]
                }
            }
        });
        let manifest = PluginPackageManifest::parse(&raw_manifest.to_string()).unwrap();

        let permissions = EffectivePackagePermissions::for_package_manifest(&manifest).unwrap();

        assert!(permissions.can_read_bus("UpdateState"));
        assert!(permissions.can_read_bus("plugin.com.example.stats.state"));
        assert!(permissions.can_read_bus("plugin.com.example.stats.anything"));
        assert!(permissions.can_publish_bus("plugin.com.example.stats.state"));
        assert!(permissions.can_read_registry("plugin.com.example.stats.state"));
        assert!(permissions.can_write_registry("plugin.com.example.stats.state"));
        assert_eq!(
            permissions.storage.write,
            vec!["plugin://self/*".to_string()]
        );
    }
}
