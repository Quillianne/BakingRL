use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tracing::{error, info, warn};

use crate::bus::{BusEvent, EventBus};
use crate::models::{GameEvent, RocketLeagueEventPayload, TelemetryConnectionStatus};
use crate::plugin_host::PluginHost;

pub type TelemetryStatusState = Arc<Mutex<TelemetryConnectionStatus>>;

pub async fn start_tcp_ingestor(
    bus: Arc<EventBus>,
    app_handle: AppHandle,
    status_state: TelemetryStatusState,
    plugin_host: Arc<PluginHost>,
    host: String,
    port: u16,
) {
    let addr = format!("{host}:{port}");
    info!("Tentative de connexion au serveur TCP sur {}...", addr);
    let mut first_attempt = true;

    loop {
        if first_attempt {
            set_connection_status(
                &app_handle,
                &status_state,
                "connecting",
                Some(format!("Connecting to {addr}")),
                &host,
                port,
            );
            first_attempt = false;
        }
        match TcpStream::connect(&addr).await {
            Ok(mut stream) => {
                info!("Connecté avec succès au flux de données Rocket League !");
                set_connection_status(&app_handle, &status_state, "connected", None, &host, port);
                let mut read_buffer = [0_u8; 8192];
                let mut json_buffer = String::new();
                let mut rate_limiter = UpdateStateRateLimiter::default();

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
                                process_message(&bus, &plugin_host, &mut rate_limiter, &message);
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

#[derive(Default)]
struct UpdateStateRateLimiter {
    last_publish: Option<Instant>,
}

impl UpdateStateRateLimiter {
    fn should_publish(&mut self, plugin_host: &PluginHost, event_name: &str) -> bool {
        if event_name != "UpdateState" {
            return true;
        }

        let fps = plugin_host
            .get_app_settings()
            .telemetry
            .update_state_throttle_fps
            .max(1);
        let interval = Duration::from_millis((1000 / u64::from(fps)).max(1));
        let now = Instant::now();
        if self
            .last_publish
            .is_some_and(|last| now.duration_since(last) < interval)
        {
            return false;
        }
        self.last_publish = Some(now);
        true
    }
}

fn process_message(
    bus: &Arc<EventBus>,
    plugin_host: &PluginHost,
    rate_limiter: &mut UpdateStateRateLimiter,
    message: &str,
) {
    let trimmed = message.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return;
    }

    match parse_rocket_league_event_message(trimmed) {
        Ok(game_event) => {
            if !rate_limiter.should_publish(plugin_host, &game_event.event) {
                return;
            }
            bus.publish(BusEvent::GameData(Arc::new(game_event)));
        }
        Err(err) => {
            warn!("{err}");
            bus.publish(BusEvent::RawJson(Arc::new(trimmed.to_string())));
        }
    }
}

fn parse_rocket_league_event_message(message: &str) -> Result<GameEvent, String> {
    let trimmed = message.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return Err("Telemetry frame is empty.".to_string());
    }
    let value = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|err| format!("Invalid telemetry JSON: {err}"))?;
    game_event_from_rocket_league_frame(value)
}

fn game_event_from_rocket_league_frame(mut value: serde_json::Value) -> Result<GameEvent, String> {
    // Rocket League and some mocks send "Data" as a string containing JSON.
    // Normalize it before validating against the typed event model.
    if let Some(data) = value.get_mut("Data") {
        if let Some(data_str) = data.as_str() {
            if let Ok(nested_json) = serde_json::from_str::<serde_json::Value>(data_str) {
                *data = nested_json;
            }
        }
    }

    serde_json::from_value::<RocketLeagueEventPayload>(value.clone()).map_err(|err| {
        format!(
            "Unable to parse JSON as a supported Rocket League telemetry event; forwarding raw frame. Error: {err}"
        )
    })?;

    let event_name = value
        .get("Event")
        .and_then(|event| event.as_str())
        .filter(|event| !event.trim().is_empty())
        .ok_or_else(|| "Telemetry frame must contain a non-empty Event field.".to_string())?
        .to_string();
    let data = value
        .get("Data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Ok(GameEvent {
        event: event_name,
        data,
    })
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
    use super::{parse_rocket_league_event_message, take_next_json_message};

    fn player_ref(name: &str, team_num: u8) -> serde_json::Value {
        serde_json::json!({
            "Name": name,
            "Shortcut": team_num + 1,
            "TeamNum": team_num
        })
    }

    fn location() -> serde_json::Value {
        serde_json::json!({
            "X": 12.0,
            "Y": -34.0,
            "Z": 56.0
        })
    }

    fn ball_last_touch() -> serde_json::Value {
        serde_json::json!({
            "Player": player_ref("Blue", 0),
            "Speed": 950.0
        })
    }

    fn simple_match_payload() -> serde_json::Value {
        serde_json::json!({
            "MatchGuid": "mock-match"
        })
    }

    fn update_state_payload() -> serde_json::Value {
        serde_json::json!({
            "MatchGuid": "mock-match",
            "Players": [
                {
                    "Name": "Blue",
                    "PrimaryId": "Steam|123|0",
                    "Shortcut": 1,
                    "TeamNum": 0,
                    "Score": 120,
                    "Goals": 1,
                    "Assists": 0,
                    "Saves": 1,
                    "Shots": 2,
                    "Touches": 14,
                    "CarTouches": 3,
                    "Demos": 0,
                    "bHasCar": true,
                    "Speed": 1234.5,
                    "Boost": 42,
                    "bBoosting": true,
                    "bOnGround": true,
                    "bOnWall": false,
                    "bPowersliding": false,
                    "bDemolished": false,
                    "bSupersonic": true
                }
            ],
            "Game": {
                "Teams": [
                    {
                        "Name": "Blue",
                        "TeamNum": 0,
                        "Score": 2,
                        "ColorPrimary": "0000FF",
                        "ColorSecondary": "0000AA"
                    },
                    {
                        "Name": "Orange",
                        "TeamNum": 1,
                        "Score": 1,
                        "ColorPrimary": "FF7A00",
                        "ColorSecondary": "AA4400"
                    }
                ],
                "TimeSeconds": 123.4,
                "bOvertime": false,
                "Frame": 12,
                "Elapsed": 4.5,
                "Ball": {
                    "Speed": 1200.0,
                    "TeamNum": 255
                },
                "bReplay": false,
                "bHasWinner": false,
                "Winner": "",
                "Arena": "Stadium_P",
                "bHasTarget": true,
                "Target": player_ref("Blue", 0)
            }
        })
    }

    fn supported_event_frames() -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("UpdateState", update_state_payload()),
            (
                "BallHit",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "Players": [player_ref("Blue", 0)],
                    "Ball": {
                        "PreHitSpeed": 100.0,
                        "PostHitSpeed": 1200.0,
                        "Location": location()
                    }
                }),
            ),
            (
                "ClockUpdatedSeconds",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "TimeSeconds": 123,
                    "bOvertime": false
                }),
            ),
            ("CountdownBegin", simple_match_payload()),
            (
                "CrossbarHit",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "BallLocation": location(),
                    "BallSpeed": 870.3,
                    "ImpactForce": 127.5,
                    "BallLastTouch": ball_last_touch()
                }),
            ),
            ("GoalReplayEnd", simple_match_payload()),
            ("GoalReplayStart", simple_match_payload()),
            ("GoalReplayWillEnd", simple_match_payload()),
            (
                "GoalScored",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "GoalSpeed": 1550.0,
                    "GoalTime": 221.0,
                    "ImpactLocation": location(),
                    "Scorer": player_ref("Blue", 0),
                    "Assister": player_ref("Assist", 0),
                    "BallLastTouch": ball_last_touch()
                }),
            ),
            ("MatchCreated", simple_match_payload()),
            ("MatchInitialized", simple_match_payload()),
            ("MatchDestroyed", simple_match_payload()),
            (
                "MatchEnded",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "WinnerTeamNum": 0
                }),
            ),
            ("MatchPaused", simple_match_payload()),
            ("MatchUnpaused", simple_match_payload()),
            ("PodiumStart", simple_match_payload()),
            ("ReplayCreated", simple_match_payload()),
            ("ReplayWillEnd", simple_match_payload()),
            ("RoundStarted", simple_match_payload()),
            (
                "StatfeedEvent",
                serde_json::json!({
                    "MatchGuid": "mock-match",
                    "EventName": "Demolish",
                    "Type": "Demolition",
                    "MainTarget": player_ref("Blue", 0),
                    "SecondaryTarget": player_ref("Orange", 1)
                }),
            ),
        ]
    }

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

    #[test]
    fn parses_supported_rocket_league_event_frames() {
        for (event_name, data) in supported_event_frames() {
            let raw = serde_json::json!({
                "Event": event_name,
                "Data": data
            })
            .to_string();

            let event = parse_rocket_league_event_message(&raw).unwrap();

            assert_eq!(event.event, event_name);
            assert_eq!(event.data["MatchGuid"], "mock-match");
        }
    }

    #[test]
    fn preserves_extra_data_fields_after_typed_validation() {
        let mut data = update_state_payload();
        data["HostOnlyExtra"] = serde_json::json!({
            "kept": true
        });
        let raw = serde_json::json!({
            "Event": "UpdateState",
            "Data": data
        })
        .to_string();

        let event = parse_rocket_league_event_message(&raw).unwrap();

        assert_eq!(event.event, "UpdateState");
        assert_eq!(event.data["HostOnlyExtra"]["kept"], true);
    }

    #[test]
    fn parses_stringified_data_payloads() {
        let raw = serde_json::json!({
            "Event": "CountdownBegin",
            "Data": serde_json::json!({
                "MatchGuid": "string-data"
            })
            .to_string()
        })
        .to_string();

        let event = parse_rocket_league_event_message(&raw).unwrap();

        assert_eq!(event.event, "CountdownBegin");
        assert_eq!(event.data["MatchGuid"], "string-data");
    }

    #[test]
    fn rejects_unknown_rocket_league_event_frames() {
        let raw = serde_json::json!({
            "Event": "PluginOwnedEvent",
            "Data": {}
        })
        .to_string();

        let error = parse_rocket_league_event_message(&raw).unwrap_err();

        assert!(error.contains("supported Rocket League telemetry event"));
    }
}
