#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tracing::{error, info, warn};

use crate::plugin_package::manifest::PluginRuntimeSidecarActivationV4;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRuntimeStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
    pub restart_count: u32,
    pub crash_count: u32,
}

impl Default for SidecarRuntimeStatus {
    fn default() -> Self {
        Self {
            running: false,
            last_exit_code: None,
            restart_count: 0,
            crash_count: 0,
        }
    }
}

#[derive(Default)]
struct SidecarRuntimeState {
    status_by_ref: HashMap<String, SidecarRuntimeStatus>,
    package_state: HashMap<String, HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarProtocol {
    JsonRpcStdio,
}

#[derive(Debug, Error)]
pub enum SidecarRuntimeError {
    #[error("Sidecar '{sidecar_ref}' uses unsupported protocol '{protocol}'.")]
    UnsupportedProtocol {
        sidecar_ref: String,
        protocol: String,
    },
    #[error("Unable to resolve package root '{path}': {source}")]
    PackageRoot {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Unable to resolve sidecar binary '{path}': {source}")]
    Binary {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Sidecar binary '{binary}' escapes package root '{root}'.")]
    BinaryEscapesPackageRoot { binary: PathBuf, root: PathBuf },
    #[error("Unable to start sidecar '{sidecar_ref}': {source}")]
    Spawn {
        sidecar_ref: String,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone)]
pub struct SidecarRuntimeSpec {
    pub package_id: String,
    pub sidecar_name: String,
    pub runtime_api: Option<semver::VersionReq>,
    pub package_root: PathBuf,
    pub binary_path: PathBuf,
    pub protocol: SidecarProtocol,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub activation: PluginRuntimeSidecarActivationV4,
}

impl SidecarRuntimeSpec {
    fn sidecar_ref(&self) -> String {
        format!("{}/{}", self.package_id, self.sidecar_name)
    }
}

struct SidecarRuntimeHandle {
    spec: SidecarRuntimeSpec,
    fingerprint: String,
    rpc: SidecarRpc,
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SidecarRuntimeHandle {
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
            let start = std::time::Instant::now();
            while !thread.is_finished() && start.elapsed() < Duration::from_secs(2) {
                thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            } else {
                warn!("Sidecar runtime did not stop within timeout; leaving thread detached.");
            }
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct SidecarRpc {
    stdin: Arc<Mutex<Option<Arc<Mutex<ChildStdin>>>>>,
    pending: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<serde_json::Value, String>>>>>,
    next_id: Arc<AtomicU64>,
}

impl SidecarRpc {
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
            .ok_or_else(|| "Sidecar process is not accepting requests.".to_string())?;
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
            .map_err(|_| "Sidecar stdin lock is poisoned.".to_string())
            .and_then(|mut writer| {
                writeln!(writer, "{message}").map_err(|err| err.to_string())?;
                writer.flush().map_err(|err| err.to_string())
            });
        if let Err(err) = write_result {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("Unable to send JSON-RPC request to sidecar: {err}"));
        }
        match response_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("Sidecar request '{method}' timed out."))
            }
        }
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
                .unwrap_or("Sidecar JSON-RPC request failed.")
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

#[derive(Default)]
pub struct SidecarRuntimeManager {
    controller: SidecarRuntimeController,
}

impl SidecarRuntimeManager {
    pub(crate) fn controller(&self) -> SidecarRuntimeController {
        self.controller.clone()
    }

    pub fn reload_with_app_handle(
        &self,
        specs: Vec<SidecarRuntimeSpec>,
        app_handle: AppHandle,
    ) -> Result<(), SidecarRuntimeError> {
        self.controller.reload_with_app_handle(specs, app_handle)
    }

    pub fn start_with_app_handle(
        &self,
        spec: SidecarRuntimeSpec,
        app_handle: AppHandle,
    ) -> Result<(), SidecarRuntimeError> {
        self.controller.start_with_app_handle(spec, app_handle)
    }

    pub fn restart_with_app_handle(
        &self,
        package_id: &str,
        sidecar_name: &str,
        app_handle: AppHandle,
    ) -> Result<bool, SidecarRuntimeError> {
        self.controller
            .restart_with_app_handle(package_id, sidecar_name, app_handle)
    }

    pub fn stop(&self, package_id: &str, sidecar_name: &str) -> bool {
        self.controller.stop(package_id, sidecar_name)
    }

    pub fn call(
        &self,
        package_id: &str,
        sidecar_name: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.controller
            .call(package_id, sidecar_name, method, params)
    }

    pub fn status(&self, package_id: &str, sidecar_name: &str) -> Option<SidecarRuntimeStatus> {
        self.controller.status(package_id, sidecar_name)
    }

    pub fn status_map(&self) -> HashMap<String, SidecarRuntimeStatus> {
        self.controller.status_map()
    }

    pub fn state_get(&self, package_id: &str, key: &str) -> Option<serde_json::Value> {
        self.controller.state_get(package_id, key)
    }

    pub fn state_set(&self, package_id: &str, key: String, value: serde_json::Value) {
        self.controller.state_set(package_id, key, value)
    }

    pub fn stop_all(&self) {
        self.controller.stop_all();
    }
}

impl Drop for SidecarRuntimeManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

#[derive(Clone, Default)]
pub(crate) struct SidecarRuntimeController {
    handles: Arc<Mutex<HashMap<String, SidecarRuntimeHandle>>>,
    state: Arc<Mutex<SidecarRuntimeState>>,
}

impl SidecarRuntimeController {
    fn update_status<F>(&self, sidecar_ref: &str, update: F)
    where
        F: FnOnce(&mut SidecarRuntimeStatus),
    {
        let mut state = self.state.lock().unwrap();
        let status = state
            .status_by_ref
            .entry(sidecar_ref.to_string())
            .or_default();
        update(status);
    }

    fn status(&self, package_id: &str, sidecar_name: &str) -> Option<SidecarRuntimeStatus> {
        let sidecar_ref = format!("{package_id}/{sidecar_name}");
        self.state
            .lock()
            .unwrap()
            .status_by_ref
            .get(&sidecar_ref)
            .cloned()
    }

    fn status_map(&self) -> HashMap<String, SidecarRuntimeStatus> {
        self.state.lock().unwrap().status_by_ref.clone()
    }

    fn state_get(&self, package_id: &str, key: &str) -> Option<serde_json::Value> {
        self.state
            .lock()
            .unwrap()
            .package_state
            .get(package_id)
            .and_then(|state| state.get(key).cloned())
    }

    fn state_set(&self, package_id: &str, key: String, value: serde_json::Value) {
        let mut state = self.state.lock().unwrap();
        state
            .package_state
            .entry(package_id.to_string())
            .or_default()
            .insert(key, value);
    }

    fn on_start(&self, sidecar_ref: &str) {
        self.update_status(sidecar_ref, |status| {
            status.running = true;
            status.last_exit_code = None;
        });
    }

    fn on_stop(&self, sidecar_ref: &str, exit_code: Option<i32>) {
        self.update_status(sidecar_ref, |status| {
            status.running = false;
            status.last_exit_code = exit_code;
        });
    }

    fn on_crash(&self, sidecar_ref: &str, exit_code: Option<i32>) {
        self.update_status(sidecar_ref, |status| {
            status.running = false;
            status.last_exit_code = exit_code;
            status.crash_count = status.crash_count.saturating_add(1);
        });
    }

    fn on_restart(&self, sidecar_ref: &str) {
        self.update_status(sidecar_ref, |status| {
            status.restart_count = status.restart_count.saturating_add(1);
        });
    }

    pub fn reload_with_app_handle(
        &self,
        specs: Vec<SidecarRuntimeSpec>,
        app_handle: AppHandle,
    ) -> Result<(), SidecarRuntimeError> {
        let mut handles = self.handles.lock().unwrap();
        let desired: HashSet<String> = specs.iter().map(SidecarRuntimeSpec::sidecar_ref).collect();

        let stale: Vec<_> = handles
            .keys()
            .filter(|sidecar_ref| !desired.contains(*sidecar_ref))
            .cloned()
            .collect();
        for sidecar_ref in stale {
            self.on_stop(&sidecar_ref, None);
            if let Some(handle) = handles.remove(&sidecar_ref) {
                handle.shutdown();
            }
        }

        let mut first_error = None;
        for spec in specs {
            let sidecar_ref = spec.sidecar_ref();
            let fingerprint = format!("{spec:?}");
            if handles
                .get(&sidecar_ref)
                .is_some_and(|handle| handle.fingerprint == fingerprint && !handle.is_finished())
            {
                continue;
            }
            if let Some(handle) = handles.remove(&sidecar_ref) {
                handle.shutdown();
            }
            self.on_stop(&sidecar_ref, None);
            match spawn_sidecar_runtime(spec, app_handle.clone(), self.state.clone()) {
                Ok(handle) => {
                    self.on_start(&sidecar_ref);
                    handles.insert(sidecar_ref, handle);
                }
                Err(err) => {
                    warn!("Unable to start sidecar runtime: {}", err);
                    emit_runtime_error(&app_handle, "sidecar", &sidecar_ref, &err.to_string());
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
        spec: SidecarRuntimeSpec,
        app_handle: AppHandle,
    ) -> Result<(), SidecarRuntimeError> {
        let mut handles = self.handles.lock().unwrap();
        let sidecar_ref = spec.sidecar_ref();
        let fingerprint = format!("{spec:?}");
        if handles
            .get(&sidecar_ref)
            .is_some_and(|handle| handle.fingerprint == fingerprint && !handle.is_finished())
        {
            return Ok(());
        }
        if let Some(handle) = handles.remove(&sidecar_ref) {
            self.on_stop(&sidecar_ref, None);
            handle.shutdown();
        }
        match spawn_sidecar_runtime(spec, app_handle.clone(), self.state.clone()) {
            Ok(handle) => {
                self.on_start(&sidecar_ref);
                handles.insert(sidecar_ref, handle);
                Ok(())
            }
            Err(err) => {
                warn!("Unable to start sidecar runtime: {}", err);
                emit_runtime_error(&app_handle, "sidecar", &sidecar_ref, &err.to_string());
                Err(err)
            }
        }
    }

    pub fn restart_with_app_handle(
        &self,
        package_id: &str,
        sidecar_name: &str,
        app_handle: AppHandle,
    ) -> Result<bool, SidecarRuntimeError> {
        let sidecar_ref = format!("{package_id}/{sidecar_name}");
        let spec = {
            let mut handles = self.handles.lock().unwrap();
            let Some(handle) = handles.remove(&sidecar_ref) else {
                return Ok(false);
            };
            let spec = handle.spec.clone();
            self.on_restart(&sidecar_ref);
            handle.shutdown();
            spec
        };
        self.start_with_app_handle(spec, app_handle)?;
        Ok(true)
    }

    pub fn stop(&self, package_id: &str, sidecar_name: &str) -> bool {
        let sidecar_ref = format!("{package_id}/{sidecar_name}");
        let mut handles = self.handles.lock().unwrap();
        self.on_stop(&sidecar_ref, None);
        if let Some(handle) = handles.remove(&sidecar_ref) {
            handle.shutdown();
            true
        } else {
            false
        }
    }

    pub fn call(
        &self,
        package_id: &str,
        sidecar_name: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let sidecar_ref = format!("{package_id}/{sidecar_name}");
        let rpc = self
            .handles
            .lock()
            .unwrap()
            .get(&sidecar_ref)
            .map(|handle| handle.rpc.clone())
            .ok_or_else(|| format!("Sidecar runtime '{sidecar_ref}' is not running."))?;
        rpc.request(method, params)
    }

    pub fn stop_all(&self) {
        let mut handles = self.handles.lock().unwrap();
        for (_, handle) in handles.drain() {
            self.on_stop(&handle.spec.sidecar_ref(), None);
            handle.shutdown();
        }
    }
}

fn spawn_sidecar_runtime(
    spec: SidecarRuntimeSpec,
    app_handle: AppHandle,
    runtime_status: Arc<Mutex<SidecarRuntimeState>>,
) -> Result<SidecarRuntimeHandle, SidecarRuntimeError> {
    let sidecar_ref = spec.sidecar_ref();
    if spec.protocol != SidecarProtocol::JsonRpcStdio {
        return Err(SidecarRuntimeError::UnsupportedProtocol {
            sidecar_ref,
            protocol: format!("{:?}", spec.protocol),
        });
    }

    let fingerprint = format!("{spec:?}");
    let package_root = canonicalize_package_root(&spec.package_root)?;
    let binary_path = canonicalize_package_file(&package_root, &spec.binary_path)?;
    let rpc = SidecarRpc::default();

    let mut command = Command::new(binary_path);
    command
        .args(&spec.args)
        .current_dir(&package_root)
        .env("BAKINGRL_SIDECAR_PROTOCOL", "jsonrpc-stdio")
        .env("BAKINGRL_PACKAGE_ID", &spec.package_id)
        .env("BAKINGRL_SIDECAR_NAME", &spec.sidecar_name)
        .envs(&spec.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = command
        .spawn()
        .map_err(|source| SidecarRuntimeError::Spawn {
            sidecar_ref: sidecar_ref.clone(),
            source,
        })?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let supervisor_rpc = rpc.clone();
    let thread = thread::Builder::new()
        .name(format!("bakingrl-sidecar-{sidecar_ref}"))
        .spawn(move || {
            supervise_child(
                sidecar_ref,
                child,
                shutdown_rx,
                app_handle,
                supervisor_rpc,
                runtime_status,
            )
        })
        .map_err(|source| SidecarRuntimeError::Spawn {
            sidecar_ref: spec.sidecar_ref(),
            source,
        })?;

    Ok(SidecarRuntimeHandle {
        spec,
        fingerprint,
        rpc,
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    })
}

fn canonicalize_package_root(path: &Path) -> Result<PathBuf, SidecarRuntimeError> {
    path.canonicalize()
        .map_err(|source| SidecarRuntimeError::PackageRoot {
            path: path.to_path_buf(),
            source,
        })
}

fn canonicalize_package_file(
    package_root: &Path,
    path: &Path,
) -> Result<PathBuf, SidecarRuntimeError> {
    let resolved = path
        .canonicalize()
        .map_err(|source| SidecarRuntimeError::Binary {
            path: path.to_path_buf(),
            source,
        })?;
    if !resolved.starts_with(package_root) {
        return Err(SidecarRuntimeError::BinaryEscapesPackageRoot {
            binary: resolved,
            root: package_root.to_path_buf(),
        });
    }
    Ok(resolved)
}

fn supervise_child(
    sidecar_ref: String,
    mut child: Child,
    shutdown_rx: mpsc::Receiver<()>,
    app_handle: AppHandle,
    rpc: SidecarRpc,
    runtime_status: Arc<Mutex<SidecarRuntimeState>>,
) {
    let stdin = child
        .stdin
        .take()
        .map(|stream| Arc::new(Mutex::new(stream)));
    rpc.set_stdin(stdin.clone());
    let stdout = child.stdout.take().map(|stream| {
        spawn_stdout_reader(
            sidecar_ref.clone(),
            stdin.clone(),
            stream,
            app_handle.clone(),
            rpc.clone(),
            runtime_status.clone(),
        )
    });
    let stderr = child
        .stderr
        .take()
        .map(|stream| spawn_log_reader(sidecar_ref.clone(), "stderr", stream, app_handle.clone()));

    info!("Sidecar runtime '{}' started.", sidecar_ref);
    loop {
        if shutdown_rx.try_recv().is_ok() {
            if let Some(stdin) = stdin.as_ref() {
                send_jsonrpc_notification(stdin, "bakingrl/shutdown", serde_json::json!({}));
            }
            let start = std::time::Instant::now();
            let sidecar_status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        break status.code();
                    }
                    Ok(None) if start.elapsed() < Duration::from_secs(2) => {
                        thread::sleep(Duration::from_millis(25));
                    }
                    _ => {
                        let _ = child.kill();
                        break child.wait().ok().and_then(|status| status.code());
                    }
                }
            };
            set_sidecar_stopped(&runtime_status, &sidecar_ref, sidecar_status);
            info!("Sidecar runtime '{}' stopped.", sidecar_ref);
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                if status.success() {
                    info!("Sidecar runtime '{}' exited with {}.", sidecar_ref, status);
                    set_sidecar_stopped(&runtime_status, &sidecar_ref, exit_code);
                } else {
                    let message = format!("Sidecar runtime exited with {status}.");
                    set_sidecar_crash(&runtime_status, &sidecar_ref, exit_code);
                    warn!("Sidecar '{}': {}", sidecar_ref, message);
                    emit_runtime_error(&app_handle, "sidecar", &sidecar_ref, &message);
                }
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(err) => {
                let message = format!("Unable to inspect sidecar process: {err}");
                set_sidecar_crash(&runtime_status, &sidecar_ref, None);
                error!("Sidecar '{}': {}", sidecar_ref, message);
                emit_runtime_error(&app_handle, "sidecar", &sidecar_ref, &message);
                break;
            }
        }
    }

    if let Some(stdout) = stdout {
        let _ = stdout.join();
    }
    if let Some(stderr) = stderr {
        let _ = stderr.join();
    }
    rpc.set_stdin(None);
    rpc.reject_pending("Sidecar process stopped before answering.");
}

fn spawn_stdout_reader<R>(
    sidecar_ref: String,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    stream: R,
    app_handle: AppHandle,
    rpc: SidecarRpc,
    runtime_state: Arc<Mutex<SidecarRuntimeState>>,
) -> thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(message) if is_jsonrpc_message(&message) => {
                    if !rpc.resolve_response(&message) {
                        handle_sidecar_jsonrpc(
                            &sidecar_ref,
                            &stdin,
                            &app_handle,
                            runtime_state.clone(),
                            message,
                        );
                    }
                }
                _ => {
                    warn!(
                        "sidecar '{}' stdout emitted non-RPC output: {}",
                        sidecar_ref, line
                    );
                    emit_runtime_log(&app_handle, "sidecar", &sidecar_ref, "stdout", &line);
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

fn required_string(params: &serde_json::Value, field: &str) -> Result<String, String> {
    params
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("Sidecar JSON-RPC request parameter '{field}' is required."))
}

fn sidecar_package_id(sidecar_ref: &str) -> &str {
    sidecar_ref.split('/').next().unwrap_or_default()
}

fn set_sidecar_stopped(
    runtime_state: &Arc<Mutex<SidecarRuntimeState>>,
    sidecar_ref: &str,
    exit_code: Option<i32>,
) {
    let mut state = runtime_state.lock().unwrap();
    let status = state
        .status_by_ref
        .entry(sidecar_ref.to_string())
        .or_default();
    status.running = false;
    status.last_exit_code = exit_code;
}

fn set_sidecar_crash(
    runtime_state: &Arc<Mutex<SidecarRuntimeState>>,
    sidecar_ref: &str,
    exit_code: Option<i32>,
) {
    let mut state = runtime_state.lock().unwrap();
    let status = state
        .status_by_ref
        .entry(sidecar_ref.to_string())
        .or_default();
    status.running = false;
    status.last_exit_code = exit_code;
    status.crash_count = status.crash_count.saturating_add(1);
}

fn state_get(
    runtime_state: &Arc<Mutex<SidecarRuntimeState>>,
    sidecar_ref: &str,
    key: &str,
) -> Option<serde_json::Value> {
    let package_id = sidecar_package_id(sidecar_ref);
    runtime_state
        .lock()
        .unwrap()
        .package_state
        .get(package_id)
        .and_then(|state| state.get(key).cloned())
}

fn state_set(
    runtime_state: &Arc<Mutex<SidecarRuntimeState>>,
    sidecar_ref: &str,
    key: String,
    value: serde_json::Value,
) {
    let package_id = sidecar_package_id(sidecar_ref);
    runtime_state
        .lock()
        .unwrap()
        .package_state
        .entry(package_id.to_string())
        .or_default()
        .insert(key, value);
}

fn get_status_snapshot(
    runtime_state: &Arc<Mutex<SidecarRuntimeState>>,
    sidecar_ref: &str,
) -> SidecarRuntimeStatus {
    runtime_state
        .lock()
        .unwrap()
        .status_by_ref
        .get(sidecar_ref)
        .cloned()
        .unwrap_or_default()
}

fn handle_sidecar_jsonrpc(
    sidecar_ref: &str,
    stdin: &Option<Arc<Mutex<ChildStdin>>>,
    app_handle: &AppHandle,
    runtime_state: Arc<Mutex<SidecarRuntimeState>>,
    message: serde_json::Value,
) {
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let id = message.get("id").cloned();
    let Some(method) = method else {
        return;
    };
    let params = message
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let result = match method.as_str() {
        "diagnostics/log" => {
            let severity = params
                .get("severity")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("info");
            let message = params
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            emit_runtime_log(app_handle, "sidecar", sidecar_ref, severity, message);
            Ok(serde_json::json!({ "ok": true }))
        }
        "telemetry/event" => {
            emit_runtime_log(
                app_handle,
                "sidecar",
                sidecar_ref,
                "telemetry",
                &params.to_string(),
            );
            Ok(serde_json::json!({ "ok": true }))
        }
        "state/get" => required_string(&params, "key").map(|key| {
            let value =
                state_get(&runtime_state, sidecar_ref, &key).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "value": value })
        }),
        "state/set" => required_string(&params, "key").map(|key| {
            let value = params
                .get("value")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            state_set(&runtime_state, sidecar_ref, key, value);
            serde_json::json!({ "ok": true })
        }),
        "runtime/status" => {
            let status = get_status_snapshot(&runtime_state, sidecar_ref);
            Ok(serde_json::json!(status))
        }
        _ => Err(format!(
            "Sidecar JSON-RPC method '{method}' is not supported."
        )),
    };
    if let (Some(id), Some(stdin)) = (id, stdin.as_ref()) {
        send_jsonrpc_response(stdin, id, result);
    }
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
    sidecar_ref: String,
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
            if stream_name == "stderr" {
                warn!("sidecar '{}' stderr: {}", sidecar_ref, line);
            } else {
                info!("sidecar '{}' stdout: {}", sidecar_ref, line);
            }
            emit_runtime_log(&app_handle, "sidecar", &sidecar_ref, stream_name, &line);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_status_starts_empty() {
        let manager = SidecarRuntimeManager::default();
        assert_eq!(manager.status("pkg", "helper"), None);
        let snapshot = manager.status_map();
        assert!(snapshot.is_empty());
    }

    #[test]
    fn runtime_state_set_and_get_are_package_local() {
        let manager = SidecarRuntimeManager::default();
        manager.state_set("com.pkg.a", "ping".to_string(), serde_json::json!("ok"));
        assert_eq!(
            manager.state_get("com.pkg.a", "ping"),
            Some(serde_json::json!("ok"))
        );
        assert_eq!(manager.state_get("com.pkg.b", "ping"), None);
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeErrorPayload<'a> {
    kind: &'a str,
    source: &'a str,
    message: &'a str,
}

fn emit_runtime_error(app_handle: &AppHandle, kind: &str, source: &str, message: &str) {
    let payload = RuntimeErrorPayload {
        kind,
        source,
        message,
    };
    let _ = app_handle.emit("bakingrl-runtime-error", payload);
}
