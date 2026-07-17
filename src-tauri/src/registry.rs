use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const REGISTRY_CHANGED_EVENT: &str = "bakingrl-registry-changed";

#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryEntry {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct Registry {
    store: Arc<RwLock<HashMap<String, Value>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set(&self, key: String, value: Value) {
        let mut store = self.store.write().unwrap();
        store.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        let store = self.store.read().unwrap();
        store.get(key).cloned()
    }

    pub fn entries(&self) -> Vec<RegistryEntry> {
        let store = self.store.read().unwrap();
        let mut entries: Vec<_> = store
            .iter()
            .map(|(key, value)| RegistryEntry {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries
    }
}

#[tauri::command]
pub fn registry_get(
    window: tauri::Window,
    registry: tauri::State<'_, Arc<Registry>>,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    ensure_admin_window(&window)?;
    Ok(registry.get(&key))
}

#[tauri::command]
pub fn registry_entries(
    window: tauri::Window,
    registry: tauri::State<'_, Arc<Registry>>,
) -> Result<Vec<RegistryEntry>, String> {
    ensure_admin_window(&window)?;
    Ok(registry.entries())
}

fn ensure_admin_window(window: &tauri::Window) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(format!(
            "Window '{}' cannot call admin-only registry APIs.",
            window.label()
        ))
    }
}
