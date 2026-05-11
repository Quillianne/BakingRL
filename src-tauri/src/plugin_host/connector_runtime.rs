use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deno_core::{op2, JsRuntime, OpState, RuntimeOptions};
use deno_error::JsErrorBox;
use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
use tracing::{error, info, warn};

use super::runtime_module_loader::PackageModuleLoader;
use super::service_runtime::ServiceCallRouter;
use crate::bus::{BusEvent, EventBus};
use crate::models::GameEvent;
use crate::plugin_v2::permissions::EffectivePackagePermissionsV2;
use crate::registry::Registry;

const MAX_FETCH_BYTES: usize = 1024 * 1024;
const RUNTIME_EVENT_QUEUE: usize = 256;
const WEBSOCKET_WRITE_QUEUE: usize = 128;
const WEBSOCKET_READ_QUEUE: usize = 256;

#[derive(Debug, Clone)]
pub struct ConnectorRuntimeModuleSpec {
    pub connector_name: String,
    pub entry_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ConnectorRuntimeSpec {
    pub package_id: String,
    pub connector_name: String,
    pub entry_path: PathBuf,
    pub storage_root: PathBuf,
    pub service_imports: Vec<String>,
    pub service_methods: HashMap<String, Vec<String>>,
    pub permissions: EffectivePackagePermissionsV2,
    pub settings: serde_json::Value,
    pub additional_modules: Vec<ConnectorRuntimeModuleSpec>,
}

impl ConnectorRuntimeSpec {
    fn connector_ref(&self) -> String {
        format!("{}/{}", self.package_id, self.connector_name)
    }

    fn runtime_key(&self) -> String {
        if self.additional_modules.is_empty() {
            self.connector_ref()
        } else {
            format!("{}/__package_connectors__", self.package_id)
        }
    }

    fn module_specs(&self) -> Vec<ConnectorRuntimeModuleSpec> {
        let mut modules = vec![ConnectorRuntimeModuleSpec {
            connector_name: self.connector_name.clone(),
            entry_path: self.entry_path.clone(),
        }];
        modules.extend(self.additional_modules.clone());
        modules
    }
}

struct ConnectorRuntimeHandle {
    fingerprint: String,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ConnectorRuntimeHandle {
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
                warn!("Connector runtime did not stop within timeout; leaving thread detached.");
            }
        }
    }
}

#[derive(Default)]
pub struct ConnectorRuntimeManager {
    handles: Mutex<HashMap<String, ConnectorRuntimeHandle>>,
}

impl ConnectorRuntimeManager {
    #[cfg(test)]
    pub fn reload(
        &self,
        specs: Vec<ConnectorRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        service_router: ServiceCallRouter,
    ) {
        self.reload_with_optional_app_handle(specs, bus, registry, service_router, None);
    }

    pub fn reload_with_app_handle(
        &self,
        specs: Vec<ConnectorRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        service_router: ServiceCallRouter,
        app_handle: AppHandle,
    ) {
        self.reload_with_optional_app_handle(
            specs,
            bus,
            registry,
            service_router,
            Some(app_handle),
        );
    }

    fn reload_with_optional_app_handle(
        &self,
        specs: Vec<ConnectorRuntimeSpec>,
        bus: Arc<EventBus>,
        registry: Arc<Registry>,
        service_router: ServiceCallRouter,
        app_handle: Option<AppHandle>,
    ) {
        let mut handles = self.handles.lock().unwrap();
        let desired: HashSet<String> = specs
            .iter()
            .map(ConnectorRuntimeSpec::runtime_key)
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
                handle.shutdown();
            }
            match spawn_connector_runtime(
                spec,
                bus.clone(),
                registry.clone(),
                service_router.clone(),
                app_handle.clone(),
            ) {
                Ok(handle) => {
                    handles.insert(runtime_key, handle);
                }
                Err(err) => {
                    warn!("Unable to start connector runtime: {}", err);
                    if let Some(app_handle) = app_handle.as_ref() {
                        emit_runtime_error(app_handle, "connector", &runtime_key, &err);
                    }
                }
            }
        }
    }
}

impl Drop for ConnectorRuntimeManager {
    fn drop(&mut self) {
        let mut handles = self.handles.lock().unwrap();
        for (_, handle) in handles.drain() {
            handle.shutdown();
        }
    }
}

#[derive(Clone)]
struct ConnectorRuntimeContext {
    package_id: String,
    storage_root: PathBuf,
    service_imports: Vec<String>,
    service_methods: HashMap<String, Vec<String>>,
    permissions: EffectivePackagePermissionsV2,
}

type EventReceiver = Rc<tokio::sync::Mutex<mpsc::Receiver<serde_json::Value>>>;
type Subscriptions = Arc<Mutex<HashSet<String>>>;
type WebSocketMap = Arc<Mutex<HashMap<u32, WebSocketHandle>>>;

struct WebSocketHandle {
    tx: mpsc::Sender<TungsteniteMessage>,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<serde_json::Value>>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorBootstrapModule {
    connector_ref: String,
    entry_url: String,
}

static NEXT_WEBSOCKET_ID: AtomicU32 = AtomicU32::new(1);

fn spawn_connector_runtime(
    spec: ConnectorRuntimeSpec,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    service_router: ServiceCallRouter,
    app_handle: Option<AppHandle>,
) -> Result<ConnectorRuntimeHandle, String> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let connector_ref = spec.connector_ref();
    let fingerprint = format!("{spec:?}");
    let error_connector_ref = connector_ref.clone();
    let thread = std::thread::Builder::new()
        .name(format!("bakingrl-connector-{connector_ref}"))
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create connector runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async move {
                if let Err(err) =
                    run_connector_runtime(spec, bus, registry, service_router, shutdown_rx).await
                {
                    error!("Connector runtime failed: {}", err);
                    if let Some(app_handle) = app_handle.as_ref() {
                        emit_runtime_error(app_handle, "connector", &error_connector_ref, &err);
                    }
                }
            });
        })
        .map_err(|e| format!("Unable to spawn connector runtime thread: {e}"))?;

    Ok(ConnectorRuntimeHandle {
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

async fn run_connector_runtime(
    spec: ConnectorRuntimeSpec,
    bus: Arc<EventBus>,
    registry: Arc<Registry>,
    service_router: ServiceCallRouter,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let connector_ref = spec.connector_ref();
    let (event_tx, event_rx) = mpsc::channel::<serde_json::Value>(RUNTIME_EVENT_QUEUE);
    let event_rx = Rc::new(tokio::sync::Mutex::new(event_rx));
    let subscriptions: Subscriptions = Arc::new(Mutex::new(HashSet::new()));
    let context = ConnectorRuntimeContext {
        package_id: spec.package_id.clone(),
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

    let module_specs = spec.module_specs();
    let modules = module_specs
        .iter()
        .map(|module| {
            let entry_url = url::Url::from_file_path(&module.entry_path).map_err(|_| {
                format!(
                    "Unable to convert connector entry to file URL: {:?}",
                    module.entry_path
                )
            })?;
            Ok(ConnectorBootstrapModule {
                connector_ref: format!("{}/{}", spec.package_id, module.connector_name),
                entry_url: entry_url.to_string(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bootstrap = connector_bootstrap(&modules, &spec.settings)?;
    let bootstrap_url = deno_core::resolve_path(
        &format!("./bakingrl-connector-{}.js", spec.connector_name),
        &std::env::current_dir().map_err(|e| format!("Unable to resolve cwd: {e}"))?,
    )
    .map_err(|e| format!("Unable to resolve connector bootstrap module: {e}"))?;
    let module_loader = PackageModuleLoader::from_entry_paths(
        module_specs.iter().map(|module| module.entry_path.clone()),
        &spec.storage_root,
    )
    .with_virtual_module(&bootstrap_url, bootstrap.clone());
    let mut js_runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(module_loader)),
        extensions: vec![bakingrl_connector::init()],
        ..Default::default()
    });
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Unable to create connector HTTP client: {e}"))?;

    {
        let op_state = js_runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.put::<Arc<EventBus>>(bus);
        state.put::<Arc<Registry>>(registry);
        state.put::<ServiceCallRouter>(service_router);
        state.put::<ConnectorRuntimeContext>(context);
        state.put::<EventReceiver>(event_rx);
        state.put::<Subscriptions>(subscriptions);
        state.put::<WebSocketMap>(Arc::new(Mutex::new(HashMap::new())));
        state.put::<reqwest::Client>(http_client);
    }

    let mod_id = js_runtime
        .load_main_es_module_from_code(&bootstrap_url, bootstrap)
        .await
        .map_err(|e| format!("Unable to load connector module '{connector_ref}': {e}"))?;
    let result = js_runtime.mod_evaluate(mod_id);
    js_runtime
        .run_event_loop(Default::default())
        .await
        .map_err(|e| format!("Connector event loop failed for '{connector_ref}': {e}"))?;
    result
        .await
        .map_err(|e| format!("Connector module evaluation failed for '{connector_ref}': {e}"))?;
    info!("Connector runtime '{connector_ref}' exited.");
    Ok(())
}

fn forward_bus_events(
    context: ConnectorRuntimeContext,
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

fn connector_bootstrap(
    modules: &[ConnectorBootstrapModule],
    settings: &serde_json::Value,
) -> Result<String, String> {
    let modules_json = serde_json::to_string(modules)
        .map_err(|e| format!("Unable to serialize connector bootstrap modules: {e}"))?;
    let settings_json = serde_json::to_string(settings)
        .map_err(|e| format!("Unable to serialize connector settings: {e}"))?;
    Ok(format!(
        r#"
const connectorEntries = {modules_json};
const packageSettings = {settings_json};
const connectors = [];
for (const entry of connectorEntries) {{
  const connectorModule = await import(entry.entryUrl);
  const connector = connectorModule.default ?? connectorModule;
  if (!connector || typeof connector !== "object") {{
    throw new Error(`Connector module '${{entry.connectorRef}}' must export an object.`);
  }}
  connectors.push({{ ref: entry.connectorRef, connector }});
}}
const listeners = new Map();
const context = {{
  bus: {{
    subscribe(eventName, callback) {{
      globalThis.Deno.core.ops.op_connector_subscribe(eventName);
      const callbacks = listeners.get(eventName) ?? new Set();
      callbacks.add(callback);
      listeners.set(eventName, callbacks);
      return () => callbacks.delete(callback);
    }},
    emit(eventName, payload) {{
      globalThis.Deno.core.ops.op_connector_bus_emit(eventName, payload ?? null);
    }}
  }},
  registry: {{
    get(key) {{
      return globalThis.Deno.core.ops.op_connector_registry_get(key);
    }},
    set(key, value) {{
      globalThis.Deno.core.ops.op_connector_registry_set(key, value ?? null);
    }}
  }},
  storage: {{
    async readText(uri) {{
      return globalThis.Deno.core.ops.op_connector_storage_read(uri);
    }},
    async writeText(uri, contents) {{
      globalThis.Deno.core.ops.op_connector_storage_write(uri, String(contents ?? ""));
    }}
  }},
  services: {{
    async call(ref, method, input) {{
      return await globalThis.Deno.core.ops.op_connector_service_call(String(ref), String(method), input ?? null);
    }}
  }},
  settings: {{
    get(key) {{ return packageSettings?.[String(key)]; }},
    all() {{ return JSON.parse(JSON.stringify(packageSettings ?? {{}})); }}
  }},
  fetch(url, init) {{
    return globalThis.Deno.core.ops.op_connector_fetch(String(url), init ?? null);
  }},
  websocket: {{
    async connect(url) {{
      const id = await globalThis.Deno.core.ops.op_connector_websocket_connect(String(url));
      return {{
        id,
        send(message) {{
          globalThis.Deno.core.ops.op_connector_websocket_send(id, String(message ?? ""));
        }},
        next() {{
          return globalThis.Deno.core.ops.op_connector_websocket_next(id);
        }},
        close() {{
          globalThis.Deno.core.ops.op_connector_websocket_close(id);
        }}
      }};
    }}
  }},
  diagnostics: console
}};
for (const entry of connectors) {{
  await entry.connector.mount?.(context);
}}
async function dispatchEvents() {{
  while (true) {{
    let event;
    try {{
      event = await globalThis.Deno.core.ops.op_connector_next_event();
    }} catch (error) {{
      if (String(error).includes("Connector event channel closed")) return;
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
try {{
  await dispatchEvents();
}} finally {{
  for (const entry of connectors) {{
    await entry.connector.unmount?.();
  }}
}}
"#
    ))
}

#[op2(fast)]
pub fn op_connector_subscribe(
    state: &mut OpState,
    #[string] event_name: String,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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
pub async fn op_connector_next_event(
    state: Rc<RefCell<OpState>>,
) -> Result<serde_json::Value, JsErrorBox> {
    let rx = {
        let state = state.borrow();
        state.borrow::<EventReceiver>().clone()
    };
    let mut rx = rx.lock().await;
    rx.recv()
        .await
        .ok_or_else(|| JsErrorBox::generic("Connector event channel closed"))
}

#[op2]
pub fn op_connector_bus_emit(
    state: &mut OpState,
    #[string] event_name: String,
    #[serde] payload: serde_json::Value,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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
pub fn op_connector_registry_get(
    state: &mut OpState,
    #[string] key: String,
) -> Result<serde_json::Value, JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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
pub fn op_connector_registry_set(
    state: &mut OpState,
    #[string] key: String,
    #[serde] value: serde_json::Value,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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
pub fn op_connector_storage_read(
    state: &mut OpState,
    #[string] uri: String,
) -> Result<String, JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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
pub fn op_connector_storage_write(
    state: &mut OpState,
    #[string] uri: String,
    #[string] contents: String,
) -> Result<(), JsErrorBox> {
    let context = state.borrow::<ConnectorRuntimeContext>().clone();
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

#[op2]
#[serde]
pub async fn op_connector_service_call(
    state: Rc<RefCell<OpState>>,
    #[string] service_ref: String,
    #[string] method: String,
    #[serde] input: serde_json::Value,
) -> Result<serde_json::Value, JsErrorBox> {
    let (context, router) = {
        let state = state.borrow();
        (
            state.borrow::<ConnectorRuntimeContext>().clone(),
            state.borrow::<ServiceCallRouter>().clone(),
        )
    };
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

#[op2]
#[serde]
pub async fn op_connector_fetch(
    state: Rc<RefCell<OpState>>,
    #[string] request_url: String,
    #[serde] init: serde_json::Value,
) -> Result<serde_json::Value, JsErrorBox> {
    let (context, client) = {
        let state = state.borrow();
        (
            state.borrow::<ConnectorRuntimeContext>().clone(),
            state.borrow::<reqwest::Client>().clone(),
        )
    };
    let parsed = url::Url::parse(&request_url).map_err(JsErrorBox::from_err)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| JsErrorBox::generic("Connector fetch URL is missing a host"))?;
    if !context.permissions.can_use_http_host(host) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot fetch host '{}'.",
            context.package_id, host
        )));
    }
    connector_fetch(&client, &request_url, init).await
}

async fn connector_fetch(
    client: &reqwest::Client,
    request_url: &str,
    init: serde_json::Value,
) -> Result<serde_json::Value, JsErrorBox> {
    let method = init
        .get("method")
        .and_then(|method| method.as_str())
        .unwrap_or("GET")
        .parse()
        .map_err(|e| JsErrorBox::generic(format!("Invalid fetch method: {e}")))?;
    let mut request = client.request(method, request_url);
    if let Some(headers) = init.get("headers").and_then(|headers| headers.as_object()) {
        for (key, value) in headers {
            if let Some(value) = value.as_str() {
                request = request.header(key, value);
            }
        }
    }
    if let Some(body) = init.get("body").and_then(|body| body.as_str()) {
        request = request.body(body.to_string());
    }
    let response = request
        .send()
        .await
        .map_err(|e| JsErrorBox::generic(format!("Connector fetch failed: {e}")))?;
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_FETCH_BYTES as u64)
    {
        return Err(JsErrorBox::generic(
            "Connector fetch response is too large.",
        ));
    }
    let headers = response
        .headers()
        .iter()
        .filter_map(|(key, value)| {
            value.to_str().ok().map(|value| {
                (
                    key.as_str().to_string(),
                    serde_json::Value::String(value.to_string()),
                )
            })
        })
        .collect::<serde_json::Map<_, _>>();
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            JsErrorBox::generic(format!("Unable to read connector fetch response: {e}"))
        })?;
        if bytes.len().saturating_add(chunk.len()) > MAX_FETCH_BYTES {
            return Err(JsErrorBox::generic(
                "Connector fetch response is too large.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&bytes).to_string();
    Ok(serde_json::json!({
        "status": status,
        "headers": headers,
        "body": body
    }))
}

#[op2]
pub async fn op_connector_websocket_connect(
    state: Rc<RefCell<OpState>>,
    #[string] request_url: String,
) -> Result<u32, JsErrorBox> {
    let (context, sockets) = {
        let state = state.borrow();
        (
            state.borrow::<ConnectorRuntimeContext>().clone(),
            state.borrow::<WebSocketMap>().clone(),
        )
    };
    let parsed = url::Url::parse(&request_url).map_err(JsErrorBox::from_err)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| JsErrorBox::generic("Connector WebSocket URL is missing a host"))?;
    if !context.permissions.can_use_websocket_host(host) {
        return Err(JsErrorBox::generic(format!(
            "Package '{}' cannot open WebSocket host '{}'.",
            context.package_id, host
        )));
    }
    let (stream, _) = tokio_tungstenite::connect_async(&request_url)
        .await
        .map_err(|e| JsErrorBox::generic(format!("Connector WebSocket connect failed: {e}")))?;
    let (mut writer, mut reader) = stream.split();
    let (write_tx, mut write_rx) = mpsc::channel::<TungsteniteMessage>(WEBSOCKET_WRITE_QUEUE);
    let (read_tx, read_rx) = mpsc::channel::<serde_json::Value>(WEBSOCKET_READ_QUEUE);
    let socket_id = NEXT_WEBSOCKET_ID.fetch_add(1, Ordering::Relaxed);

    tokio::task::spawn_local(async move {
        while let Some(message) = write_rx.recv().await {
            if writer.send(message).await.is_err() {
                break;
            }
        }
    });
    tokio::task::spawn_local(async move {
        while let Some(message) = reader.next().await {
            let value = match message {
                Ok(TungsteniteMessage::Text(text)) => serde_json::json!({
                    "type": "text",
                    "data": text.to_string()
                }),
                Ok(TungsteniteMessage::Binary(bytes)) => serde_json::json!({
                    "type": "binary",
                    "data": hex::encode(bytes)
                }),
                Ok(TungsteniteMessage::Close(_)) => {
                    let _ = read_tx.send(serde_json::json!({ "type": "close" })).await;
                    break;
                }
                Ok(_) => continue,
                Err(err) => {
                    let _ = read_tx
                        .send(serde_json::json!({
                            "type": "error",
                            "error": err.to_string()
                        }))
                        .await;
                    break;
                }
            };
            if read_tx.send(value).await.is_err() {
                break;
            }
        }
    });

    sockets.lock().unwrap().insert(
        socket_id,
        WebSocketHandle {
            tx: write_tx,
            rx: Arc::new(tokio::sync::Mutex::new(read_rx)),
        },
    );
    Ok(socket_id)
}

#[op2(fast)]
pub fn op_connector_websocket_send(
    state: &mut OpState,
    socket_id: u32,
    #[string] message: String,
) -> Result<(), JsErrorBox> {
    let sockets = state.borrow::<WebSocketMap>().clone();
    let sockets = sockets.lock().unwrap();
    let socket = sockets
        .get(&socket_id)
        .ok_or_else(|| JsErrorBox::generic(format!("Unknown WebSocket id '{socket_id}'")))?;
    socket
        .tx
        .try_send(TungsteniteMessage::Text(message.into()))
        .map_err(|_| {
            JsErrorBox::generic(format!("WebSocket '{socket_id}' is overloaded or closed"))
        })
}

#[op2]
#[serde]
pub async fn op_connector_websocket_next(
    state: Rc<RefCell<OpState>>,
    socket_id: u32,
) -> Result<serde_json::Value, JsErrorBox> {
    let rx = {
        let state = state.borrow();
        let sockets = state.borrow::<WebSocketMap>().clone();
        let sockets = sockets.lock().unwrap();
        sockets
            .get(&socket_id)
            .map(|socket| socket.rx.clone())
            .ok_or_else(|| JsErrorBox::generic(format!("Unknown WebSocket id '{socket_id}'")))?
    };
    let mut rx = rx.lock().await;
    rx.recv()
        .await
        .ok_or_else(|| JsErrorBox::generic(format!("WebSocket '{socket_id}' is closed")))
}

#[op2(fast)]
pub fn op_connector_websocket_close(state: &mut OpState, socket_id: u32) -> Result<(), JsErrorBox> {
    let sockets = state.borrow::<WebSocketMap>().clone();
    let Some(socket) = sockets.lock().unwrap().remove(&socket_id) else {
        return Err(JsErrorBox::generic(format!(
            "Unknown WebSocket id '{socket_id}'"
        )));
    };
    let _ = socket.tx.try_send(TungsteniteMessage::Close(None));
    Ok(())
}

deno_core::extension!(
    bakingrl_connector,
    ops = [
        op_connector_subscribe,
        op_connector_next_event,
        op_connector_bus_emit,
        op_connector_registry_get,
        op_connector_registry_set,
        op_connector_storage_read,
        op_connector_storage_write,
        op_connector_service_call,
        op_connector_fetch,
        op_connector_websocket_connect,
        op_connector_websocket_send,
        op_connector_websocket_next,
        op_connector_websocket_close,
    ],
);

#[cfg(test)]
mod tests {
    use super::super::service_runtime::{ServiceRuntimeManager, ServiceRuntimeSpec};
    use super::*;
    use crate::plugin_v2::manifest::NetworkPermissionsV2;
    use crate::plugin_v2::permissions::{
        EffectiveBusPermissionsV2, EffectiveRegistryPermissionsV2, EffectiveStoragePermissionsV2,
    };

    fn empty_permissions() -> EffectivePackagePermissionsV2 {
        EffectivePackagePermissionsV2 {
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
        }
    }

    #[tokio::test]
    async fn connector_fetch_denies_undeclared_host_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("connector.js");
        std::fs::write(
            &entry_path,
            r#"
export default {
  async mount(context) {
    try {
      await context.fetch("http://denied.example.test/data");
    } catch (error) {
      context.registry.set("plugin.com.example.connector.result", error instanceof Error ? error.message : String(error));
    }
  }
};
"#,
        )
        .unwrap();

        let registry = Arc::new(Registry::new());
        let manager = ConnectorRuntimeManager::default();
        manager.reload(
            vec![ConnectorRuntimeSpec {
                package_id: "com.example.connector".to_string(),
                connector_name: "http".to_string(),
                entry_path,
                storage_root: dir.path().join("storage"),
                service_imports: vec![],
                service_methods: HashMap::new(),
                permissions: EffectivePackagePermissionsV2 {
                    bus: EffectiveBusPermissionsV2 {
                        read: vec![],
                        publish: vec![],
                    },
                    registry: EffectiveRegistryPermissionsV2 {
                        read: vec![],
                        write: vec!["plugin.com.example.connector.*".to_string()],
                    },
                    network: NetworkPermissionsV2 {
                        http: vec!["allowed.example.test".to_string()],
                        websocket: vec![],
                    },
                    storage: EffectiveStoragePermissionsV2 {
                        read: vec![],
                        write: vec![],
                    },
                },
                settings: serde_json::json!({}),
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            registry.clone(),
            ServiceCallRouter::default(),
        );

        for _ in 0..50 {
            if registry
                .get("plugin.com.example.connector.result")
                .is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let result = registry
            .get("plugin.com.example.connector.result")
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap();
        assert!(result.contains("cannot fetch host 'denied.example.test'"));
    }

    #[tokio::test]
    async fn connector_storage_is_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("connector.js");
        let storage_root = dir.path().join("storage");
        std::fs::write(
            &entry_path,
            r#"
export default {
  async mount(context) {
    await context.storage.writeText("plugin://self/data/value.txt", "stored");
    const value = await context.storage.readText("plugin://self/data/value.txt");
    context.registry.set("plugin.com.example.connector.result", value);
  }
};
"#,
        )
        .unwrap();

        let registry = Arc::new(Registry::new());
        let manager = ConnectorRuntimeManager::default();
        manager.reload(
            vec![ConnectorRuntimeSpec {
                package_id: "com.example.connector".to_string(),
                connector_name: "storage".to_string(),
                entry_path,
                storage_root: storage_root.clone(),
                service_imports: vec![],
                service_methods: HashMap::new(),
                permissions: EffectivePackagePermissionsV2 {
                    bus: EffectiveBusPermissionsV2 {
                        read: vec![],
                        publish: vec![],
                    },
                    registry: EffectiveRegistryPermissionsV2 {
                        read: vec![],
                        write: vec!["plugin.com.example.connector.*".to_string()],
                    },
                    network: Default::default(),
                    storage: EffectiveStoragePermissionsV2 {
                        read: vec!["plugin://self/*".to_string()],
                        write: vec!["plugin://self/*".to_string()],
                    },
                },
                settings: serde_json::json!({}),
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            registry.clone(),
            ServiceCallRouter::default(),
        );

        for _ in 0..50 {
            if registry
                .get("plugin.com.example.connector.result")
                .is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            registry.get("plugin.com.example.connector.result"),
            Some(serde_json::Value::String("stored".to_string()))
        );
        assert_eq!(
            std::fs::read_to_string(storage_root.join("data/value.txt")).unwrap(),
            "stored"
        );
    }

    #[tokio::test]
    async fn connector_can_call_declared_service_import() {
        let dir = tempfile::tempdir().unwrap();
        let service_entry = dir.path().join("service.js");
        let connector_entry = dir.path().join("connector.js");
        std::fs::write(
            &service_entry,
            r#"
export default {
  methods: {
    add(input) {
      return input.left + input.right;
    }
  }
};
"#,
        )
        .unwrap();
        std::fs::write(
            &connector_entry,
            r#"
export default {
  async mount(context) {
    const value = await context.services.call("com.example.math/add", "add", { left: 10, right: 32 });
    context.registry.set("plugin.com.example.connector.result", value);
  }
};
"#,
        )
        .unwrap();

        let service_manager = ServiceRuntimeManager::default();
        let service_methods =
            HashMap::from([("com.example.math/add".to_string(), vec!["add".to_string()])]);
        service_manager.reload(
            vec![ServiceRuntimeSpec {
                package_id: "com.example.math".to_string(),
                service_name: "add".to_string(),
                entry_path: service_entry,
                storage_root: dir.path().join("service-storage"),
                service_imports: vec![],
                service_methods: service_methods.clone(),
                permissions: empty_permissions(),
                settings: serde_json::json!({}),
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            Arc::new(Registry::new()),
        );

        let registry = Arc::new(Registry::new());
        let connector_manager = ConnectorRuntimeManager::default();
        connector_manager.reload(
            vec![ConnectorRuntimeSpec {
                package_id: "com.example.connector".to_string(),
                connector_name: "caller".to_string(),
                entry_path: connector_entry,
                storage_root: dir.path().join("connector-storage"),
                service_imports: vec!["com.example.math/add".to_string()],
                service_methods,
                permissions: EffectivePackagePermissionsV2 {
                    bus: EffectiveBusPermissionsV2 {
                        read: vec![],
                        publish: vec![],
                    },
                    registry: EffectiveRegistryPermissionsV2 {
                        read: vec![],
                        write: vec!["plugin.com.example.connector.*".to_string()],
                    },
                    network: Default::default(),
                    storage: EffectiveStoragePermissionsV2 {
                        read: vec![],
                        write: vec![],
                    },
                },
                settings: serde_json::json!({}),
                additional_modules: Vec::new(),
            }],
            Arc::new(EventBus::new(16)),
            registry.clone(),
            service_manager.router(),
        );

        for _ in 0..50 {
            if registry
                .get("plugin.com.example.connector.result")
                .is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            registry.get("plugin.com.example.connector.result"),
            Some(serde_json::Value::from(42))
        );
    }
}
