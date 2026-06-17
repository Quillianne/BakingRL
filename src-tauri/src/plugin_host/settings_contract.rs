use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[cfg(not(target_os = "macos"))]
use keyring::{Entry, Error as KeyringError};
use serde_json::{Map, Value};

use crate::models::PackageSettingsFile;

use super::package_files::read_json_package_file;
use super::PackageRecord;

const KEYCHAIN_SERVICE: &str = "com.quillianne.bakingrl.plugins";
static SECRET_VALUE_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageConfigurationState {
    pub package_id: String,
    pub title: String,
    pub has_custom_page: bool,
    pub schema: Option<Value>,
    pub values: Value,
    pub secrets: Vec<PackageSecretDescriptor>,
    pub secret_store_available: bool,
    pub secret_store_error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSecretDescriptor {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub required: bool,
    pub configured: bool,
}

#[derive(Debug, Clone)]
pub struct PackageSecretDefinition {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub required: bool,
}

pub(super) fn read_package_settings_schema(
    record: &PackageRecord,
) -> Result<Option<Value>, String> {
    let settings_path = record.manifest.settings().map(ToOwned::to_owned);
    let Some(settings_path) = settings_path else {
        return Ok(None);
    };
    read_json_package_file(Path::new(&record.descriptor.path), &settings_path).map(Some)
}

pub(super) fn merge_package_settings(
    schema_path: Option<&str>,
    package_root: &Path,
    values: Value,
) -> Value {
    let schema = schema_path.and_then(|path| read_json_package_file(package_root, path).ok());
    merge_package_settings_with_schema(schema.as_ref(), values)
}

pub(super) fn merge_package_settings_with_schema(schema: Option<&Value>, values: Value) -> Value {
    let mut merged = Map::new();
    if let Some(schema) = schema {
        insert_schema_defaults(schema, &mut merged);
    }
    if let Some(values) = values.as_object() {
        let secret_keys = secret_key_set(schema);
        for (key, value) in values {
            if !secret_keys.contains(key) {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(merged)
}

pub(super) fn sanitize_package_settings_values(
    schema: Option<&Value>,
    values: Value,
) -> Result<Value, String> {
    let Some(values) = values.as_object() else {
        return Err("Package settings must be a JSON object.".to_string());
    };
    let secret_keys = secret_key_set(schema);
    for key in values.keys() {
        if secret_keys.contains(key) {
            return Err(format!(
                "Package setting '{key}' is declared as a secret and must be saved through the secret API."
            ));
        }
    }
    Ok(Value::Object(values.clone()))
}

pub(super) fn secret_definitions(schema: Option<&Value>) -> Vec<PackageSecretDefinition> {
    let Some(schema) = schema else {
        return Vec::new();
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .filter_map(|(key, property)| {
                    if !is_secret_property(property) {
                        return None;
                    }
                    Some(PackageSecretDefinition {
                        key: key.clone(),
                        label: property
                            .get("title")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| label_from_key(key)),
                        description: property
                            .get("description")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        required: required.contains(key),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn secret_key_set(schema: Option<&Value>) -> HashSet<String> {
    secret_definitions(schema)
        .into_iter()
        .map(|definition| definition.key)
        .collect()
}

pub(super) fn package_secret_configured(
    settings: &PackageSettingsFile,
    package_id: &str,
    key: &str,
) -> bool {
    settings
        .configured_secrets
        .get(package_id)
        .and_then(|secrets| secrets.get(key))
        .copied()
        .unwrap_or(false)
}

pub(super) fn read_package_secret_configured(
    settings_path: &Path,
    package_id: &str,
    key: &str,
) -> bool {
    fs::read_to_string(settings_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<PackageSettingsFile>(&raw).ok())
        .map(|settings| package_secret_configured(&settings, package_id, key))
        .unwrap_or(false)
}

pub(super) fn set_package_secret_configured(
    settings: &mut PackageSettingsFile,
    package_id: &str,
    key: &str,
    configured: bool,
) {
    if configured {
        settings
            .configured_secrets
            .entry(package_id.to_string())
            .or_default()
            .insert(key.to_string(), true);
        return;
    }

    let Some(secrets) = settings.configured_secrets.get_mut(package_id) else {
        return;
    };
    secrets.remove(key);
    if secrets.is_empty() {
        settings.configured_secrets.remove(package_id);
    }
}

pub(super) fn secret_store_status() -> Result<(), String> {
    let _ = secret_account("__bakingrl_probe__", "__probe__");
    #[cfg(not(target_os = "macos"))]
    let _ = secret_entry("__bakingrl_probe__", "__probe__")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_host::descriptors::descriptor_for_manifest;
    use crate::plugin_package::manifest::PluginPackageManifest;

    fn v4_manifest(raw: serde_json::Value) -> PluginPackageManifest {
        PluginPackageManifest::parse(&raw.to_string()).unwrap()
    }

    #[test]
    fn read_package_settings_schema_only_uses_v4_contributes_settings() {
        let package_root = std::env::temp_dir()
            .join("brl-settings-contract-schema")
            .join("v4");
        let schema_path = package_root.join("schemas").join("plugin-settings.json");
        let _ = std::fs::remove_dir_all(&package_root);
        std::fs::create_dir_all(schema_path.parent().unwrap()).unwrap();
        let raw_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string"
                }
            }
        });
        std::fs::write(&schema_path, raw_schema.to_string()).unwrap();

        let manifest = v4_manifest(serde_json::json!({
            "schemaVersion": "bakingrl.plugin/4",
            "id": "com.example.settings",
            "name": "Settings",
            "version": "1.0.0",
            "bakingrlApi": "2.0.0",
            "contributes": {
                "settings": {
                    "schema": "schemas/plugin-settings.json"
                }
            }
        }));
        let path = package_root.to_string_lossy().to_string();
        let record = super::super::PackageRecord {
            descriptor: descriptor_for_manifest(&manifest, path, true),
            manifest,
        };
        let loaded = read_package_settings_schema(&record).unwrap();

        assert_eq!(loaded, Some(raw_schema));
        let _ = std::fs::remove_dir_all(&package_root);
    }
}

#[cfg(target_os = "macos")]
pub(super) fn read_package_secret(package_id: &str, key: &str) -> Result<Option<String>, String> {
    let account = secret_account(package_id, key);
    if let Some(value) = cached_secret_value(&account) {
        return Ok(value);
    }
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &account) {
        Ok(value) => {
            let value = String::from_utf8(value)
                .map_err(|error| format!("Package secret '{key}' is not valid UTF-8: {error}"))?;
            cache_secret_value(account, Some(value.clone()));
            Ok(Some(value))
        }
        Err(error) if is_macos_no_entry(error) => {
            cache_secret_value(account, None);
            Ok(None)
        }
        Err(error) => Err(format!("Unable to read package secret '{key}': {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn read_package_secret(package_id: &str, key: &str) -> Result<Option<String>, String> {
    let account = secret_account(package_id, key);
    if let Some(value) = cached_secret_value(&account) {
        return Ok(value);
    }
    let entry = secret_entry(package_id, key)?;
    match entry.get_password() {
        Ok(value) => {
            cache_secret_value(account, Some(value.clone()));
            Ok(Some(value))
        }
        Err(KeyringError::NoEntry) => {
            cache_secret_value(account, None);
            Ok(None)
        }
        Err(error) => Err(format!("Unable to read package secret '{key}': {error}")),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn write_package_secret(package_id: &str, key: &str, value: &str) -> Result<(), String> {
    let account = secret_account(package_id, key);
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        &account,
        value.as_bytes(),
    )
    .map_err(|error| format!("Unable to write package secret '{key}': {error}"))?;
    cache_secret_value(account, Some(value.to_string()));
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn write_package_secret(package_id: &str, key: &str, value: &str) -> Result<(), String> {
    let account = secret_account(package_id, key);
    let entry = secret_entry(package_id, key)?;
    entry
        .set_password(value)
        .map_err(|error| format!("Unable to write package secret '{key}': {error}"))?;
    cache_secret_value(account, Some(value.to_string()));
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn delete_package_secret(package_id: &str, key: &str) -> Result<(), String> {
    let account = secret_account(package_id, key);
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, &account) {
        Ok(()) => {
            cache_secret_value(account, None);
            Ok(())
        }
        Err(error) if is_macos_no_entry(error) => {
            cache_secret_value(account, None);
            Ok(())
        }
        Err(error) => Err(format!("Unable to delete package secret '{key}': {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn delete_package_secret(package_id: &str, key: &str) -> Result<(), String> {
    let account = secret_account(package_id, key);
    let entry = secret_entry(package_id, key)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {
            cache_secret_value(account, None);
            Ok(())
        }
        Err(error) => Err(format!("Unable to delete package secret '{key}': {error}")),
    }
}

#[cfg(target_os = "macos")]
fn is_macos_no_entry(error: security_framework::base::Error) -> bool {
    error.code() == -25300
}

fn insert_schema_defaults(schema: &Value, output: &mut Map<String, Value>) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, property) in properties {
            if is_secret_property(property) {
                continue;
            }
            if let Some(default_value) = property.get("default") {
                output.insert(key.clone(), default_value.clone());
            }
        }
        return;
    }

    if let Some(fields) = schema.get("fields").and_then(Value::as_array) {
        for field in fields {
            if let (Some(key), Some(default_value)) = (
                field.get("key").and_then(Value::as_str),
                field.get("default"),
            ) {
                output.insert(key.to_string(), default_value.clone());
            }
        }
    }
}

fn is_secret_property(property: &Value) -> bool {
    property
        .get("x-bakingrl-secret")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn label_from_key(key: &str) -> String {
    let mut label = String::new();
    let mut previous_lower = false;
    for ch in key.replace(['_', '-'], " ").chars() {
        if ch.is_ascii_uppercase() && previous_lower {
            label.push(' ');
        }
        previous_lower = ch.is_ascii_lowercase();
        label.push(ch);
    }
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => key.to_string(),
    }
}

#[cfg(not(target_os = "macos"))]
fn secret_entry(package_id: &str, key: &str) -> Result<Entry, String> {
    Entry::new(KEYCHAIN_SERVICE, &secret_account(package_id, key))
        .map_err(|error| format!("Unable to create keychain entry for '{key}': {error}"))
}

fn secret_account(package_id: &str, key: &str) -> String {
    format!("{package_id}:{key}")
}

fn cached_secret_value(account: &str) -> Option<Option<String>> {
    SECRET_VALUE_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(account)
        .cloned()
}

fn cache_secret_value(account: String, value: Option<String>) {
    SECRET_VALUE_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .insert(account, value);
}
