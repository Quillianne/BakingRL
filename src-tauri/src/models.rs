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
pub struct ObsSettings {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub access_token: String,
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            access_token: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeIsolation {
    Export,
    Package,
}

impl Default for PluginRuntimeIsolation {
    fn default() -> Self {
        Self::Export
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecuritySettings {
    #[serde(default)]
    pub plugin_runtime_isolation: PluginRuntimeIsolation,
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
            plugin_runtime_isolation: PluginRuntimeIsolation::default(),
            require_trusted_remote_packages: default_require_trusted_remote_packages(),
            trusted_package_public_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverlaySettings {
    pub hide_when_game_unfocused: bool,
    pub update_rate_fps: u16,
    #[serde(default = "default_use_monitor_size")]
    pub use_monitor_size: bool,
    #[serde(default)]
    pub monitor_id: Option<String>,
    #[serde(default = "default_screen_width")]
    pub screen_width: u32,
    #[serde(default = "default_screen_height")]
    pub screen_height: u32,
}

fn default_use_monitor_size() -> bool {
    true
}

fn default_screen_width() -> u32 {
    1920
}

fn default_screen_height() -> u32 {
    1080
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            hide_when_game_unfocused: true,
            update_rate_fps: 30,
            use_monitor_size: default_use_monitor_size(),
            monitor_id: None,
            screen_width: default_screen_width(),
            screen_height: default_screen_height(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelemetrySettings {
    pub rocket_league_host: String,
    pub rocket_league_port: u16,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        Self {
            rocket_league_host: "127.0.0.1".to_string(),
            rocket_league_port: 49123,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObsGatewayStatus {
    pub running: bool,
    pub address: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub behavior: AppBehaviorSettings,
    #[serde(default)]
    pub security: SecuritySettings,
    #[serde(default)]
    pub obs: ObsSettings,
    #[serde(default)]
    pub overlay: OverlaySettings,
    #[serde(default)]
    pub telemetry: TelemetrySettings,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PackageSettingsFile {
    #[serde(default)]
    pub values: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub configured_secrets: HashMap<String, HashMap<String, bool>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverlayItem {
    pub id: String,
    pub package_id: String,
    pub export_name: String,
    #[serde(default)]
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverlayLayer {
    pub id: String,
    pub name: String,
    #[serde(default = "default_layer_kind")]
    pub kind: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub items: Vec<OverlayItem>,
}

/// Host-owned composition that can be routed to an overlay runtime.
///
/// The overlay runtime is the display surface (in-game window or OBS route).
/// The layout is the editable content model rendered on that surface.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverlayLayout {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_overlay_width")]
    pub width: f64,
    #[serde(default = "default_overlay_height")]
    pub height: f64,
    #[serde(default)]
    pub layers: Vec<OverlayLayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OverlayItem>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub template_source: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

/// Persisted catalog of overlay layouts and their runtime routing.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverlayLayoutsFile {
    pub active_layout_id: String,
    #[serde(default = "default_stream_layout_id")]
    pub stream_layout_id: String,
    pub layouts: Vec<OverlayLayout>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageItem {
    pub id: String,
    #[serde(default = "default_page_item_kind")]
    pub kind: String,
    #[serde(default)]
    pub package_id: Option<String>,
    #[serde(default)]
    pub export_name: Option<String>,
    #[serde(default)]
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageLayer {
    pub id: String,
    pub name: String,
    #[serde(default = "default_layer_kind")]
    pub kind: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub items: Vec<PageItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageBackground {
    #[serde(default = "default_background_kind")]
    pub kind: String,
    #[serde(default = "default_page_background_color")]
    pub color: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default = "default_background_fit")]
    pub fit: String,
}

impl Default for PageBackground {
    fn default() -> Self {
        Self {
            kind: default_background_kind(),
            color: default_page_background_color(),
            image: None,
            fit: default_background_fit(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageSettings {
    #[serde(default = "default_page_open_target")]
    pub open_target: String,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            open_target: default_page_open_target(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageLayout {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_page_width")]
    pub width: f64,
    #[serde(default = "default_page_height")]
    pub height: f64,
    #[serde(default)]
    pub background: PageBackground,
    #[serde(default)]
    pub settings: PageSettings,
    #[serde(default)]
    pub layers: Vec<PageLayer>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub template_source: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PagesFile {
    pub pages: Vec<PageLayout>,
}

fn default_stream_layout_id() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

fn default_layer_kind() -> String {
    "normal".to_string()
}

fn default_overlay_width() -> f64 {
    1920.0
}

fn default_overlay_height() -> f64 {
    1080.0
}

fn default_page_item_kind() -> String {
    "visual".to_string()
}

fn default_background_kind() -> String {
    "color".to_string()
}

fn default_page_background_color() -> String {
    "#0f172a".to_string()
}

fn default_background_fit() -> String {
    "cover".to_string()
}

fn default_page_open_target() -> String {
    "app".to_string()
}

fn default_page_width() -> f64 {
    1440.0
}

fn default_page_height() -> f64 {
    900.0
}
