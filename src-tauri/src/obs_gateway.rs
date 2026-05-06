use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::{info, warn};

use crate::bus::{BusEvent, EventBus};
use crate::plugin_host::PluginHost;
use crate::registry::Registry;

#[derive(Clone)]
struct ObsGatewayState {
    bus: Arc<EventBus>,
    plugin_host: Arc<PluginHost>,
    registry: Arc<Registry>,
    frontend_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComponentSourceRequest {
    component_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceCallRequest {
    service_ref: String,
    method: String,
    input: serde_json::Value,
}

pub fn start_obs_gateway(
    bus: Arc<EventBus>,
    plugin_host: Arc<PluginHost>,
    registry: Arc<Registry>,
    listener: TcpListener,
    _shutdown_rx: oneshot::Receiver<()>,
) {
    tauri::async_runtime::spawn(async move {
        let addr = listener
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let frontend_dir = plugin_host.frontend_dist_dir();
        let state = ObsGatewayState {
            bus,
            plugin_host,
            registry,
            frontend_dir: frontend_dir.clone(),
        };
        let cors = CorsLayer::new()
            .allow_origin("*".parse::<HeaderValue>().unwrap())
            .allow_methods([Method::GET, Method::POST])
            .allow_headers(tower_http::cors::Any);
        let app = Router::new()
            .route("/api/plugins", get(list_packages))
            .route("/api/layouts", get(get_overlay_layouts))
            .route("/api/pages", get(get_pages))
            .route(
                "/api/packages/{package_id}/visuals/{export_name}/source",
                get(read_visual_export_source),
            )
            .route(
                "/api/packages/{package_id}/files/{*path}",
                get(read_package_file),
            )
            .route(
                "/api/packages/{package_id}/components/source",
                post(read_component_export_source),
            )
            .route(
                "/api/packages/{package_id}/services/call",
                post(call_service_export),
            )
            .route(
                "/api/packages/{package_id}/settings",
                get(get_package_settings),
            )
            .route(
                "/api/packages/{package_id}/registry/{key}",
                get(plugin_registry_get),
            )
            .route("/api/registry/{key}", get(registry_get))
            .route("/ws", get(ws_telemetry))
            .route("/", get(spa_index))
            .route("/overlay/ingame", get(spa_index))
            .route("/overlay/stream", get(spa_index))
            .route("/overlay/layout/{layout_id}", get(spa_index))
            .route("/editor/layout/{layout_id}", get(spa_index))
            .fallback_service(ServeDir::new(&frontend_dir))
            .layer(cors)
            .with_state(state);

        info!("Starting OBS v2 gateway on {}", addr);
        if let Err(err) = listener.set_nonblocking(true) {
            warn!("Unable to set OBS gateway listener nonblocking: {}", err);
            return;
        }
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(err) => {
                warn!("Failed to convert OBS gateway listener: {}", err);
                return;
            }
        };
        if let Err(err) = axum::serve(listener, app).await {
            warn!("OBS gateway stopped with error: {}", err);
        }
    });
}

async fn list_packages(State(state): State<ObsGatewayState>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(state.plugin_host.list_packages()).unwrap_or_default())
}

async fn get_overlay_layouts(State(state): State<ObsGatewayState>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(state.plugin_host.get_overlay_layouts()).unwrap_or_default())
}

async fn get_pages(State(state): State<ObsGatewayState>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(state.plugin_host.get_pages()).unwrap_or_default())
}

async fn read_visual_export_source(
    State(state): State<ObsGatewayState>,
    Path((package_id, export_name)): Path<(String, String)>,
) -> Response {
    text_result(
        state
            .plugin_host
            .read_visual_export_source(&package_id, &export_name),
    )
}

async fn read_package_file(
    State(state): State<ObsGatewayState>,
    Path((package_id, path)): Path<(String, String)>,
) -> Response {
    match state.plugin_host.read_package_file(&package_id, &path) {
        Ok(bytes) => {
            let content_type = content_type_for_path(&path);
            ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

async fn read_component_export_source(
    State(state): State<ObsGatewayState>,
    Path(package_id): Path<String>,
    Json(request): Json<ComponentSourceRequest>,
) -> Response {
    json_result(
        state
            .plugin_host
            .read_component_export_source(&package_id, &request.component_ref),
    )
}

async fn call_service_export(
    State(state): State<ObsGatewayState>,
    Path(package_id): Path<String>,
    Json(request): Json<ServiceCallRequest>,
) -> Response {
    json_result(
        state
            .plugin_host
            .call_service_export(
                &package_id,
                &request.service_ref,
                &request.method,
                request.input,
            )
            .await,
    )
}

async fn get_package_settings(
    State(state): State<ObsGatewayState>,
    Path(package_id): Path<String>,
) -> Response {
    json_result(state.plugin_host.get_package_settings(&package_id))
}

async fn plugin_registry_get(
    State(state): State<ObsGatewayState>,
    Path((package_id, key)): Path<(String, String)>,
) -> Response {
    json_result(
        state
            .plugin_host
            .can_package_read_registry(&package_id, &key)
            .map(|_| state.registry.get(&key)),
    )
}

async fn registry_get(State(state): State<ObsGatewayState>, Path(key): Path<String>) -> Response {
    json_result(Ok(state.registry.get(&key)))
}

async fn ws_telemetry(
    State(state): State<ObsGatewayState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let (mut sender, _) = socket.split();
        let mut rx = state.bus.subscribe();
        while let Ok(event) = rx.recv().await {
            let payload = match event {
                BusEvent::GameData(event) => serde_json::to_string(&*event).ok(),
                BusEvent::RawJson(raw) => Some((*raw).clone()),
            };
            let Some(payload) = payload else {
                continue;
            };
            if sender.send(Message::Text(payload.into())).await.is_err() {
                break;
            }
        }
    })
}

async fn spa_index(State(state): State<ObsGatewayState>) -> Response {
    match tokio::fs::read(state.frontend_dir.join("index.html")).await {
        Ok(bytes) => (
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unable to read frontend index.html: {err}"),
        )
            .into_response(),
    }
}

fn json_result<T: serde::Serialize>(result: Result<T, String>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => (axum::http::StatusCode::BAD_REQUEST, err).into_response(),
    }
}

fn text_result(result: Result<String, String>) -> Response {
    match result {
        Ok(value) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/javascript; charset=utf-8",
            )],
            value,
        )
            .into_response(),
        Err(err) => (axum::http::StatusCode::BAD_REQUEST, err).into_response(),
    }
}

fn content_type_for_path(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}
