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

use crate::plugin_package::manifest::PluginRuntimeSidecarActivationV3;

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
    pub activation: PluginRuntimeSidecarActivationV3,
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
}

impl SidecarRuntimeController {
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
            match spawn_sidecar_runtime(spec, app_handle.clone()) {
                Ok(handle) => {
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
            handle.shutdown();
        }
        match spawn_sidecar_runtime(spec, app_handle.clone()) {
            Ok(handle) => {
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
            handle.shutdown();
            spec
        };
        self.start_with_app_handle(spec, app_handle)?;
        Ok(true)
    }

    pub fn stop(&self, package_id: &str, sidecar_name: &str) -> bool {
        let sidecar_ref = format!("{package_id}/{sidecar_name}");
        let mut handles = self.handles.lock().unwrap();
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
            handle.shutdown();
        }
    }
}

fn spawn_sidecar_runtime(
    spec: SidecarRuntimeSpec,
    app_handle: AppHandle,
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
        .spawn(move || supervise_child(sidecar_ref, child, shutdown_rx, app_handle, supervisor_rpc))
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
            info!("Sidecar runtime '{}' stopped.", sidecar_ref);
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    info!("Sidecar runtime '{}' exited with {}.", sidecar_ref, status);
                } else {
                    let message = format!("Sidecar runtime exited with {status}.");
                    warn!("Sidecar '{}': {}", sidecar_ref, message);
                    emit_runtime_error(&app_handle, "sidecar", &sidecar_ref, &message);
                }
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(err) => {
                let message = format!("Unable to inspect sidecar process: {err}");
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
                        handle_sidecar_jsonrpc(&sidecar_ref, &stdin, &app_handle, message);
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

fn handle_sidecar_jsonrpc(
    sidecar_ref: &str,
    stdin: &Option<Arc<Mutex<ChildStdin>>>,
    app_handle: &AppHandle,
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
