#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use thiserror::Error;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{error, info, warn};

use super::diagnostics::{PluginDiagnosticInput, PluginDiagnosticSeverity, PluginDiagnosticsStore};
use super::service_registry::{
    CommandCallClient, CommandCallRequest, CommandCallRouter, ServiceCallClient,
    ServiceCallRequest, ServiceCallRouter,
};
use super::settings_contract::{read_package_secret, read_package_secret_configured};
use super::sidecar_runtime::{SidecarRuntimeController, SidecarRuntimeSpec};
use super::PluginHost;
use crate::bus::{BusEvent, EventBus};
use crate::models::GameEvent;
use crate::plugin_package::manifest::PluginRuntimeSidecarActivationV4;
use crate::registry::Registry;

const MAX_CRASHES_IN_WINDOW: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(60);
const RESTART_DELAY: Duration = Duration::from_millis(500);
const STATE_HUB_FILE: &str = "runtime-state.json";

#[derive(Debug, Error)]
pub enum ExtensionHostRuntimeError {
    #[error("Unable to find Node.js on PATH or in prepared Tauri sidecar resources.")]
    NodeNotFound,
    #[error("Unable to find built extension host bootstrap. Run `npm run extension-host:build`.")]
    BootstrapNotFound,
    #[error("Unable to resolve extension host bootstrap '{path}': {source}")]
    Bootstrap {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Unable to resolve package root '{path}': {source}")]
    PackageRoot {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Unable to resolve extension host entry '{path}': {source}")]
    Entry {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Extension host entry '{entry}' escapes package root '{root}'.")]
    EntryEscapesPackageRoot { entry: PathBuf, root: PathBuf },
    #[error("Unable to serialize extension host bootstrap spec for '{runtime_key}': {source}")]
    BootstrapSpec {
        runtime_key: String,
        source: serde_json::Error,
    },
    #[error("Unable to start extension host '{runtime_key}': {source}")]
    Spawn {
        runtime_key: String,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionHostWebviewSpec {
    pub title: Option<String>,
    pub entry: Option<String>,
    pub path: Option<String>,
    pub route: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExtensionHostRuntimeSpec {
    pub package_id: String,
    pub runtime_api: Option<semver::VersionReq>,
    pub package_root: PathBuf,
    pub entry_path: PathBuf,
    pub storage_root: PathBuf,
    pub package_settings_path: PathBuf,
    pub secret_keys: HashSet<String>,
    pub service_imports: Vec<String>,
    pub service_methods: HashMap<String, Vec<String>>,
    pub settings: serde_json::Value,
    pub sidecars: Vec<SidecarRuntimeSpec>,
    pub webviews: HashMap<String, ExtensionHostWebviewSpec>,
    pub node_path: Option<PathBuf>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

impl ExtensionHostRuntimeSpec {
    fn runtime_key(&self) -> String {
        self.package_id.clone()
    }
}

struct ExtensionHostRuntimeHandle {
    fingerprint: String,
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ExtensionHostRuntimeHandle {
    fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
    }

    fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let start = Instant::now();
            while !thread.is_finished() && start.elapsed() < Duration::from_secs(3) {
                thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            } else {
                warn!(
                    "Extension host runtime did not stop within timeout; leaving thread detached."
                );
            }
        }
    }
}

#[derive(Default)]
pub struct ExtensionHostRuntimeManager {
    handles: Mutex<HashMap<String, ExtensionHostRuntimeHandle>>,
}

impl ExtensionHostRuntimeManager {
    pub fn reload_with_app_handle(
        &self,
        specs: Vec<ExtensionHostRuntimeSpec>,
        app_handle: AppHandle,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        command_router: CommandCallRouter,
        service_router: ServiceCallRouter,
        sidecars: SidecarRuntimeController,
        diagnostics: PluginDiagnosticsStore,
    ) -> Result<(), ExtensionHostRuntimeError> {
        let mut handles = self.handles.lock().unwrap();
        let desired: HashSet<String> = specs
            .iter()
            .map(ExtensionHostRuntimeSpec::runtime_key)
            .collect();

        let stale: Vec<_> = handles
            .keys()
            .filter(|runtime_key| !desired.contains(*runtime_key))
            .cloned()
            .collect();
        for runtime_key in stale {
            if let Some(handle) = handles.remove(&runtime_key) {
                handle.shutdown();
            }
        }

        let mut first_error = None;
        for spec in specs {
            let runtime_key = spec.runtime_key();
            let fingerprint = format!("{spec:?}");
            if handles
                .get(&runtime_key)
                .is_some_and(|handle| handle.fingerprint == fingerprint && !handle.is_finished())
            {
                continue;
            }
            if let Some(handle) = handles.remove(&runtime_key) {
                handle.shutdown();
            }
            match spawn_extension_host_runtime(
                spec,
                app_handle.clone(),
                bus.clone(),
                registry.clone(),
                command_router.clone(),
                service_router.clone(),
                sidecars.clone(),
                diagnostics.clone(),
            ) {
                Ok(handle) => {
                    handles.insert(runtime_key, handle);
                }
                Err(err) => {
                    warn!("Unable to start extension host runtime: {}", err);
                    emit_runtime_error(
                        &app_handle,
                        "extensionHost",
                        &runtime_key,
                        &err.to_string(),
                    );
                    diagnostics.push(PluginDiagnosticInput {
                        package_id: Some(runtime_key.clone()),
                        source: "extensionHost".to_string(),
                        severity: PluginDiagnosticSeverity::Error,
                        phase: "activation".to_string(),
                        message: err.to_string(),
                        crash_count: None,
                    });
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }

        match first_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    pub fn start_with_app_handle(
        &self,
        spec: ExtensionHostRuntimeSpec,
        app_handle: AppHandle,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        command_router: CommandCallRouter,
        service_router: ServiceCallRouter,
        sidecars: SidecarRuntimeController,
        diagnostics: PluginDiagnosticsStore,
    ) -> Result<(), ExtensionHostRuntimeError> {
        let mut handles = self.handles.lock().unwrap();
        let runtime_key = spec.runtime_key();
        let fingerprint = format!("{spec:?}");
        if handles
            .get(&runtime_key)
            .is_some_and(|handle| handle.fingerprint == fingerprint && !handle.is_finished())
        {
            return Ok(());
        }
        if let Some(handle) = handles.remove(&runtime_key) {
            handle.shutdown();
        }
        let handle = spawn_extension_host_runtime(
            spec,
            app_handle,
            bus,
            registry,
            command_router,
            service_router,
            sidecars,
            diagnostics,
        )?;
        handles.insert(runtime_key, handle);
        Ok(())
    }

    pub fn stop(&self, package_id: &str) -> bool {
        let mut handles = self.handles.lock().unwrap();
        if let Some(handle) = handles.remove(package_id) {
            handle.shutdown();
            true
        } else {
            false
        }
    }

    pub fn stop_all(&self) {
        let mut handles = self.handles.lock().unwrap();
        for (_, handle) in handles.drain() {
            handle.shutdown();
        }
    }
}

impl Drop for ExtensionHostRuntimeManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

#[derive(Clone)]
struct ExtensionHostContext {
    package_id: String,
    runtime_api: Option<semver::VersionReq>,
    storage_root: PathBuf,
    package_settings_path: PathBuf,
    secret_keys: HashSet<String>,
    service_imports: Vec<String>,
    service_methods: HashMap<String, Vec<String>>,
    bus: Arc<EventBus>,
    bus_subscriptions: Arc<Mutex<HashSet<String>>>,
    latest_telemetry: Arc<Mutex<Option<serde_json::Value>>>,
    registry: Arc<Registry>,
    command_router: CommandCallRouter,
    command_call_tx: tokio_mpsc::Sender<CommandCallRequest>,
    registered_commands: Arc<Mutex<HashSet<String>>>,
    service_router: ServiceCallRouter,
    service_call_tx: tokio_mpsc::Sender<ServiceCallRequest>,
    registered_services: Arc<Mutex<HashSet<String>>>,
    diagnostics: PluginDiagnosticsStore,
    sidecars: SidecarRuntimeController,
    sidecar_specs: HashMap<String, SidecarRuntimeSpec>,
    webviews: HashMap<String, ExtensionHostWebviewSpec>,
    rpc: ExtensionHostRpc,
    app_handle: AppHandle,
}

#[derive(Clone, Default)]
struct ExtensionHostRpc {
    stdin: Arc<Mutex<Option<Arc<Mutex<ChildStdin>>>>>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<serde_json::Value, String>>>>>,
    next_id: Arc<AtomicU64>,
}

impl ExtensionHostRpc {
    fn set_stdin(&self, stdin: Option<Arc<Mutex<ChildStdin>>>) {
        *self.stdin.lock().unwrap() = stdin;
    }

    fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let stdin = self
            .stdin
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "Extension host process is not accepting requests.".to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (response_tx, response_rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, response_tx);
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let write_result = stdin
            .lock()
            .map_err(|_| "Extension host stdin lock is poisoned.".to_string())
            .and_then(|mut writer| {
                writeln!(writer, "{message}").map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())
            });
        if let Err(err) = write_result {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!(
                "Unable to send JSON-RPC request to extension host: {err}"
            ));
        }
        match response_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("Extension host request '{method}' timed out."))
            }
        }
    }

    fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), String> {
        let stdin = self
            .stdin
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "Extension host process is not accepting requests.".to_string())?;
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        stdin
            .lock()
            .map_err(|_| "Extension host stdin lock is poisoned.".to_string())
            .and_then(|mut writer| {
                writeln!(writer, "{message}").map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())
            })
            .map_err(|err| format!("Unable to send JSON-RPC notification to extension host: {err}"))
    }

    fn resolve_response(&self, message: &serde_json::Value) -> bool {
        let Some(id) = message.get("id").and_then(serde_json::Value::as_u64) else {
            return false;
        };
        if !message.get("result").is_some() && !message.get("error").is_some() {
            return false;
        }
        let result = if let Some(error) = message.get("error") {
            Err(error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Extension host JSON-RPC request failed.")
                .to_string())
        } else {
            Ok(message
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        };
        if let Some(response) = self.pending.lock().unwrap().remove(&id) {
            let _ = response.send(result);
        }
        true
    }

    fn reject_pending(&self, message: &str) {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for (_, response) in pending {
            let _ = response.send(Err(message.to_string()));
        }
    }
}

#[derive(Clone)]
struct ExtensionHostLaunch {
    runtime_key: String,
    node_path: PathBuf,
    bootstrap_path: PathBuf,
    package_root: PathBuf,
    args: Vec<String>,
    env: HashMap<String, String>,
    bootstrap_spec: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionHostBootstrapSpec {
    package_id: String,
    package_root: String,
    entry_url: String,
    runtime_api: Option<String>,
    storage_root: String,
    settings: serde_json::Value,
    service_imports: Vec<String>,
    service_methods: HashMap<String, Vec<String>>,
    sidecars: Vec<String>,
    webviews: HashMap<String, ExtensionHostWebviewSpec>,
}

fn spawn_extension_host_runtime(
    spec: ExtensionHostRuntimeSpec,
    app_handle: AppHandle,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    command_router: CommandCallRouter,
    service_router: ServiceCallRouter,
    sidecars: SidecarRuntimeController,
    diagnostics: PluginDiagnosticsStore,
) -> Result<ExtensionHostRuntimeHandle, ExtensionHostRuntimeError> {
    let runtime_key = spec.runtime_key();
    let fingerprint = format!("{spec:?}");
    let package_root = canonicalize_package_root(&spec.package_root)?;
    let entry_path = canonicalize_package_file(&package_root, &spec.entry_path)?;
    let node_path = resolve_node_path(&app_handle, spec.node_path.clone())?;
    let bootstrap_path = resolve_bootstrap_path(&app_handle)?;
    let entry_url = url::Url::from_file_path(&entry_path)
        .map_err(|_| ExtensionHostRuntimeError::Entry {
            path: entry_path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unable to convert entry to file URL",
            ),
        })?
        .to_string();
    fs::create_dir_all(&spec.storage_root).map_err(|source| ExtensionHostRuntimeError::Entry {
        path: spec.storage_root.clone(),
        source,
    })?;

    let sidecar_specs = spec
        .sidecars
        .iter()
        .map(|sidecar| (sidecar.sidecar_name.clone(), sidecar.clone()))
        .collect::<HashMap<_, _>>();
    let webviews = spec.webviews.clone();
    let rpc = ExtensionHostRpc::default();
    let (command_call_tx, command_call_rx) = tokio_mpsc::channel(128);
    spawn_extension_command_dispatcher(
        runtime_key.clone(),
        rpc.clone(),
        command_call_rx,
        app_handle.clone(),
    );
    let (service_call_tx, service_call_rx) = tokio_mpsc::channel(128);
    spawn_extension_service_dispatcher(
        runtime_key.clone(),
        rpc.clone(),
        service_call_rx,
        app_handle.clone(),
    );
    let bootstrap_spec = serde_json::to_string(&ExtensionHostBootstrapSpec {
        package_id: spec.package_id.clone(),
        package_root: package_root.to_string_lossy().to_string(),
        entry_url,
        runtime_api: spec
            .runtime_api
            .as_ref()
            .map(std::string::ToString::to_string),
        storage_root: spec.storage_root.to_string_lossy().to_string(),
        settings: spec.settings.clone(),
        service_imports: spec.service_imports.clone(),
        service_methods: spec.service_methods.clone(),
        sidecars: sidecar_specs.keys().cloned().collect(),
        webviews: webviews.clone(),
    })
    .map_err(|source| ExtensionHostRuntimeError::BootstrapSpec {
        runtime_key: runtime_key.clone(),
        source,
    })?;

    let launch = ExtensionHostLaunch {
        runtime_key: runtime_key.clone(),
        node_path,
        bootstrap_path,
        package_root,
        args: spec.args,
        env: spec.env,
        bootstrap_spec,
    };
    let context = ExtensionHostContext {
        package_id: spec.package_id,
        runtime_api: spec.runtime_api.clone(),
        storage_root: spec.storage_root,
        package_settings_path: spec.package_settings_path,
        secret_keys: spec.secret_keys,
        service_imports: spec.service_imports,
        service_methods: spec.service_methods,
        bus,
        bus_subscriptions: Arc::new(Mutex::new(HashSet::new())),
        latest_telemetry: Arc::new(Mutex::new(None)),
        registry,
        command_router,
        command_call_tx,
        registered_commands: Arc::new(Mutex::new(HashSet::new())),
        service_router,
        service_call_tx,
        registered_services: Arc::new(Mutex::new(HashSet::new())),
        diagnostics,
        sidecars,
        sidecar_specs,
        webviews,
        rpc,
        app_handle: app_handle.clone(),
    };

    for sidecar in context
        .sidecar_specs
        .values()
        .filter(|sidecar| sidecar.activation == PluginRuntimeSidecarActivationV4::OnEnable)
    {
        if let Err(err) = context
            .sidecars
            .start_with_app_handle(sidecar.clone(), app_handle.clone())
        {
            emit_runtime_error(
                &app_handle,
                "sidecar",
                &format!("{}/{}", context.package_id, sidecar.sidecar_name),
                &err.to_string(),
            );
            push_diagnostic(
                &context,
                PluginDiagnosticSeverity::Error,
                "activation",
                err.to_string(),
                None,
            );
        }
    }

    let child = spawn_runtime_child(&launch)?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let thread = thread::Builder::new()
        .name(format!("bakingrl-extension-host-{runtime_key}"))
        .spawn(move || supervise_runtime(launch, context, child, shutdown_rx))
        .map_err(|source| ExtensionHostRuntimeError::Spawn {
            runtime_key: runtime_key.clone(),
            source,
        })?;

    Ok(ExtensionHostRuntimeHandle {
        fingerprint,
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    })
}

fn spawn_extension_service_dispatcher(
    runtime_key: String,
    rpc: ExtensionHostRpc,
    mut service_call_rx: tokio_mpsc::Receiver<ServiceCallRequest>,
    app_handle: AppHandle,
) {
    let builder =
        thread::Builder::new().name(format!("bakingrl-extension-host-services-{runtime_key}"));
    let _ = builder.spawn(move || {
        while let Some(request) = service_call_rx.blocking_recv() {
            let result = rpc.request(
                "services/callRegistered",
                serde_json::json!({
                    "serviceRef": request.service_ref,
                    "method": request.method,
                    "input": request.input,
                }),
            );
            if let Err(message) = &result {
                emit_runtime_error(&app_handle, "extensionHost", &runtime_key, message);
            }
            let _ = request.response.send(result);
        }
    });
}

fn spawn_extension_command_dispatcher(
    runtime_key: String,
    rpc: ExtensionHostRpc,
    mut command_call_rx: tokio_mpsc::Receiver<CommandCallRequest>,
    app_handle: AppHandle,
) {
    let builder =
        thread::Builder::new().name(format!("bakingrl-extension-host-commands-{runtime_key}"));
    let _ = builder.spawn(move || {
        while let Some(request) = command_call_rx.blocking_recv() {
            let result = rpc.request(
                "commands/executeRegistered",
                serde_json::json!({
                    "command": request.command_ref,
                    "args": request.args,
                }),
            );
            if let Err(message) = &result {
                emit_runtime_error(&app_handle, "extensionHost", &runtime_key, message);
            }
            let _ = request.response.send(result);
        }
    });
}

fn spawn_runtime_child(launch: &ExtensionHostLaunch) -> Result<Child, ExtensionHostRuntimeError> {
    let mut command = Command::new(&launch.node_path);
    command
        .arg(&launch.bootstrap_path)
        .args(&launch.args)
        .current_dir(&launch.package_root)
        .env("BAKINGRL_RUNTIME_KIND", "extension-host")
        .env("BAKINGRL_PACKAGE_ID", &launch.runtime_key)
        .env("BAKINGRL_EXTENSION_HOST_SPEC", &launch.bootstrap_spec)
        .envs(&launch.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command
        .spawn()
        .map_err(|source| ExtensionHostRuntimeError::Spawn {
            runtime_key: launch.runtime_key.clone(),
            source,
        })
}

fn canonicalize_package_root(path: &Path) -> Result<PathBuf, ExtensionHostRuntimeError> {
    path.canonicalize()
        .map_err(|source| ExtensionHostRuntimeError::PackageRoot {
            path: path.to_path_buf(),
            source,
        })
}

fn canonicalize_package_file(
    package_root: &Path,
    path: &Path,
) -> Result<PathBuf, ExtensionHostRuntimeError> {
    let resolved = path
        .canonicalize()
        .map_err(|source| ExtensionHostRuntimeError::Entry {
            path: path.to_path_buf(),
            source,
        })?;
    if !resolved.starts_with(package_root) {
        return Err(ExtensionHostRuntimeError::EntryEscapesPackageRoot {
            entry: resolved,
            root: package_root.to_path_buf(),
        });
    }
    Ok(resolved)
}

fn resolve_node_path(
    app_handle: &AppHandle,
    explicit_path: Option<PathBuf>,
) -> Result<PathBuf, ExtensionHostRuntimeError> {
    if let Some(path) = explicit_path {
        return Ok(path);
    }
    if let Ok(path) = std::env::var("BAKINGRL_NODE_BINARY") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    #[cfg(debug_assertions)]
    if let Ok(path) = which::which("node") {
        return Ok(path);
    }

    let binary_name = format!("node-{}{}", target_triple(), exe_suffix());
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        candidates.push(resource_dir.join("bin").join(&binary_name));
        candidates.push(resource_dir.join(&binary_name));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("src-tauri").join("bin").join(&binary_name));
        candidates.push(current_dir.join("bin").join(&binary_name));
    }
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    which::which("node").map_err(|_| ExtensionHostRuntimeError::NodeNotFound)
}

fn resolve_bootstrap_path(app_handle: &AppHandle) -> Result<PathBuf, ExtensionHostRuntimeError> {
    if let Ok(path) = std::env::var("BAKINGRL_EXTENSION_HOST_BOOTSTRAP") {
        return canonicalize_bootstrap(PathBuf::from(path));
    }

    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        candidates.push(resource_dir.join("extension-host").join("bootstrap.mjs"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(
            current_dir
                .join("src-tauri")
                .join("gen")
                .join("extension-host")
                .join("bootstrap.mjs"),
        );
        candidates.push(
            current_dir
                .join("gen")
                .join("extension-host")
                .join("bootstrap.mjs"),
        );
    }

    for candidate in candidates {
        if candidate.exists() {
            return canonicalize_bootstrap(candidate);
        }
    }
    Err(ExtensionHostRuntimeError::BootstrapNotFound)
}

fn canonicalize_bootstrap(path: PathBuf) -> Result<PathBuf, ExtensionHostRuntimeError> {
    path.canonicalize()
        .map_err(|source| ExtensionHostRuntimeError::Bootstrap { path, source })
}

fn target_triple() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "aarch64-pc-windows-msvc"
    }
}

fn exe_suffix() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

enum ChildOutcome {
    Shutdown,
    ExitSuccess,
    Crash(String),
}

fn supervise_runtime(
    launch: ExtensionHostLaunch,
    context: ExtensionHostContext,
    first_child: Child,
    shutdown_rx: mpsc::Receiver<()>,
) {
    let mut child = Some(first_child);
    let mut crashes = VecDeque::new();

    loop {
        let running_child = match child.take() {
            Some(child) => child,
            None => match spawn_runtime_child(&launch) {
                Ok(child) => child,
                Err(err) => {
                    let message = err.to_string();
                    emit_runtime_error(
                        &context.app_handle,
                        "extensionHost",
                        &launch.runtime_key,
                        &message,
                    );
                    push_diagnostic(
                        &context,
                        PluginDiagnosticSeverity::Error,
                        "activation",
                        message,
                        None,
                    );
                    break;
                }
            },
        };

        match supervise_child_once(&launch.runtime_key, running_child, &shutdown_rx, &context) {
            ChildOutcome::Shutdown | ChildOutcome::ExitSuccess => break,
            ChildOutcome::Crash(message) => {
                let now = Instant::now();
                crashes.push_back(now);
                while crashes
                    .front()
                    .is_some_and(|first| now.duration_since(*first) > CRASH_WINDOW)
                {
                    crashes.pop_front();
                }
                let crash_count = crashes.len() as u32;
                let severity = if crashes.len() >= MAX_CRASHES_IN_WINDOW {
                    PluginDiagnosticSeverity::Fatal
                } else {
                    PluginDiagnosticSeverity::Error
                };
                push_diagnostic(
                    &context,
                    severity,
                    "runtime",
                    message.clone(),
                    Some(crash_count),
                );
                emit_runtime_error(
                    &context.app_handle,
                    "extensionHost",
                    &launch.runtime_key,
                    &message,
                );
                if crashes.len() >= MAX_CRASHES_IN_WINDOW {
                    let blocked = format!(
                        "Extension host crashed {} times in {}s; restart is blocked until package reload.",
                        crashes.len(),
                        CRASH_WINDOW.as_secs()
                    );
                    warn!("extensionHost '{}': {}", launch.runtime_key, blocked);
                    push_diagnostic(
                        &context,
                        PluginDiagnosticSeverity::Fatal,
                        "runtime",
                        blocked.clone(),
                        Some(crash_count),
                    );
                    emit_runtime_error(
                        &context.app_handle,
                        "extensionHost",
                        &launch.runtime_key,
                        &blocked,
                    );
                    break;
                }
                thread::sleep(RESTART_DELAY);
            }
        }
    }

    context.rpc.set_stdin(None);
    context
        .rpc
        .reject_pending("Extension host runtime stopped before answering.");
    let registered_services = std::mem::take(&mut *context.registered_services.lock().unwrap());
    for service_ref in registered_services {
        context.service_router.remove(&service_ref);
    }
}

fn supervise_child_once(
    runtime_key: &str,
    mut child: Child,
    shutdown_rx: &mpsc::Receiver<()>,
    context: &ExtensionHostContext,
) -> ChildOutcome {
    context.bus_subscriptions.lock().unwrap().clear();
    let stdin = child
        .stdin
        .take()
        .map(|stream| Arc::new(Mutex::new(stream)));
    context.rpc.set_stdin(stdin.clone());
    let (bus_shutdown_tx, bus_shutdown_rx) = mpsc::channel();
    let bus_forwarder =
        spawn_bus_forwarder(runtime_key.to_string(), context.clone(), bus_shutdown_rx);
    let stdout = child.stdout.take().map(|stream| {
        spawn_rpc_reader(
            runtime_key.to_string(),
            stdin.clone(),
            stream,
            context.clone(),
        )
    });
    let stderr = child.stderr.take().map(|stream| {
        spawn_log_reader(
            runtime_key.to_string(),
            "stderr",
            stream,
            context.app_handle.clone(),
        )
    });

    info!("Extension host runtime '{}' started.", runtime_key);
    let outcome = loop {
        if shutdown_rx.try_recv().is_ok() {
            if let Some(stdin) = stdin.as_ref() {
                send_jsonrpc_notification(stdin, "bakingrl/shutdown", serde_json::json!({}));
            }
            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if start.elapsed() < Duration::from_secs(2) => {
                        thread::sleep(Duration::from_millis(25));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
            info!("Extension host runtime '{}' stopped.", runtime_key);
            break ChildOutcome::Shutdown;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    info!(
                        "Extension host runtime '{}' exited with {}.",
                        runtime_key, status
                    );
                    break ChildOutcome::ExitSuccess;
                }
                let message = format!("Extension host runtime exited with {status}.");
                warn!("extensionHost '{}': {}", runtime_key, message);
                break ChildOutcome::Crash(message);
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(err) => {
                let message = format!("Unable to inspect extension host process: {err}");
                error!("extensionHost '{}': {}", runtime_key, message);
                break ChildOutcome::Crash(message);
            }
        }
    };

    if let Some(stdout) = stdout {
        let _ = stdout.join();
    }
    if let Some(stderr) = stderr {
        let _ = stderr.join();
    }
    let _ = bus_shutdown_tx.send(());
    let _ = bus_forwarder.join();
    context.bus_subscriptions.lock().unwrap().clear();
    for command_ref in context.registered_commands.lock().unwrap().drain() {
        context.command_router.remove(&command_ref);
    }
    for service_ref in context.registered_services.lock().unwrap().drain() {
        context.service_router.remove(&service_ref);
    }
    context.rpc.set_stdin(None);
    context
        .rpc
        .reject_pending("Extension host process stopped before answering.");
    outcome
}

fn spawn_bus_forwarder(
    runtime_key: String,
    context: ExtensionHostContext,
    shutdown_rx: mpsc::Receiver<()>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut bus_rx = context.bus.subscribe();
        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            match bus_rx.try_recv() {
                Ok(event) => {
                    let name = event.name().to_string();
                    let value = match event {
                        BusEvent::GameData(event) => {
                            let value =
                                serde_json::to_value(&*event).unwrap_or(serde_json::Value::Null);
                            *context.latest_telemetry.lock().unwrap() = Some(value.clone());
                            value
                        }
                        BusEvent::PluginEvent(event) => {
                            serde_json::to_value(&*event).unwrap_or(serde_json::Value::Null)
                        }
                        BusEvent::RawJson(raw) => {
                            serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null)
                        }
                    };
                    if !is_bus_subscribed(&context.bus_subscriptions, &name) {
                        continue;
                    }
                    if let Err(err) = context
                        .rpc
                        .notify("bus/event", serde_json::json!({ "event": value }))
                    {
                        warn!(
                            "Unable to forward bus event '{}' to extensionHost '{}': {}",
                            name, runtime_key, err
                        );
                        break;
                    }
                }
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(25)),
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => break,
            }
        }
    })
}

fn is_bus_subscribed(subscriptions: &Arc<Mutex<HashSet<String>>>, event_name: &str) -> bool {
    subscriptions.lock().unwrap().iter().any(|pattern| {
        pattern == "*"
            || pattern == event_name
            || pattern
                .strip_suffix(".*")
                .is_some_and(|prefix| event_name.starts_with(prefix))
    })
}

fn spawn_rpc_reader<R>(
    runtime_key: String,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    stream: R,
    context: ExtensionHostContext,
) -> thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(message) if is_jsonrpc_message(&message) => {
                    handle_jsonrpc_message(&runtime_key, &stdin, &context, message);
                }
                _ => {
                    warn!(
                        "extensionHost '{}' stdout emitted non-RPC output: {}",
                        runtime_key, line
                    );
                    emit_runtime_log(
                        &context.app_handle,
                        "extensionHost",
                        &runtime_key,
                        "stdout",
                        &line,
                    );
                }
            }
        }
    })
}

fn is_jsonrpc_message(message: &serde_json::Value) -> bool {
    message
        .get("jsonrpc")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|version| version == "2.0")
}

fn handle_jsonrpc_message(
    runtime_key: &str,
    stdin: &Option<Arc<Mutex<ChildStdin>>>,
    context: &ExtensionHostContext,
    message: serde_json::Value,
) {
    if context.rpc.resolve_response(&message) {
        return;
    }
    let Some(method) = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let id = message.get("id").cloned();
    let params = message
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let result = handle_host_request(runtime_key, context, &method, params);
    if let (Some(id), Some(stdin)) = (id, stdin.as_ref()) {
        send_jsonrpc_response(stdin, id, result);
    }
}

fn handle_host_request(
    runtime_key: &str,
    context: &ExtensionHostContext,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "commands/registerCommand" => register_command(context, params),
        "commands/unregisterCommand" => unregister_command(context, params),
        "commands/executeCommand" => call_command(context, params),
        "services/registerService" => register_service(context, params),
        "services/unregisterService" => unregister_service(context, params),
        "services/call" => call_service(context, params),
        "plugins/list" => plugins_list(context),
        "extensions/listPoints" => extensions_list_points(context, params),
        "extensions/listContributions" => extensions_list_contributions(context, params),
        "resources/list" => resources_list(context, params),
        "resources/read" => resources_read(context, params),
        "bus/subscribe" => bus_subscribe(context, params),
        "bus/unsubscribe" => bus_unsubscribe(context, params),
        "bus/emit" => bus_emit(context, params),
        "telemetryHub/subscribe" => bus_subscribe(context, params),
        "telemetryHub/unsubscribe" => bus_unsubscribe(context, params),
        "telemetryHub/publish" => telemetry_hub_publish(context, params),
        "telemetryHub/snapshot" | "telemetryHub/getSnapshot" => telemetry_hub_snapshot(context),
        "registry/get" => registry_get(context, params),
        "registry/set" => registry_set(context, params),
        "registry/entries" => registry_entries(context),
        "storage/readText" => storage_read_text(context, params),
        "storage/writeText" => storage_write_text(context, params),
        "secrets/get" => secrets_get(context, params),
        "secrets/configured" => secrets_configured(context, params),
        "diagnostics/log" => diagnostics_log(context, params),
        "sidecars/start" => sidecar_start(context, params),
        "sidecars/stop" => sidecar_stop(context, params),
        "sidecars/restart" => sidecar_restart(context, params),
        "sidecars/call" => sidecar_call(context, params),
        "telemetry/event" => telemetry_event(context, params),
        "stateHub/read" => state_hub_read(context, params),
        "stateHub/write" => state_hub_write(context, params),
        "stateHub/snapshot" | "stateHub/getSnapshot" => state_hub_snapshot(context),
        "webviews/open" => webview_open(runtime_key, context, params),
        "webviews/close" => webview_close(runtime_key, context, params),
        _ => Err(format!(
            "Extension host JSON-RPC method '{method}' is not supported."
        )),
    }
}

fn package_scoped_ref(package_id: &str, value: String, kind: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{kind} cannot be empty."));
    }
    if let Some((target_package_id, export_name)) = trimmed.split_once('/') {
        if target_package_id != package_id {
            return Err(format!(
                "Extension host '{package_id}' cannot register {kind} '{trimmed}'."
            ));
        }
        if export_name.trim().is_empty() {
            return Err(format!("{kind} '{trimmed}' must include an export name."));
        }
        return Ok(trimmed.to_string());
    }
    Ok(format!("{package_id}/{trimmed}"))
}

fn command_ref_for_context(
    context: &ExtensionHostContext,
    params: &serde_json::Value,
) -> Result<String, String> {
    package_scoped_ref(
        &context.package_id,
        required_string(params, "command")?,
        "command",
    )
}

fn register_command(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let command_ref = command_ref_for_context(context, &params)?;
    plugin_host(context)?.validate_command_call(&context.package_id, &command_ref)?;
    context.command_router.insert(
        command_ref.clone(),
        CommandCallClient::new_extension_host(
            format!("extensionHost:{}", context.package_id),
            context.command_call_tx.clone(),
        ),
    );
    context
        .registered_commands
        .lock()
        .unwrap()
        .insert(command_ref.clone());
    Ok(serde_json::json!({ "ok": true, "command": command_ref }))
}

fn unregister_command(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let command_ref = command_ref_for_context(context, &params)?;
    context.command_router.remove(&command_ref);
    context
        .registered_commands
        .lock()
        .unwrap()
        .remove(&command_ref);
    Ok(serde_json::json!({ "ok": true, "command": command_ref }))
}

fn call_command(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let command = required_string(&params, "command")?;
    let command_ref = if command.contains('/') {
        command
    } else {
        format!("{}/{}", context.package_id, command)
    };
    let args = params
        .get("args")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    tauri::async_runtime::block_on(plugin_host(context)?.call_command_export(
        &context.package_id,
        &command_ref,
        args,
    ))
}

fn register_service(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let service_ref = required_string(&params, "serviceRef")?;
    let Some((target_package_id, _)) = service_ref.split_once('/') else {
        return Err(format!(
            "Service ref '{service_ref}' must use '<package-id>/<service>'."
        ));
    };
    if target_package_id != context.package_id {
        return Err(format!(
            "Extension host '{}' cannot register service '{}'.",
            context.package_id, service_ref
        ));
    }
    if !context.service_methods.contains_key(&service_ref) {
        return Err(format!(
            "Service '{}' is not declared in contributes.services.",
            service_ref
        ));
    }
    context.service_router.insert(
        service_ref.clone(),
        ServiceCallClient::new_extension_host(
            format!("extensionHost:{}", context.package_id),
            context.service_call_tx.clone(),
        ),
    );
    context
        .registered_services
        .lock()
        .unwrap()
        .insert(service_ref.clone());
    Ok(serde_json::json!({ "ok": true }))
}

fn unregister_service(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let service_ref = required_string(&params, "serviceRef")?;
    context.service_router.remove(&service_ref);
    context
        .registered_services
        .lock()
        .unwrap()
        .remove(&service_ref);
    Ok(serde_json::json!({ "ok": true }))
}

fn call_service(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let service_ref = required_string(&params, "serviceRef")?;
    let method = required_string(&params, "method")?;
    let input = params
        .get("input")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    plugin_host(context)?.validate_service_call(&context.package_id, &service_ref, &method)?;
    tauri::async_runtime::block_on(context.service_router.call(&service_ref, method, input))
}

fn plugin_host(
    context: &ExtensionHostContext,
) -> Result<tauri::State<'_, Arc<PluginHost>>, String> {
    context
        .app_handle
        .try_state::<Arc<PluginHost>>()
        .ok_or_else(|| "Plugin host state is not available.".to_string())
}

fn plugins_list(context: &ExtensionHostContext) -> Result<serde_json::Value, String> {
    plugin_host(context)?.list_runtime_packages(&context.package_id)
}

fn extensions_list_points(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let package_id = optional_string(&params, "packageId");
    plugin_host(context)?.list_extension_points(&context.package_id, package_id.as_deref())
}

fn extensions_list_contributions(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let target = optional_string(&params, "target");
    plugin_host(context)?.list_extension_contributions(&context.package_id, target.as_deref())
}

fn resources_list(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let package_id = optional_string(&params, "packageId");
    let resource_type = optional_string(&params, "type");
    let visibility = optional_string(&params, "visibility");
    plugin_host(context)?.list_package_resources(
        &context.package_id,
        package_id.as_deref(),
        resource_type.as_deref(),
        visibility.as_deref(),
    )
}

fn resources_read(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resource_ref = required_string(&params, "ref")?;
    let path = optional_string(&params, "path");
    plugin_host(context)?.read_package_resource(&context.package_id, &resource_ref, path.as_deref())
}

fn bus_subscribe(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let event_name = required_string(&params, "eventName")?;
    context.bus_subscriptions.lock().unwrap().insert(event_name);
    Ok(serde_json::json!({ "ok": true }))
}

fn bus_unsubscribe(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let event_name = required_string(&params, "eventName")?;
    context
        .bus_subscriptions
        .lock()
        .unwrap()
        .remove(&event_name);
    Ok(serde_json::json!({ "ok": true }))
}

fn bus_emit(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let event_name = required_string(&params, "eventName")?;
    let payload = params
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    context
        .bus
        .publish(BusEvent::PluginEvent(Arc::new(GameEvent {
            event: event_name,
            data: payload,
        })));
    Ok(serde_json::json!({ "ok": true }))
}

fn telemetry_hub_publish(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let event_name = required_string(&params, "eventName")?;
    let payload = params
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    *context.latest_telemetry.lock().unwrap() = Some(serde_json::json!({
        "Event": event_name.clone(),
        "Data": payload.clone(),
    }));
    context.bus.publish(BusEvent::GameData(Arc::new(GameEvent {
        event: event_name,
        data: payload,
    })));
    Ok(serde_json::json!({ "ok": true }))
}

fn telemetry_hub_snapshot(context: &ExtensionHostContext) -> Result<serde_json::Value, String> {
    Ok(telemetry_snapshot_value(
        context.bus.as_ref(),
        context.latest_telemetry.as_ref(),
    ))
}

fn telemetry_snapshot_value(
    bus: &EventBus,
    latest_telemetry: &Mutex<Option<serde_json::Value>>,
) -> serde_json::Value {
    if let Some(event) = bus.latest_game_event() {
        return serde_json::to_value(&*event).unwrap_or(serde_json::Value::Null);
    }
    latest_telemetry
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(serde_json::Value::Null)
}

fn telemetry_event(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = required_string(&params, "name")?;
    let properties = params
        .get("properties")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    emit_runtime_log(
        &context.app_handle,
        "extensionHost",
        &context.package_id,
        "telemetry",
        &params.to_string(),
    );
    telemetry_hub_publish(
        context,
        serde_json::json!({
            "eventName": name,
            "payload": properties
        }),
    )
}

fn registry_get(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    Ok(context
        .registry
        .get(&key)
        .unwrap_or(serde_json::Value::Null))
}

fn registry_set(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    let value = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    context.registry.set(key, value);
    Ok(serde_json::json!({ "ok": true }))
}

fn registry_entries(context: &ExtensionHostContext) -> Result<serde_json::Value, String> {
    let entries = context.registry.entries().into_iter().collect::<Vec<_>>();
    serde_json::to_value(entries).map_err(|err| err.to_string())
}

fn storage_read_text(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let uri = required_string(&params, "uri")?;
    let path = resolve_storage_uri(&context.storage_root, &uri)?;
    fs::read_to_string(path)
        .map(serde_json::Value::String)
        .map_err(|err| err.to_string())
}

fn storage_write_text(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let uri = required_string(&params, "uri")?;
    let contents = params
        .get("contents")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let path = resolve_storage_uri(&context.storage_root, &uri)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, contents).map_err(|err| err.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

fn read_state_hub(
    context: &ExtensionHostContext,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let path = state_hub_path(&context.storage_root);
    match fs::read_to_string(path.clone()) {
        Ok(raw) if raw.trim().is_empty() => Ok(serde_json::Map::new()),
        Ok(raw) => {
            let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| {
                format!(
                    "Unable to read state hub JSON from '{}': {}",
                    path.display(),
                    err
                )
            })?;
            value.as_object().cloned().ok_or_else(|| {
                format!(
                    "State hub file '{}' must contain a JSON object.",
                    path.display()
                )
            })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(err) => Err(format!(
            "Unable to read state hub file '{}': {}",
            path.display(),
            err
        )),
    }
}

fn write_state_hub(
    context: &ExtensionHostContext,
    state: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let path = state_hub_path(&context.storage_root);
    let payload = serde_json::Value::Object(state);
    let raw = serde_json::to_string_pretty(&payload).map_err(|err| {
        format!(
            "Unable to serialize state hub file '{}': {}",
            path.display(),
            err
        )
    })?;
    fs::write(&path, raw).map_err(|err| {
        format!(
            "Unable to write state hub file '{}': {}",
            path.display(),
            err
        )
    })?;
    Ok(())
}

fn state_hub_path(storage_root: &Path) -> PathBuf {
    storage_root.join(STATE_HUB_FILE)
}

fn state_hub_read(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    let state = read_state_hub(context)?;
    Ok(state.get(&key).cloned().unwrap_or(serde_json::Value::Null))
}

fn state_hub_write(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    let value = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let mut state = read_state_hub(context)?;
    state.insert(key, value);
    write_state_hub(context, state)?;
    Ok(serde_json::json!({ "ok": true }))
}

fn state_hub_snapshot(context: &ExtensionHostContext) -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Object(read_state_hub(context)?))
}

fn resolve_storage_uri(storage_root: &Path, uri: &str) -> Result<PathBuf, String> {
    let relative = uri
        .strip_prefix("plugin://self/")
        .ok_or_else(|| format!("Storage URI '{uri}' must start with plugin://self/."))?;
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("Storage URI '{uri}' escapes plugin storage."));
    }
    Ok(storage_root.join(relative_path))
}

fn secrets_get(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    ensure_declared_secret_key(&context.package_id, &context.secret_keys, &key)?;
    read_package_secret(&context.package_id, &key).map(|value| match value {
        Some(value) => serde_json::Value::String(value),
        None => serde_json::Value::Null,
    })
}

fn secrets_configured(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let key = required_string(&params, "key")?;
    ensure_declared_secret_key(&context.package_id, &context.secret_keys, &key)?;
    Ok(serde_json::Value::Bool(read_package_secret_configured(
        &context.package_settings_path,
        &context.package_id,
        &key,
    )))
}

fn ensure_declared_secret_key(
    package_id: &str,
    secret_keys: &HashSet<String>,
    key: &str,
) -> Result<(), String> {
    if secret_keys.contains(key) {
        Ok(())
    } else {
        Err(format!(
            "Package '{package_id}' did not declare secret setting '{key}'."
        ))
    }
}

fn diagnostics_log(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let message = params
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let severity = params
        .get("severity")
        .and_then(serde_json::Value::as_str)
        .map(parse_diagnostic_severity)
        .unwrap_or(PluginDiagnosticSeverity::Info);
    let phase = params
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("runtime")
        .to_string();
    push_diagnostic(context, severity, &phase, message.clone(), None);
    emit_runtime_log(
        &context.app_handle,
        "extensionHost",
        &context.package_id,
        "diagnostics",
        &message,
    );
    Ok(serde_json::json!({ "ok": true }))
}

fn sidecar_start(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = sidecar_name(&params)?;
    let spec = context.sidecar_specs.get(&name).cloned().ok_or_else(|| {
        format!(
            "Sidecar '{name}' is not declared by '{}'.",
            context.package_id
        )
    })?;
    context
        .sidecars
        .start_with_app_handle(spec, context.app_handle.clone())
        .map_err(|err| err.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

fn sidecar_stop(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = sidecar_name(&params)?;
    let stopped = context.sidecars.stop_with_app_handle(
        &context.package_id,
        &name,
        context.app_handle.clone(),
    );
    Ok(serde_json::json!({ "stopped": stopped }))
}

fn sidecar_restart(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = sidecar_name(&params)?;
    if !context.sidecar_specs.contains_key(&name) {
        return Err(format!(
            "Sidecar '{name}' is not declared by '{}'.",
            context.package_id
        ));
    }
    let restarted = context
        .sidecars
        .restart_with_app_handle(&context.package_id, &name, context.app_handle.clone())
        .map_err(|err| err.to_string())?;
    if !restarted {
        let spec = context.sidecar_specs.get(&name).cloned().unwrap();
        context
            .sidecars
            .start_with_app_handle(spec, context.app_handle.clone())
            .map_err(|err| err.to_string())?;
    }
    Ok(serde_json::json!({ "ok": true }))
}

fn sidecar_call(
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = sidecar_name(&params)?;
    if !context.sidecar_specs.contains_key(&name) {
        return Err(format!(
            "Sidecar '{name}' is not declared by '{}'.",
            context.package_id
        ));
    }
    let method = required_string(&params, "method")?;
    let input = params
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    context
        .sidecars
        .call(&context.package_id, &name, &method, input)
}

fn webview_open(
    runtime_key: &str,
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = required_string(&params, "id")?;
    let webview = context.webviews.get(&id).ok_or_else(|| {
        format!(
            "Plugin '{}' does not declare webview '{id}'.",
            context.package_id
        )
    })?;
    let options = params
        .get("options")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let path = webview_route(
        &context.package_id,
        &id,
        webview,
        context.runtime_api.as_ref(),
    )?;
    let label = webview_window_label(&context.package_id, &id);
    let title = webview_option_string(&options, "title")
        .or_else(|| webview.title.clone())
        .unwrap_or_else(|| format!("{} - {id}", context.package_id));
    let width = webview_option_number(&options, "width", 960.0);
    let height = webview_option_number(&options, "height", 640.0);
    open_webview_window(&context.app_handle, &label, &path, &title, width, height)?;
    let payload = serde_json::json!({
        "packageId": context.package_id,
        "id": id,
        "options": options,
    });
    context
        .app_handle
        .emit("bakingrl-plugin-webview-open", payload)
        .map_err(|err| err.to_string())?;
    emit_runtime_log(
        &context.app_handle,
        "extensionHost",
        runtime_key,
        "webview",
        &format!("open {id}"),
    );
    Ok(serde_json::json!({ "ok": true }))
}

fn webview_close(
    runtime_key: &str,
    context: &ExtensionHostContext,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = required_string(&params, "id")?;
    let label = webview_window_label(&context.package_id, &id);
    if let Some(window) = context.app_handle.get_webview_window(&label) {
        window.close().map_err(|err| err.to_string())?;
    }
    let payload = serde_json::json!({
        "packageId": context.package_id,
        "id": id,
    });
    context
        .app_handle
        .emit("bakingrl-plugin-webview-close", payload)
        .map_err(|err| err.to_string())?;
    emit_runtime_log(
        &context.app_handle,
        "extensionHost",
        runtime_key,
        "webview",
        &format!("close {id}"),
    );
    Ok(serde_json::json!({ "ok": true }))
}

fn webview_route(
    package_id: &str,
    id: &str,
    webview: &ExtensionHostWebviewSpec,
    runtime_api: Option<&semver::VersionReq>,
) -> Result<String, String> {
    if let Some(route) = webview
        .route
        .as_deref()
        .filter(|route| route.starts_with('/'))
    {
        return Ok(route.to_string());
    }
    let source = webview
        .entry
        .as_deref()
        .map(|entry| ("entry", entry))
        .or_else(|| webview.path.as_deref().map(|path| ("path", path)))
        .ok_or_else(|| format!("Webview '{id}' must declare entry or path."))?;
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair(source.0, source.1);
    if let Some(runtime_api) = runtime_api {
        query.append_pair("runtimeApi", &runtime_api.to_string());
    }
    Ok(format!(
        "/plugin-webview/{}/{}?{}",
        encode_path_segment(package_id),
        encode_path_segment(id),
        query.finish()
    ))
}

fn open_webview_window(
    app_handle: &AppHandle,
    label: &str,
    path: &str,
    title: &str,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let js_path = serde_json::to_string(path).map_err(|err| err.to_string())?;
    if let Some(window) = app_handle.get_webview_window(label) {
        window
            .eval(format!("window.location.href = {js_path};"))
            .map_err(|err| err.to_string())?;
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(app_handle, label, WebviewUrl::App(PathBuf::from(path)))
        .title(title)
        .inner_size(width.max(480.0), height.max(320.0))
        .min_inner_size(480.0, 320.0)
        .decorations(false)
        .resizable(true)
        .visible(true)
        .build()
        .map_err(|err| err.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

fn webview_window_label(package_id: &str, id: &str) -> String {
    let safe: String = format!("{package_id}-{id}")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!("plugin-webview-{safe}")
}

fn encode_path_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn webview_option_number(options: &serde_json::Value, key: &str, fallback: f64) -> f64 {
    options
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| *value > 0.0)
        .unwrap_or(fallback)
}

fn webview_option_string(options: &serde_json::Value, key: &str) -> Option<String> {
    options
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn sidecar_name(params: &serde_json::Value) -> Result<String, String> {
    params
        .get("name")
        .or_else(|| params.get("id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Missing sidecar name.".to_string())
}

fn required_string(params: &serde_json::Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Missing required string parameter '{key}'."))
}

fn optional_string(params: &serde_json::Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_diagnostic_severity(value: &str) -> PluginDiagnosticSeverity {
    match value {
        "warning" | "warn" => PluginDiagnosticSeverity::Warning,
        "error" => PluginDiagnosticSeverity::Error,
        "fatal" => PluginDiagnosticSeverity::Fatal,
        _ => PluginDiagnosticSeverity::Info,
    }
}

fn push_diagnostic(
    context: &ExtensionHostContext,
    severity: PluginDiagnosticSeverity,
    phase: &str,
    message: String,
    crash_count: Option<u32>,
) {
    context.diagnostics.push(PluginDiagnosticInput {
        package_id: Some(context.package_id.clone()),
        source: "extensionHost".to_string(),
        severity,
        phase: phase.to_string(),
        message,
        crash_count,
    });
}

fn send_jsonrpc_notification(
    stdin: &Arc<Mutex<ChildStdin>>,
    method: &str,
    params: serde_json::Value,
) {
    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    if let Ok(mut writer) = stdin.lock() {
        let _ = writeln!(writer, "{message}");
        let _ = writer.flush();
    }
}

fn send_jsonrpc_response(
    stdin: &Arc<Mutex<ChildStdin>>,
    id: serde_json::Value,
    result: Result<serde_json::Value, String>,
) {
    let message = match result {
        Ok(value) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": value
        }),
        Err(message) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": message
            }
        }),
    };
    if let Ok(mut writer) = stdin.lock() {
        let _ = writeln!(writer, "{message}");
        let _ = writer.flush();
    }
}

fn spawn_log_reader<R>(
    runtime_key: String,
    stream_name: &'static str,
    stream: R,
    app_handle: AppHandle,
) -> thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            warn!("extensionHost '{}' {}: {}", runtime_key, stream_name, line);
            emit_runtime_log(
                &app_handle,
                "extensionHost",
                &runtime_key,
                stream_name,
                &line,
            );
        }
    })
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeLogPayload<'a> {
    kind: &'a str,
    source: &'a str,
    stream: &'a str,
    line: &'a str,
}

fn emit_runtime_log(app_handle: &AppHandle, kind: &str, source: &str, stream: &str, line: &str) {
    let payload = RuntimeLogPayload {
        kind,
        source,
        stream,
        line,
    };
    let _ = app_handle.emit("bakingrl-runtime-log", payload);
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeErrorPayload<'a> {
    kind: &'a str,
    source: &'a str,
    message: &'a str,
    timestamp_ms: u64,
}

fn emit_runtime_error(app_handle: &AppHandle, kind: &str, source: &str, message: &str) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let payload = RuntimeErrorPayload {
        kind,
        source,
        message,
        timestamp_ms,
    };
    let _ = app_handle.emit("bakingrl-runtime-error", payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_snapshot_prefers_latest_bus_game_event() {
        let bus = EventBus::new(16);
        let latest_telemetry = Mutex::new(Some(serde_json::json!({
            "Event": "Fallback",
            "Data": { "source": "forwarder" }
        })));

        bus.publish(BusEvent::GameData(Arc::new(GameEvent {
            event: "UpdateState".to_string(),
            data: serde_json::json!({ "MatchGuid": "mock-match" }),
        })));

        let snapshot = telemetry_snapshot_value(&bus, &latest_telemetry);
        assert_eq!(snapshot["Event"], "UpdateState");
        assert_eq!(snapshot["Data"]["MatchGuid"], "mock-match");
    }

    #[test]
    fn telemetry_snapshot_uses_forwarded_value_until_bus_has_snapshot() {
        let bus = EventBus::new(16);
        let latest_telemetry = Mutex::new(Some(serde_json::json!({
            "Event": "BallHit",
            "Data": { "Speed": 321 }
        })));

        let snapshot = telemetry_snapshot_value(&bus, &latest_telemetry);
        assert_eq!(snapshot["Event"], "BallHit");
        assert_eq!(snapshot["Data"]["Speed"], 321);
    }

    #[test]
    fn telemetry_snapshot_ignores_plugin_bus_events() {
        let bus = EventBus::new(16);
        let latest_telemetry = Mutex::new(None);

        bus.publish(BusEvent::GameData(Arc::new(GameEvent {
            event: "UpdateState".to_string(),
            data: serde_json::json!({ "MatchGuid": "rl-snapshot" }),
        })));
        bus.publish(BusEvent::PluginEvent(Arc::new(GameEvent {
            event: "plugin.example.state".to_string(),
            data: serde_json::json!({ "status": "ready" }),
        })));

        let snapshot = telemetry_snapshot_value(&bus, &latest_telemetry);
        assert_eq!(snapshot["Event"], "UpdateState");
        assert_eq!(snapshot["Data"]["MatchGuid"], "rl-snapshot");
    }

    #[test]
    fn package_scoped_ref_expands_local_command_names() {
        assert_eq!(
            package_scoped_ref("bakingrl.pkg", "openSettings".to_string(), "command").unwrap(),
            "bakingrl.pkg/openSettings"
        );
        assert_eq!(
            package_scoped_ref(
                "bakingrl.pkg",
                "bakingrl.pkg/openSettings".to_string(),
                "command"
            )
            .unwrap(),
            "bakingrl.pkg/openSettings"
        );
    }

    #[test]
    fn package_scoped_ref_rejects_foreign_refs() {
        let error = package_scoped_ref(
            "bakingrl.pkg",
            "bakingrl.other/openSettings".to_string(),
            "command",
        )
        .unwrap_err();
        assert!(error.contains("cannot register command"));
    }

    #[test]
    fn secret_key_guard_rejects_undeclared_keys() {
        let secret_keys = HashSet::from(["apiKey".to_string()]);

        assert!(ensure_declared_secret_key("bakingrl.pkg", &secret_keys, "apiKey").is_ok());

        let error =
            ensure_declared_secret_key("bakingrl.pkg", &secret_keys, "missingKey").unwrap_err();
        assert_eq!(
            error,
            "Package 'bakingrl.pkg' did not declare secret setting 'missingKey'."
        );
    }
}
