use std::collections::HashMap;
use std::path::{Path, PathBuf};

use deno_core::{
    resolve_import, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, RequestedModuleType,
    ResolutionKind,
};
use deno_error::JsErrorBox;

const MAX_RUNTIME_MODULE_BYTES: u64 = 10 * 1024 * 1024;

pub(super) struct PackageModuleLoader {
    allowed_roots: Vec<PathBuf>,
    virtual_modules: HashMap<String, Vec<u8>>,
}

impl PackageModuleLoader {
    pub(super) fn from_entry_paths<I>(entry_paths: I, storage_root: &Path) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let mut allowed_roots = Vec::new();
        if storage_root
            .file_name()
            .is_some_and(|name| name == "storage")
            && storage_root
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == ".bakingrl")
        {
            if let Some(package_root) = storage_root.parent().and_then(Path::parent) {
                push_canonical_root(&mut allowed_roots, package_root);
            }
        }
        for entry_path in entry_paths {
            if let Some(parent) = entry_path.parent() {
                push_canonical_root(&mut allowed_roots, parent);
            }
        }
        Self {
            allowed_roots,
            virtual_modules: HashMap::new(),
        }
    }

    pub(super) fn with_virtual_module(mut self, specifier: &ModuleSpecifier, code: String) -> Self {
        self.virtual_modules
            .insert(specifier.to_string(), code.into_bytes());
        self
    }

    fn allowed_path(&self, specifier: &ModuleSpecifier) -> Result<PathBuf, JsErrorBox> {
        if specifier.scheme() != "file" {
            return Err(JsErrorBox::generic(format!(
                "Runtime module import '{}' is not a file URL.",
                specifier
            )));
        }
        let path = specifier.to_file_path().map_err(|_| {
            JsErrorBox::generic(format!(
                "Runtime module import '{}' is not a valid file path.",
                specifier
            ))
        })?;
        let resolved = path.canonicalize().map_err(|e| {
            JsErrorBox::generic(format!(
                "Unable to resolve runtime module '{}': {e}",
                path.display()
            ))
        })?;
        if self
            .allowed_roots
            .iter()
            .any(|root| resolved.starts_with(root))
        {
            Ok(resolved)
        } else {
            Err(JsErrorBox::generic(format!(
                "Runtime module '{}' is outside the plugin package.",
                resolved.display()
            )))
        }
    }
}

impl ModuleLoader for PackageModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        let resolved = resolve_import(specifier, referrer).map_err(JsErrorBox::from_err)?;
        if self.virtual_modules.contains_key(resolved.as_str()) {
            return Ok(resolved);
        }
        self.allowed_path(&resolved)?;
        Ok(resolved)
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleLoadReferrer>,
        options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        if let Some(code) = self.virtual_modules.get(module_specifier.as_str()) {
            return ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::Bytes(code.clone().into_boxed_slice().into()),
                module_specifier,
                None,
            )));
        }
        let path = match self.allowed_path(module_specifier) {
            Ok(path) => path,
            Err(err) => return ModuleLoadResponse::Sync(Err(err)),
        };
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                    "Unable to inspect runtime module '{}': {err}",
                    path.display()
                ))));
            }
        };
        if metadata.len() > MAX_RUNTIME_MODULE_BYTES {
            return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "Runtime module '{}' exceeds the module size limit.",
                path.display()
            ))));
        }
        let module_type = match module_type_for_path(&path, &options) {
            Ok(module_type) => module_type,
            Err(err) => return ModuleLoadResponse::Sync(Err(err)),
        };
        match std::fs::read(&path) {
            Ok(code) => ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(code.into_boxed_slice().into()),
                module_specifier,
                None,
            ))),
            Err(err) => ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "Unable to read runtime module '{}': {err}",
                path.display()
            )))),
        }
    }
}

fn module_type_for_path(
    path: &Path,
    options: &ModuleLoadOptions,
) -> Result<ModuleType, JsErrorBox> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase());
    if extension.as_deref() == Some("json") {
        if options.requested_module_type != RequestedModuleType::Json {
            return Err(JsErrorBox::generic(
                "Attempted to load JSON module without specifying a JSON import attribute.",
            ));
        }
        return Ok(ModuleType::Json);
    }
    if extension.as_deref() == Some("wasm") {
        return Ok(ModuleType::Wasm);
    }
    Ok(match &options.requested_module_type {
        RequestedModuleType::Other(module_type) => ModuleType::Other(module_type.clone()),
        RequestedModuleType::Text => ModuleType::Text,
        RequestedModuleType::Bytes => ModuleType::Bytes,
        _ => ModuleType::JavaScript,
    })
}

fn push_canonical_root(roots: &mut Vec<PathBuf>, path: &Path) {
    let Ok(root) = path.canonicalize() else {
        return;
    };
    if roots.iter().any(|existing| root.starts_with(existing)) {
        return;
    }
    roots.retain(|existing| !existing.starts_with(&root));
    roots.push(root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_modules_outside_allowed_roots() {
        let package_dir = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let entry_path = package_dir.path().join("dist/service.js");
        let outside_path = outside_dir.path().join("outside.js");
        std::fs::create_dir_all(entry_path.parent().unwrap()).unwrap();
        std::fs::write(&entry_path, "export default {};").unwrap();
        std::fs::write(&outside_path, "export default {};").unwrap();

        let loader = PackageModuleLoader::from_entry_paths(
            vec![entry_path.clone()],
            &package_dir.path().join(".bakingrl").join("storage"),
        );
        let referrer = ModuleSpecifier::from_file_path(&entry_path)
            .unwrap()
            .to_string();
        assert!(loader
            .resolve("./service.js", &referrer, ResolutionKind::Import)
            .is_ok());

        let outside = ModuleSpecifier::from_file_path(&outside_path)
            .unwrap()
            .to_string();
        assert!(loader
            .resolve(&outside, &referrer, ResolutionKind::DynamicImport)
            .is_err());
    }
}
