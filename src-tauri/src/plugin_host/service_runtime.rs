use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deno_core::{op2, JsRuntime, OpState, RuntimeOptions};
use deno_error::JsErrorBox;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use super::runtime_module_loader::PackageModuleLoader;
use crate::bus::{BusEvent, EventBus};
use crate::models::GameEvent;
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;
use crate::registry::Registry;

const RUNTIME_COMMAND_QUEUE: usize = 128;
const RUNTIME_EVENT_QUEUE: usize = 256;
const RUNTIME_CALL_QUEUE: usize = 128;

#[derive(Debug, Clone)]
pub struct ServiceRuntimeModuleSpec {
    pub service_name: String,
    pub entry_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServiceRuntimeSpec {
    pub package_id: String,
    pub service_name: String,
    pub entry_path: PathBuf,
    pub storage_root: PathBuf,
    pub service_imports: Vec<String>,
    pub service_methods: HashMap<String, Vec<String>>,
    pub permissions: EffectivePackagePermissionsV2,
    pub additional_modules: Vec<ServiceRuntimeModuleSpec>,
}

impl ServiceRuntimeSpec {
    fn service_ref(&self) -> String {
        format!("{}/{}", self.package_id, self.service_name)
    }

    fn runtime_key(&self) -> String {
        if self.additional_modules.is_empty() {
            self.service_ref()
        } else {
            format!("{}/__package__", self.package_id)
        }
    }

    fn module_specs(&self) -> Vec<ServiceRuntimeModuleSpec> {
        let mut modules = vec![ServiceRuntimeModuleSpec {
            service_name: self.service_name.clone(),
            entry_path: self.entry_path.clone(),
        }];
        modules.extend(self.additional_modules.clone());
        modules
    }

    fn service_refs(&self) -> Vec<String> {
        self.module_specs()
            .into_iter()
            .map(|module| format!("{}/{}", self.package_id, module.service_name))
            .collect()
    }
}

enum ServiceRuntimeCommand {
    Call {
        service_ref: String,
        method: String,
        input: serde_json::Value,
        response: oneshot::Sender<Result<serde_json::Value, String>>,
    },
}

struct ServiceRuntimeHandle {
    client: ServiceRuntimeClient,
    service_refs: Vec<String>,
    fingerprint: String,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ServiceRuntimeHandle {
    fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let start = std::time::Instant::now();
            while !thread.is_finished() && start.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            } else {
                warn!("Service runtime did not stop within timeout; leaving thread detached.");
            }
        }
    }
}

#[derive(Default)]
pub struct ServiceRuntimeManager {
    handles: Mutex<HashMap<String, ServiceRuntimeHandle>>,
    router: ServiceCallRouter,
}

impl ServiceRuntimeManager {
    pub(crate) fn router(&self) -> ServiceCallRouter {
        self.router.clone()
    }

    #[cfg(test)]
    pub fn reload(
        &self,
        specs: Vec<ServiceRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
    ) {
        self.reload_with_optional_app_handle(specs, bus, registry, None);
    }

    pub fn reload_with_app_handle(
        &self,
        specs: Vec<ServiceRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        app_handle: AppHandle,
    ) {
        self.reload_with_optional_app_handle(specs, bus, registry, Some(app_handle));
    }

    fn reload_with_optional_app_handle(
        &self,
        specs: Vec<ServiceRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        app_handle: Option<AppHandle>,
    ) {
        let mut handles = self.handles.lock().unwrap();
        let desired: HashSet<String> = specs.iter().map(ServiceRuntimeSpec::runtime_key).collect();

        let stale: Vec<_> = handles
            .keys()
            .filter(|runtime_key| !desired.contains(*runtime_key))
            .cloned()
            .collect();
        for runtime_key in stale {
            if let Some(handle) = handles.remove(&runtime_key) {
                for service_ref in &handle.service_refs {
                    self.router.remove(service_ref);
                }
                handle.shutdown();
            }
        }

        for spec in specs {
            let runtime_key = spec.runtime_key();
            let fingerprint = format!("{spec:?}");
            if handles
                .get(&runtime_key)
                .is_some_and(|handle| handle.fingerprint == fingerprint)
            {
                continue;
            }
            if let Some(handle) = handles.remove(&runtime_key) {
                for service_ref in &handle.service_refs {
                    self.router.remove(service_ref);
                }
                handle.shutdown();
            }
            match spawn_service_runtime(
                spec,
                bus.clone(),
                registry.clone(),
                self.router.clone(),
                app_handle.clone(),
            ) {
                Ok(handle) => {
                    for service_ref in &handle.service_refs {
                        self.router.insert(
                            service_ref.clone(),
                            ServiceRuntimeClient {
                                service_ref: service_ref.clone(),
                                tx: handle.client.tx.clone(),
                            },
                        );
                    }
                    handles.insert(runtime_key, handle);
                }
                Err(err) => {
                    warn!("Unable to start service runtime: {}", err);
                    if let Some(app_handle) = app_handle.as_ref() {
                        emit_runtime_error(app_handle, "service", &runtime_key, &err);
                    }
                }
            }
        }
    }

    pub async fn call(
        &self,
        service_ref: &str,
        method: String,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.router.call(service_ref, method, input).await
    }
}

impl Drop for ServiceRuntimeManager {
    fn drop(&mut self) {
        let mut handles = self.handles.lock().unwrap();
        for (_, handle) in handles.drain() {
            handle.shutdown();
        }
    }
}

#[derive(Clone)]
struct ServiceRuntimeContext {
    package_id: String,
    service_ref: String,
    storage_root: PathBuf,
    service_imports: Vec<String>,
    service_methods: HashMap<String, Vec<String>>,
    permissions: EffectivePackagePermissionsV2,
}

#[derive(Clone)]
pub(crate) struct ServiceRuntimeClient {
    service_ref: String,
    tx: mpsc::Sender<ServiceRuntimeCommand>,
}

impl ServiceRuntimeClient {
    fn call(
        &self,
        method: String,
        input: serde_json::Value,
        response: oneshot::Sender<Result<serde_json::Value, String>>,
    ) -> Result<(), String> {
        self.tx
            .try_send(ServiceRuntimeCommand::Call {
                service_ref: self.service_ref.clone(),
                method,
                input,
                response,
            })
            .map_err(|_| {
                format!(
                    "Service runtime '{}' is overloaded or stopped.",
                    self.service_ref
                )
            })
    }
}

#[derive(Clone, Default)]
pub(crate) struct ServiceCallRouter {
    clients: Arc<Mutex<HashMap<String, ServiceRuntimeClient>>>,
}

impl ServiceCallRouter {
    fn insert(&self, service_ref: String, client: ServiceRuntimeClient) {
        self.clients.lock().unwrap().insert(service_ref, client);
    }

    fn remove(&self, service_ref: &str) {
        self.clients.lock().unwrap().remove(service_ref);
    }

    pub(crate) async fn call(
        &self,
        service_ref: &str,
        method: String,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let client = self
            .clients
            .lock()
            .unwrap()
            .get(service_ref)
            .cloned()
            .ok_or_else(|| format!("Service runtime '{service_ref}' is not running."))?;
        let (response_tx, response_rx) = oneshot::channel();
        client.call(method, input, response_tx)?;
        tokio::time::timeout(Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| format!("Service runtime '{service_ref}' timed out."))?
            .map_err(|_| format!("Service runtime '{service_ref}' did not answer."))?
    }
}

struct PendingCall {
    id: u32,
    service_ref: String,
    method: String,
    input: serde_json::Value,
    response: oneshot::Sender<Result<serde_json::Value, String>>,
}

type EventReceiver = Rc<tokio::sync::Mutex<mpsc::Receiver<serde_json::Value>>>;
type CallReceiver = Rc<tokio::sync::Mutex<mpsc::Receiver<PendingCall>>>;
type PendingResponses =
    Rc<RefCell<HashMap<u32, oneshot::Sender<Result<serde_json::Value, String>>>>>;
type Subscriptions = Arc<Mutex<HashSet<String>>>;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceBootstrapModule {
    service_ref: String,
    entry_url: String,
    methods: Vec<String>,
}

fn spawn_service_runtime(
    spec: ServiceRuntimeSpec,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    router: ServiceCallRouter,
    app_handle: Option<AppHandle>,
) -> Result<ServiceRuntimeHandle, String> {
    let (command_tx, command_rx) = mpsc::channel::<ServiceRuntimeCommand>(RUNTIME_COMMAND_QUEUE);
    let runtime_key = spec.runtime_key();
    let service_ref = spec.service_ref();
    let service_refs = spec.service_refs();
    let fingerprint = format!("{spec:?}");
    let client = ServiceRuntimeClient {
        service_ref: service_ref.clone(),
        tx: command_tx,
    };
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let error_service_ref = service_ref.clone();

    let thread = std::thread::Builder::new()
        .name(format!("bakingrl-service-{runtime_key}"))
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create service runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async move {
                if let Err(err) =
                    run_service_runtime(spec, bus, registry, router, command_rx, shutdown_rx).await
                {
                    error!("Service runtime failed: {}", err);
                    if let Some(app_handle) = app_handle.as_ref() {
                        emit_runtime_error(app_handle, "service", &error_service_ref, &err);
                    }
                }
            });
        })
        .map_err(|e| format!("Unable to spawn service runtime thread: {e}"))?;

    Ok(ServiceRuntimeHandle {
        client,
        service_refs,
        fingerprint,
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    })
}

fn emit_runtime_error(app_handle: &AppHandle, kind: &str, source: &str, message: &str) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let payload = serde_json::json!({
        "kind": kind,
        "source": source,
        "message": message,
        "timestamp_ms": timestamp_ms
    });
    let _ = app_handle.emit("bakingrl-runtime-error", payload);
}

async fn run_service_runtime(
    spec: ServiceRuntimeSpec,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    router: ServiceCallRouter,
    mut command_rx: mpsc::Receiver<ServiceRuntimeCommand>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let service_ref = spec.service_ref();
    let (event_tx, event_rx) = mpsc::channel::<serde_json::Value>(RUNTIME_EVENT_QUEUE);
    let (call_tx, call_rx) = mpsc::channel::<PendingCall>(RUNTIME_CALL_QUEUE);
    let event_rx = Rc::new(tokio::sync::Mutex::new(event_rx));
    let call_rx = Rc::new(tokio::sync::Mutex::new(call_rx));
    let pending_responses: PendingResponses = Rc::new(RefCell::new(HashMap::new()));
    let subscriptions: Subscriptions = Arc::new(Mutex::new(HashSet::new()));
    let context = ServiceRuntimeContext {
        package_id: spec.package_id.clone(),
        service_ref: service_ref.clone(),
        storage_root: spec.storage_root.clone(),
        service_imports: spec.service_imports.clone(),
        service_methods: spec.service_methods.clone(),
        permissions: spec.permissions.clone(),
    };

    forward_bus_events(
        context.clone(),
        bus.clone(),
        event_tx,
        subscriptions.clone(),
        shutdown_rx,
    );
    forward_service_commands(&service_ref, &mut command_rx, call_tx);

    let module_specs = spec.module_specs();
    let service_methods = spec.service_methods.clone();
    let modules = module_specs
        .iter()
        .map(|module| {
            let service_ref = format!("{}/{}", spec.package_id, module.service_name);
            let entry_url = url::Url::from_file_path(&module.entry_path).map_err(|_| {
                format!(
                    "Unable to convert service entry to file URL: {:?}",
                    module.entry_path
                )
            })?;
            Ok(ServiceBootstrapModule {
                methods: service_methods
                    .get(&service_ref)
                    .cloned()
                    .unwrap_or_default(),
                service_ref,
                entry_url: entry_url.to_string(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bootstrap = service_bootstrap(&modules)?;
    let bootstrap_url = deno_core::resolve_path(
        &format!("./bakingrl-service-{}.js", spec.service_name),
        &std::env::current_dir().map_err(|e| format!("Unable to resolve cwd: {e}"))?,
    )
    .map_err(|e| format!("Unable to resolve service bootstrap module: {e}"))?;
    let module_loader = PackageModuleLoader::from_entry_paths(
        module_specs.iter().map(|module| module.entry_path.clone()),
        &spec.storage_root,
    )
    .with_virtual_module(&bootstrap_url, bootstrap.clone());
    let mut js_runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(module_loader)),
        extensions: vec![bakingrl_service::init()],
        ..Default::default()
    });

    {
        let op_state = js_runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.put::<Arc<EventBus>>(bus);
        state.put::<Arc<Registry>>(registry);
        state.put::<ServiceCallRouter>(router);
        state.put::<ServiceRuntimeContext>(context);
        state.put::<EventReceiver>(event_rx);
        state.put::<CallReceiver>(call_rx);
        state.put::<PendingResponses>(pending_responses);
        state.put::<Subscriptions>(subscriptions);
    }

    let mod_id = js_runtime
        .load_main_es_module_from_code(&bootstrap_url, bootstrap)
        .await
        .map_err(|e| format!("Unable to load service module '{service_ref}': {e}"))?;
    let result = js_runtime.mod_evaluate(mod_id);
    js_runtime
        .run_event_loop(Default::default())
        .await
        .map_err(|e| format!("Service event loop failed for '{service_ref}': {e}"))?;
    result
        .await
        .map_err(|e| format!("Service module evaluation failed for '{service_ref}': {e}"))?;
    info!("Service runtime '{service_ref}' exited.");
    Ok(())
}

fn forward_service_commands(
    service_ref: &str,
    command_rx: &mut mpsc::Receiver<ServiceRuntimeCommand>,
    call_tx: mpsc::Sender<PendingCall>,
) {
    let service_ref = service_ref.to_string();
    let (_replacement_tx, replacement_rx) = mpsc::channel(1);
    let mut command_rx = std::mem::replace(command_rx, replacement_rx);
    tokio::task::spawn_local(async move {
        let mut next_id = 1u32;
        while let Some(command) = command_rx.recv().await {
            match command {
                ServiceRuntimeCommand::Call {
                    service_ref: target_service_ref,
                    method,
                    input,
                    response,
                } => {
                    let id = next_id;
                    next_id += 1;
                    if call_tx
                        .send(PendingCall {
                            id,
                            service_ref: target_service_ref,
                            method,
                            input,
                            response,
                        })
                        .await
                        .is_err()
                    {
                        warn!("Service runtime '{}' dropped a call request.", service_ref);
                    }
                }
            }
        }
    });
}

fn forward_bus_events(
    context: ServiceRuntimeContext,
    bus: Arc<EventBus>,
    event_tx: mpsc::Sender<serde_json::Value>,
    subscriptions: Subscriptions,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    tokio::task::spawn_local(async move {
        let mut bus_rx = bus.subscribe();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                event = bus_rx.recv() => {
                    let Ok(event) = event else {
                        break;
                    };
                    let name = event.name().to_string();
                    if !context.permissions.can_read_bus(&name) {
                        continue;
                    }
                    let subscribed = subscriptions
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|pattern| pattern == "*" || pattern == &name || pattern.strip_suffix(".*").is_some_and(|prefix| name.starts_with(prefix)));
                    if !subscribed {
                        continue;
                    }
                    let value = match event {
                        BusEvent::GameData(event) => serde_json::to_value(&*event).unwrap_or(serde_json::Value::Null),
                        BusEvent::RawJson(raw) => serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null),
                    };
                    if event_tx.send(value).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn service_bootstrap(modules: &[ServiceBootstrapModule]) -> Result<String, String> {
    let modules_json = serde_json::to_string(modules)
        .map_err(|e| format!("Unable to serialize service bootstrap modules: {e}"))?;
    Ok(format!(
        r#"
const serviceEntries = {modules_json};
const services = new Map();
const allowedMethods = new Map();
for (const entry of serviceEntries) {{
  const serviceModule = await import(entry.entryUrl);
  const service = serviceModule.default ?? serviceModule;
  if (!service || typeof service !== "object") {{
    throw new Error(`Service module '${{entry.serviceRef}}' must export an object.`);
  }}
  services.set(entry.serviceRef, service);
  allowedMethods.set(entry.serviceRef, new Set(entry.methods ?? []));
}}
const listeners = new Map();
const callStack = [];
async function invokeService(serviceRef, methodName, input) {{
  if (callStack[callStack.length - 1] === serviceRef) {{
    throw new Error("A service cannot synchronously call itself.");
  }}
  const service = services.get(serviceRef);
  if (!service) throw new Error(`Unknown service '${{serviceRef}}'.`);
  if (!allowedMethods.get(serviceRef)?.has(methodName)) {{
    throw new Error(`Service '${{serviceRef}}' does not expose method '${{methodName}}'.`);
  }}
  const method = service.methods?.[methodName];
  if (typeof method !== "function") throw new Error(`Unknown service method '${{methodName}}'.`);
  callStack.push(serviceRef);
  try {{
    return await method(input, context);
  }} finally {{
    callStack.pop();
  }}
}}
const context = {{
  bus: {{
    subscribe(eventName, callback) {{
      globalThis.Deno.core.ops.op_service_subscribe(eventName);
      const callbacks = listeners.get(eventName) ?? new Set();
      callbacks.add(callback);
      listeners.set(eventName, callbacks);
      return () => callbacks.delete(callback);
    }},
    emit(eventName, payload) {{
      globalThis.Deno.core.ops.op_service_bus_emit(eventName, payload ?? null);
    }}
  }},
  registry: {{
    get(key) {{
      return globalThis.Deno.core.ops.op_service_registry_get(key);
    }},
    set(key, value) {{
      globalThis.Deno.core.ops.op_service_registry_set(key, value ?? null);
    }}
  }},
  storage: {{
    readText(uri) {{
      return globalThis.Deno.core.ops.op_service_storage_read(uri);
    }},
    writeText(uri, contents) {{
      globalThis.Deno.core.ops.op_service_storage_write(uri, String(contents ?? ""));
    }}
  }},
  services: {{
    async call(ref, method, input) {{
      const serviceRef = String(ref);
      const methodName = String(method);
      if (services.has(serviceRef)) {{
        return await invokeService(serviceRef, methodName, input ?? null);
      }}
      return await globalThis.Deno.core.ops.op_service_call(serviceRef, methodName, input ?? null);
    }}
  }},
  settings: {{
    get() {{ return undefined; }},
    all() {{ return {{}}; }}
  }},
  diagnostics: console
}};
for (const service of services.values()) {{
  await service.mount?.(context);
}}
async function dispatchEvents() {{
  while (true) {{
    let event;
    try {{
      event = await globalThis.Deno.core.ops.op_service_next_event();
    }} catch (error) {{
      if (String(error).includes("Service event channel closed")) return;
      throw error;
    }}
    const eventName = event.Event ?? event.event;
    for (const [pattern, callbacks] of listeners) {{
      if (pattern === "*" || pattern === eventName || (pattern.endsWith(".*") && eventName.startsWith(pattern.slice(0, -1)))) {{
        for (const callback of callbacks) await callback(event);
      }}
    }}
  }}
}}
async function dispatchCalls() {{
  while (true) {{
    let call;
    try {{
      call = await globalThis.Deno.core.ops.op_service_next_call();
    }} catch (error) {{
      if (String(error).includes("Service call channel closed")) return;
      throw error;
    }}
    try {{
      const output = await invokeService(call.serviceRef, call.method, call.input);
      globalThis.Deno.core.ops.op_service_complete_call(call.id, true, output ?? null);
    }} catch (error) {{
      globalThis.Deno.core.ops.op_service_complete_call(call.id, false, error instanceof Error ? error.message : String(error));
    }}
  }}
}}
await Promise.race([dispatchEvents(), dispatchCalls()]);
"#
    ))
}

#[op2(fast)]
pub fn op_service_subscribe(
    state: &mut OpState,
    #[string] event_name: String,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context.permissions.can_read_bus(&event_name) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot subscribe to '{}'.",
            context.package_id, event_name
        )));
    }
    state
        .borrow::<Subscriptions>()
        .lock()
        .unwrap()
        .insert(event_name);
    Ok(())
}

#[op2]
#[serde]
pub async fn op_service_next_event(
    state: Rc<RefCell<OpState>>,
) -> Result<serde_json::Value, JsErrorBox> {
    let rx = {
        let state = state.borrow();
        state.borrow::<EventReceiver>().clone()
    };
    let mut rx = rx.lock().await;
    rx.recv()
        .await
        .ok_or_else(|| JsErrorBox::generic("Service event channel closed"))
}

#[op2]
#[serde]
pub async fn op_service_next_call(
    state: Rc<RefCell<OpState>>,
) -> Result<serde_json::Value, JsErrorBox> {
    let rx = {
        let state = state.borrow();
        state.borrow::<CallReceiver>().clone()
    };
    let mut rx = rx.lock().await;
    let pending = rx
        .recv()
        .await
        .ok_or_else(|| JsErrorBox::generic("Service call channel closed"))?;
    {
        let state = state.borrow();
        state
            .borrow::<PendingResponses>()
            .borrow_mut()
            .insert(pending.id, pending.response);
    }
    Ok(serde_json::json!({
        "id": pending.id,
        "serviceRef": pending.service_ref,
        "method": pending.method,
        "input": pending.input
    }))
}

#[op2]
pub fn op_service_complete_call(
    state: &mut OpState,
    id: u32,
    success: bool,
    #[serde] value: serde_json::Value,
) -> Result<(), JsErrorBox> {
    let Some(response) = state.borrow::<PendingResponses>().borrow_mut().remove(&id) else {
        return Err(JsErrorBox::generic(format!(
            "Unknown service call id '{id}'"
        )));
    };
    let result = if success {
        Ok(value)
    } else {
        Err(value.as_str().unwrap_or("Service call failed").to_string())
    };
    let _ = response.send(result);
    Ok(())
}

#[op2]
pub fn op_service_bus_emit(
    state: &mut OpState,
    #[string] event_name: String,
    #[serde] payload: serde_json::Value,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context.permissions.can_publish_bus(&event_name) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot publish '{}'.",
            context.package_id, event_name
        )));
    }
    let bus = state.borrow::<Arc<EventBus>>().clone();
    bus.publish(BusEvent::GameData(Arc::new(GameEvent {
        event: event_name,
        data: payload,
    })));
    Ok(())
}

#[op2]
#[serde]
pub async fn op_service_call(
    state: Rc<RefCell<OpState>>,
    #[string] service_ref: String,
    #[string] method: String,
    #[serde] input: serde_json::Value,
) -> Result<serde_json::Value, JsErrorBox> {
    let (context, router) = {
        let state = state.borrow();
        (
            state.borrow::<ServiceRuntimeContext>().clone(),
            state.borrow::<ServiceCallRouter>().clone(),
        )
    };
    if service_ref == context.service_ref {
        return Err(JsErrorBox::generic(
            "A service cannot synchronously call itself.",
        ));
    }
    let Some((target_package_id, _)) = service_ref.split_once('/') else {
        return Err(JsErrorBox::generic(format!(
            "Service ref '{}' must use '<package-id>/<service>'.",
            service_ref
        )));
    };
    if target_package_id != context.package_id
        && !context
            .service_imports
            .iter()
            .any(|declared| declared == &service_ref)
    {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' did not declare service import '{}'.",
            context.package_id, service_ref
        )));
    }
    let allowed_methods = context
        .service_methods
        .get(&service_ref)
        .ok_or_else(|| JsErrorBox::generic(format!("Service '{}' is not known.", service_ref)))?;
    if !allowed_methods.iter().any(|allowed| allowed == &method) {
        return Err(JsErrorBox::generic(format!(
            "Service '{}' does not expose method '{}'.",
            service_ref, method
        )));
    }
    router
        .call(&service_ref, method, input)
        .await
        .map_err(JsErrorBox::generic)
}

#[op2]
#[serde]
pub fn op_service_registry_get(
    state: &mut OpState,
    #[string] key: String,
) -> Result<serde_json::Value, JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context.permissions.can_read_registry(&key) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot read registry key '{}'.",
            context.package_id, key
        )));
    }
    let registry = state.borrow::<Arc<Registry>>().clone();
    Ok(registry.get(&key).unwrap_or(serde_json::Value::Null))
}

#[op2]
pub fn op_service_registry_set(
    state: &mut OpState,
    #[string] key: String,
    #[serde] value: serde_json::Value,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context.permissions.can_write_registry(&key) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot write registry key '{}'.",
            context.package_id, key
        )));
    }
    let registry = state.borrow::<Arc<Registry>>().clone();
    registry.set(key, value);
    Ok(())
}

#[op2]
#[string]
pub fn op_service_storage_read(
    state: &mut OpState,
    #[string] uri: String,
) -> Result<String, JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context
        .permissions
        .storage
        .read
        .iter()
        .any(|pattern| pattern == "plugin://self/*")
    {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot read storage URI '{}'.",
            context.package_id, uri
        )));
    }
    let path = resolve_storage_uri(&context.storage_root, &uri)?;
    std::fs::read_to_string(path).map_err(JsErrorBox::from_err)
}

#[op2(fast)]
pub fn op_service_storage_write(
    state: &mut OpState,
    #[string] uri: String,
    #[string] contents: String,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ServiceRuntimeContext>().clone();
    if !context
        .permissions
        .storage
        .write
        .iter()
        .any(|pattern| pattern == "plugin://self/*")
    {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot write storage URI '{}'.",
            context.package_id, uri
        )));
    }
    let path = resolve_storage_uri(&context.storage_root, &uri)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(JsErrorBox::from_err)?;
    }
    std::fs::write(path, contents).map_err(JsErrorBox::from_err)
}

fn resolve_storage_uri(storage_root: &Path, uri: &str) -> Result<PathBuf, JsErrorBox> {
    let Some(relative) = uri.strip_prefix("plugin://self/") else {
        return Err(JsErrorBox::generic(format!(
            "Storage URI '{}' must start with plugin://self/.",
            uri
        )));
    };
    if relative.trim().is_empty() {
        return Err(JsErrorBox::generic("Storage URI path cannot be empty."));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(JsErrorBox::generic(format!(
            "Storage URI '{}' escapes plugin storage.",
            uri
        )));
    }
    Ok(storage_root.join(relative_path))
}

deno_core::extension!(
    bakingrl_service,
    ops = [
        op_service_subscribe,
        op_service_next_event,
        op_service_next_call,
        op_service_complete_call,
        op_service_call,
        op_service_bus_emit,
        op_service_registry_get,
        op_service_registry_set,
        op_service_storage_read,
        op_service_storage_write,
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_v2::permissions::{
        EffectiveBusPermissionsV2, EffectiveRegistryPermissionsV2, EffectiveStoragePermissionsV2,
    };

    #[tokio::test]
    async fn calls_service_method() {
        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("service.js");
        std::fs::write(
            &entry_path,
            r#"
export default {
  methods: {
    echo(input) {
      return { ok: true, input };
    }
  }
};
"#,
        )
        .unwrap();

        let manager = ServiceRuntimeManager::default();
        manager.reload(
            vec![ServiceRuntimeSpec {
                package_id: "com.example.service".to_string(),
                service_name: "echo".to_string(),
                entry_path,
                storage_root: dir.path().join("storage"),
                service_imports: vec![],
                service_methods: HashMap::from([(
                    "com.example.service/echo".to_string(),
                    vec!["echo".to_string()],
                )]),
                permissions: EffectivePackagePermissionsV2 {
                    bus: EffectiveBusPermissionsV2 {
                        read: vec![],
                        publish: vec![],
                    },
                    registry: EffectiveRegistryPermissionsV2 {
                        read: vec![],
                        write: vec![],
                    },
                    network: Default::default(),
                    storage: EffectiveStoragePermissionsV2 {
                        read: vec![],
                        write: vec![],
                    },
                },
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            Arc::new(Registry::new()),
        );

        let output = manager
            .call(
                "com.example.service/echo",
                "echo".to_string(),
                serde_json::json!({ "value": 42 }),
            )
            .await
            .unwrap();
        assert_eq!(output["ok"], true);
        assert_eq!(output["input"]["value"], 42);
    }

    #[tokio::test]
    async fn service_storage_is_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("service.js");
        let storage_root = dir.path().join("storage");
        std::fs::write(
            &entry_path,
            r#"
export default {
  methods: {
    async writeAndRead(input, context) {
      await context.storage.writeText("plugin://self/data/value.txt", String(input.value));
      return await context.storage.readText("plugin://self/data/value.txt");
    },
    async escape(_input, context) {
      await context.storage.writeText("plugin://self/../outside.txt", "bad");
    }
  }
};
"#,
        )
        .unwrap();

        let manager = ServiceRuntimeManager::default();
        manager.reload(
            vec![ServiceRuntimeSpec {
                package_id: "com.example.storage".to_string(),
                service_name: "store".to_string(),
                entry_path,
                storage_root: storage_root.clone(),
                service_imports: vec![],
                service_methods: HashMap::from([(
                    "com.example.storage/store".to_string(),
                    vec!["writeAndRead".to_string(), "escape".to_string()],
                )]),
                permissions: EffectivePackagePermissionsV2 {
                    bus: EffectiveBusPermissionsV2 {
                        read: vec![],
                        publish: vec![],
                    },
                    registry: EffectiveRegistryPermissionsV2 {
                        read: vec![],
                        write: vec![],
                    },
                    network: Default::default(),
                    storage: EffectiveStoragePermissionsV2 {
                        read: vec!["plugin://self/*".to_string()],
                        write: vec!["plugin://self/*".to_string()],
                    },
                },
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            Arc::new(Registry::new()),
        );

        let output = manager
            .call(
                "com.example.storage/store",
                "writeAndRead".to_string(),
                serde_json::json!({ "value": "hello" }),
            )
            .await
            .unwrap();
        assert_eq!(output, "hello");
        assert_eq!(
            std::fs::read_to_string(storage_root.join("data/value.txt")).unwrap(),
            "hello"
        );

        let err = manager
            .call(
                "com.example.storage/store",
                "escape".to_string(),
                serde_json::json!({}),
            )
            .await
            .unwrap_err();
        assert!(err.contains("escapes plugin storage"));
    }

    #[tokio::test]
    async fn service_can_call_declared_service_import() {
        let dir = tempfile::tempdir().unwrap();
        let provider_entry = dir.path().join("provider.js");
        let caller_entry = dir.path().join("caller.js");
        std::fs::write(
            &provider_entry,
            r#"
export default {
  methods: {
    double(input) {
      return input.value * 2;
    }
  }
};
"#,
        )
        .unwrap();
        std::fs::write(
            &caller_entry,
            r#"
export default {
  methods: {
    async calculate(input, context) {
      const doubled = await context.services.call("com.example.provider/math", "double", input);
      return doubled + 1;
    }
  }
};
"#,
        )
        .unwrap();

        let permissions = EffectivePackagePermissionsV2 {
            bus: EffectiveBusPermissionsV2 {
                read: vec![],
                publish: vec![],
            },
            registry: EffectiveRegistryPermissionsV2 {
                read: vec![],
                write: vec![],
            },
            network: Default::default(),
            storage: EffectiveStoragePermissionsV2 {
                read: vec![],
                write: vec![],
            },
        };
        let service_methods = HashMap::from([
            (
                "com.example.provider/math".to_string(),
                vec!["double".to_string()],
            ),
            (
                "com.example.caller/calc".to_string(),
                vec!["calculate".to_string()],
            ),
        ]);
        let manager = ServiceRuntimeManager::default();
        manager.reload(
            vec![
                ServiceRuntimeSpec {
                    package_id: "com.example.provider".to_string(),
                    service_name: "math".to_string(),
                    entry_path: provider_entry,
                    storage_root: dir.path().join("provider-storage"),
                    service_imports: vec![],
                    service_methods: service_methods.clone(),
                    permissions: permissions.clone(),
                    additional_modules: Vec::new(),
                },
                ServiceRuntimeSpec {
                    package_id: "com.example.caller".to_string(),
                    service_name: "calc".to_string(),
                    entry_path: caller_entry,
                    storage_root: dir.path().join("caller-storage"),
                    service_imports: vec!["com.example.provider/math".to_string()],
                    service_methods,
                    permissions,
                    additional_modules: Vec::new(),
                },
            ],
            Arc::new(EventBus::new(16)),
            Arc::new(Registry::new()),
        );

        let output = manager
            .call(
                "com.example.caller/calc",
                "calculate".to_string(),
                serde_json::json!({ "value": 20 }),
            )
            .await
            .unwrap();
        assert_eq!(output, 41);
    }

    #[tokio::test]
    async fn package_runtime_dispatches_multiple_services() {
        let dir = tempfile::tempdir().unwrap();
        let caller_entry = dir.path().join("caller.js");
        let helper_entry = dir.path().join("helper.js");
        std::fs::write(
            &caller_entry,
            r#"
export default {
  methods: {
    async calculate(input, context) {
      const doubled = await context.services.call("com.example.package/helper", "double", input);
      return doubled + 1;
    }
  }
};
"#,
        )
        .unwrap();
        std::fs::write(
            &helper_entry,
            r#"
export default {
  methods: {
    double(input) {
      return input.value * 2;
    }
  }
};
"#,
        )
        .unwrap();

        let permissions = EffectivePackagePermissionsV2 {
            bus: EffectiveBusPermissionsV2 {
                read: vec![],
                publish: vec![],
            },
            registry: EffectiveRegistryPermissionsV2 {
                read: vec![],
                write: vec![],
            },
            network: Default::default(),
            storage: EffectiveStoragePermissionsV2 {
                read: vec![],
                write: vec![],
            },
        };
        let service_methods = HashMap::from([
            (
                "com.example.package/calc".to_string(),
                vec!["calculate".to_string()],
            ),
            (
                "com.example.package/helper".to_string(),
                vec!["double".to_string()],
            ),
        ]);
        let manager = ServiceRuntimeManager::default();
        manager.reload(
            vec![ServiceRuntimeSpec {
                package_id: "com.example.package".to_string(),
                service_name: "calc".to_string(),
                entry_path: caller_entry,
                storage_root: dir.path().join("storage"),
                service_imports: vec![],
                service_methods,
                permissions,
                additional_modules: vec![ServiceRuntimeModuleSpec {
                    service_name: "helper".to_string(),
                    entry_path: helper_entry,
                }],
            }],
            Arc::new(EventBus::new(16)),
            Arc::new(Registry::new()),
        );

        let output = manager
            .call(
                "com.example.package/calc",
                "calculate".to_string(),
                serde_json::json!({ "value": 20 }),
            )
            .await
            .unwrap();
        assert_eq!(output, 41);

        let output = manager
            .call(
                "com.example.package/helper",
                "double".to_string(),
                serde_json::json!({ "value": 21 }),
            )
            .await
            .unwrap();
        assert_eq!(output, 42);
    }
}
