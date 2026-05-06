use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tracing::{error, info, warn};

use crate::bus::{BusEvent, EventBus};
use crate::models::{GameEvent, RocketLeagueEventPayload, TelemetryConnectionStatus};

pub type TelemetryStatusState = Arc<Mutex<TelemetryConnectionStatus>>;

pub async fn start_tcp_ingestor(
    bus: Arc<EventBus>,
    app_handle: AppHandle,
    status_state: TelemetryStatusState,
    host: String,
    port: u16,
) {
    let addr = format!("{host}:{port}");
    info!("Tentative de connexion au serveur TCP sur {}...", addr);

    loop {
        set_connection_status(
            &app_handle,
            &status_state,
            "connecting",
            Some(format!("Connecting to {addr}")),
            &host,
            port,
        );
        match TcpStream::connect(&addr).await {
            Ok(mut stream) => {
                info!("Connecté avec succès au flux de données Rocket League !");
                set_connection_status(&app_handle, &status_state, "connected", None, &host, port);
                let mut read_buffer = [0_u8; 8192];
                let mut json_buffer = String::new();

                loop {
                    match stream.read(&mut read_buffer).await {
                        Ok(0) => {
                            info!("La connexion a été fermée par le serveur.");
                            set_connection_status(
                                &app_handle,
                                &status_state,
                                "disconnected",
                                Some("Connection closed by Rocket League.".to_string()),
                                &host,
                                port,
                            );
                            break;
                        }
                        Ok(bytes_read) => {
                            json_buffer
                                .push_str(&String::from_utf8_lossy(&read_buffer[..bytes_read]));

                            while let Some(message) = take_next_json_message(&mut json_buffer) {
                                process_message(&bus, &message);
                            }

                            if json_buffer.len() > 1_048_576 {
                                warn!(
                                    "Tampon TCP trop volumineux sans JSON complet; purge du tampon."
                                );
                                json_buffer.clear();
                            }
                        }
                        Err(e) => {
                            error!("Erreur de lecture du socket : {}", e);
                            set_connection_status(
                                &app_handle,
                                &status_state,
                                "disconnected",
                                Some(e.to_string()),
                                &host,
                                port,
                            );
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Échec de la connexion : {}. Le serveur tourne-t-il ? Nouvelle tentative dans 5 secondes...", e);
                set_connection_status(
                    &app_handle,
                    &status_state,
                    "disconnected",
                    Some(e.to_string()),
                    &host,
                    port,
                );
            }
        }

        // Attente avant de reconnecter
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

fn set_connection_status(
    app_handle: &AppHandle,
    status_state: &TelemetryStatusState,
    state: &str,
    message: Option<String>,
    host: &str,
    port: u16,
) {
    let updated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let status = TelemetryConnectionStatus {
        state: state.to_string(),
        message,
        host: host.to_string(),
        port,
        updated_at_ms,
    };
    if let Ok(mut current) = status_state.lock() {
        *current = status.clone();
    }
    let _ = app_handle.emit("bakingrl-telemetry-status", status);
}

fn process_message(bus: &Arc<EventBus>, message: &str) {
    let trimmed = message.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return;
    }

    // Rocket League ou certains mocks envoient "Data" sous forme de string contenant du JSON.
    // Si c'est le cas, on doit le désérialiser avant de le passer au modèle typé.
    let mut parsed_val: Result<serde_json::Value, _> = serde_json::from_str(trimmed);
    if let Ok(ref mut val) = parsed_val {
        if let Some(data) = val.get_mut("Data") {
            if let Some(data_str) = data.as_str() {
                if let Ok(nested_json) = serde_json::from_str::<serde_json::Value>(data_str) {
                    *data = nested_json;
                }
            }
        }
    }

    match parsed_val {
        Ok(val) => match serde_json::from_value::<RocketLeagueEventPayload>(val.clone()) {
            Ok(_payload) => {
                let event_name = val
                    .get("Event")
                    .and_then(|e| e.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let inner_data = val.get("Data").cloned().unwrap_or(serde_json::Value::Null);

                let game_event = GameEvent {
                    event: event_name,
                    data: inner_data,
                };

                bus.publish(BusEvent::GameData(Arc::new(game_event)));
            }
            Err(err) => {
                warn!(
                    "Impossible de parser le JSON en événement Rocket League, on envoie le format brut. Erreur: {}",
                    err
                );
                bus.publish(BusEvent::RawJson(Arc::new(trimmed.to_string())));
            }
        },
        Err(err) => {
            warn!("JSON invalide : {}", err);
            bus.publish(BusEvent::RawJson(Arc::new(trimmed.to_string())));
        }
    }
}

fn take_next_json_message(buffer: &mut String) -> Option<String> {
    let start = match buffer.find('{') {
        Some(index) => index,
        None => {
            buffer.clear();
            return None;
        }
    };

    if start > 0 {
        buffer.drain(..start);
    }

    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in buffer.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = index + character.len_utf8();
                    let message = buffer[..end].to_string();
                    buffer.drain(..end);
                    return Some(message);
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::take_next_json_message;

    #[test]
    fn extracts_concatenated_json_messages() {
        let mut buffer =
            r#"{"Event":"UpdateState","Data":{}}{"Event":"BallHit","Data":{"Speed":12}}"#
                .to_string();

        assert_eq!(
            take_next_json_message(&mut buffer).as_deref(),
            Some(r#"{"Event":"UpdateState","Data":{}}"#)
        );
        assert_eq!(
            take_next_json_message(&mut buffer).as_deref(),
            Some(r#"{"Event":"BallHit","Data":{"Speed":12}}"#)
        );
        assert!(take_next_json_message(&mut buffer).is_none());
    }

    #[test]
    fn waits_for_incomplete_json_message() {
        let mut buffer = r#"{"Event":"UpdateState","Data":{"#.to_string();

        assert!(take_next_json_message(&mut buffer).is_none());

        buffer.push_str(r#""Players":[]}}"#);
        assert_eq!(
            take_next_json_message(&mut buffer).as_deref(),
            Some(r#"{"Event":"UpdateState","Data":{"Players":[]}}"#)
        );
    }

    #[test]
    fn ignores_braces_inside_strings() {
        let mut buffer = r#"{"Event":"UpdateState","Data":{"Name":"Player {One}"}}"#.to_string();

        assert_eq!(
            take_next_json_message(&mut buffer).as_deref(),
            Some(r#"{"Event":"UpdateState","Data":{"Name":"Player {One}"}}"#)
        );
    }
}
