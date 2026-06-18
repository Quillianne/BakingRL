use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attacker {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Shortcut")]
    pub shortcut: i32,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Player {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "PrimaryId")]
    pub primary_id: Option<String>,
    #[serde(rename = "Shortcut")]
    pub shortcut: Option<i32>,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
    #[serde(rename = "Score")]
    pub score: Option<i32>,
    #[serde(rename = "Goals")]
    pub goals: Option<i32>,
    #[serde(rename = "Assists")]
    pub assists: Option<i32>,
    #[serde(rename = "Saves")]
    pub saves: Option<i32>,
    #[serde(rename = "Shots")]
    pub shots: Option<i32>,
    #[serde(rename = "Touches")]
    pub touches: Option<i32>,
    #[serde(rename = "CarTouches")]
    pub car_touches: Option<i32>,
    #[serde(rename = "Demos")]
    pub demos: Option<i32>,
    #[serde(rename = "bHasCar")]
    pub b_has_car: Option<bool>,
    #[serde(rename = "Speed")]
    pub speed: Option<f64>,
    #[serde(rename = "Boost")]
    pub boost: Option<i32>,
    #[serde(rename = "bBoosting")]
    pub b_boosting: Option<bool>,
    #[serde(rename = "bOnGround")]
    pub b_on_ground: Option<bool>,
    #[serde(rename = "bOnWall")]
    pub b_on_wall: Option<bool>,
    #[serde(rename = "bPowersliding")]
    pub b_powersliding: Option<bool>,
    #[serde(rename = "bDemolished")]
    pub b_demolished: Option<bool>,
    #[serde(rename = "bSupersonic")]
    pub b_supersonic: Option<bool>,
    #[serde(rename = "Attacker")]
    pub attacker: Option<Attacker>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Team {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
    #[serde(rename = "Score")]
    pub score: i32,
    #[serde(rename = "ColorPrimary")]
    pub color_primary: String,
    #[serde(rename = "ColorSecondary")]
    pub color_secondary: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BallState {
    #[serde(rename = "Speed")]
    pub speed: f64,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Target {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Shortcut")]
    pub shortcut: i32,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameState {
    #[serde(rename = "Teams")]
    pub teams: Vec<Team>,
    #[serde(rename = "TimeSeconds")]
    pub time_seconds: f64,
    #[serde(rename = "bOvertime")]
    pub b_overtime: bool,
    #[serde(rename = "Frame")]
    pub frame: Option<i32>,
    #[serde(rename = "Elapsed")]
    pub elapsed: Option<f64>,
    #[serde(rename = "Ball")]
    pub ball: BallState,
    #[serde(rename = "bReplay")]
    pub b_replay: bool,
    #[serde(rename = "bHasWinner")]
    pub b_has_winner: bool,
    #[serde(rename = "Winner")]
    pub winner: String,
    #[serde(rename = "Arena")]
    pub arena: String,
    #[serde(rename = "bHasTarget")]
    pub b_has_target: bool,
    #[serde(rename = "Target")]
    pub target: Option<Target>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateStatePayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "Players")]
    pub players: Vec<Player>,
    #[serde(rename = "Game")]
    pub game: GameState,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Location {
    #[serde(rename = "X")]
    pub x: f64,
    #[serde(rename = "Y")]
    pub y: f64,
    #[serde(rename = "Z")]
    pub z: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BallHitState {
    #[serde(rename = "PreHitSpeed")]
    pub pre_hit_speed: f64,
    #[serde(rename = "PostHitSpeed")]
    pub post_hit_speed: f64,
    #[serde(rename = "Location")]
    pub location: Location,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BallHitPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "Players")]
    pub players: Vec<PlayerRef>,
    #[serde(rename = "Ball")]
    pub ball: BallHitState,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerRef {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Shortcut")]
    pub shortcut: Option<i32>,
    #[serde(rename = "TeamNum")]
    pub team_num: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClockUpdatedSecondsPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "TimeSeconds")]
    pub time_seconds: i32,
    #[serde(rename = "bOvertime")]
    pub b_overtime: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SimpleMatchPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BallLastTouch {
    #[serde(rename = "Player")]
    pub player: PlayerRef,
    #[serde(rename = "Speed")]
    pub speed: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrossbarHitPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "BallLocation")]
    pub ball_location: Location,
    #[serde(rename = "BallSpeed")]
    pub ball_speed: f64,
    #[serde(rename = "ImpactForce")]
    pub impact_force: f64,
    #[serde(rename = "BallLastTouch")]
    pub ball_last_touch: BallLastTouch,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoalScoredPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "GoalSpeed")]
    pub goal_speed: f64,
    #[serde(rename = "GoalTime")]
    pub goal_time: f64,
    #[serde(rename = "ImpactLocation")]
    pub impact_location: Location,
    #[serde(rename = "Scorer")]
    pub scorer: PlayerRef,
    #[serde(rename = "Assister")]
    pub assister: Option<PlayerRef>,
    #[serde(rename = "BallLastTouch")]
    pub ball_last_touch: BallLastTouch,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchEndedPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "WinnerTeamNum")]
    pub winner_team_num: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatfeedEventPayload {
    #[serde(rename = "MatchGuid")]
    pub match_guid: Option<String>,
    #[serde(rename = "EventName")]
    pub event_name: String,
    #[serde(rename = "Type")]
    pub event_type: String,
    #[serde(rename = "MainTarget")]
    pub main_target: PlayerRef,
    #[serde(rename = "SecondaryTarget")]
    pub secondary_target: Option<PlayerRef>,
}

/// Représente les événements typés provenant de Rocket League
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "Event", content = "Data")]
pub enum RocketLeagueEventPayload {
    UpdateState(UpdateStatePayload),
    BallHit(BallHitPayload),
    ClockUpdatedSeconds(ClockUpdatedSecondsPayload),
    CountdownBegin(SimpleMatchPayload),
    CrossbarHit(CrossbarHitPayload),
    GoalReplayEnd(SimpleMatchPayload),
    GoalReplayStart(SimpleMatchPayload),
    GoalReplayWillEnd(SimpleMatchPayload),
    GoalScored(GoalScoredPayload),
    MatchCreated(SimpleMatchPayload),
    MatchInitialized(SimpleMatchPayload),
    MatchDestroyed(SimpleMatchPayload),
    MatchEnded(MatchEndedPayload),
    MatchPaused(SimpleMatchPayload),
    MatchUnpaused(SimpleMatchPayload),
    PodiumStart(SimpleMatchPayload),
    ReplayCreated(SimpleMatchPayload),
    ReplayWillEnd(SimpleMatchPayload),
    RoundStarted(SimpleMatchPayload),
    StatfeedEvent(StatfeedEventPayload),
}

/// Structure d'enveloppe générale
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameEvent {
    #[serde(rename = "Event")]
    pub event: String,
    #[serde(rename = "Data")]
    pub data: serde_json::Value, // On garde la data brute ici pour le fallback ou le log si besoin, mais on va changer l'ingestor pour utiliser directement l'enum taggé
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PackageStateFile {
    pub enabled: HashMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppBehaviorSettings {
    pub launch_at_startup: bool,
    pub close_will_hide: bool,
    pub start_minimized: bool,
}

impl Default for AppBehaviorSettings {
    fn default() -> Self {
        Self {
            launch_at_startup: false,
            close_will_hide: false,
            start_minimized: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecuritySettings {
    #[serde(default)]
    pub plugins_safe_mode: bool,
    #[serde(default)]
    pub disable_plugin_activation: bool,
    #[serde(default = "default_require_trusted_remote_packages")]
    pub require_trusted_remote_packages: bool,
    #[serde(default)]
    pub trusted_package_public_keys: Vec<String>,
}

fn default_require_trusted_remote_packages() -> bool {
    true
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            plugins_safe_mode: false,
            disable_plugin_activation: false,
            require_trusted_remote_packages: default_require_trusted_remote_packages(),
            trusted_package_public_keys: Vec::new(),
        }
    }
}

fn default_update_state_throttle_fps() -> u16 {
    30
}

#[derive(Debug, Serialize, Clone)]
pub struct TelemetrySettings {
    pub rocket_league_host: String,
    pub rocket_league_port: u16,
    pub update_state_throttle_fps: u16,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        Self {
            rocket_league_host: "127.0.0.1".to_string(),
            rocket_league_port: 49123,
            update_state_throttle_fps: default_update_state_throttle_fps(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelemetryConnectionStatus {
    pub state: String,
    pub message: Option<String>,
    pub host: String,
    pub port: u16,
    pub updated_at_ms: u64,
}

impl TelemetryConnectionStatus {
    pub fn new(state: &str, message: Option<String>, host: String, port: u16) -> Self {
        Self {
            state: state.to_string(),
            message,
            host,
            port,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct TelemetrySettingsInput {
    #[serde(default)]
    rocket_league_host: Option<String>,
    #[serde(default)]
    rocket_league_port: Option<u16>,
    #[serde(default, alias = "update_rate_fps")]
    update_state_throttle_fps: Option<u16>,
}

#[derive(Debug, Deserialize, Default)]
struct LegacyOverlaySettings {
    #[serde(default, alias = "update_rate_fps")]
    update_state_throttle_fps: Option<u16>,
}

#[derive(Debug, Deserialize, Default)]
struct AppSettingsInput {
    #[serde(default)]
    behavior: AppBehaviorSettings,
    #[serde(default)]
    security: SecuritySettings,
    #[serde(default)]
    telemetry: TelemetrySettingsInput,
    #[serde(default)]
    overlay: Option<LegacyOverlaySettings>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppSettings {
    pub behavior: AppBehaviorSettings,
    pub security: SecuritySettings,
    pub telemetry: TelemetrySettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            behavior: AppBehaviorSettings::default(),
            security: SecuritySettings::default(),
            telemetry: TelemetrySettings::default(),
        }
    }
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = AppSettingsInput::deserialize(deserializer)?;
        let telemetry_defaults = TelemetrySettings::default();
        let legacy_throttle = input
            .overlay
            .and_then(|overlay| overlay.update_state_throttle_fps);
        Ok(Self {
            behavior: input.behavior,
            security: input.security,
            telemetry: TelemetrySettings {
                rocket_league_host: input
                    .telemetry
                    .rocket_league_host
                    .unwrap_or(telemetry_defaults.rocket_league_host),
                rocket_league_port: input
                    .telemetry
                    .rocket_league_port
                    .unwrap_or(telemetry_defaults.rocket_league_port),
                update_state_throttle_fps: input
                    .telemetry
                    .update_state_throttle_fps
                    .or(legacy_throttle)
                    .unwrap_or(telemetry_defaults.update_state_throttle_fps),
            },
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PackageSettingsFile {
    #[serde(default)]
    pub values: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub configured_secrets: HashMap<String, HashMap<String, bool>>,
}
