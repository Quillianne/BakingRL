use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_DIAGNOSTIC_LIMIT: usize = 500;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginDiagnosticSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiagnosticEvent {
    pub package_id: Option<String>,
    pub source: String,
    pub severity: PluginDiagnosticSeverity,
    pub phase: String,
    pub message: String,
    pub timestamp_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_count: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PluginDiagnosticInput {
    pub package_id: Option<String>,
    pub source: String,
    pub severity: PluginDiagnosticSeverity,
    pub phase: String,
    pub message: String,
    pub crash_count: Option<u32>,
}

#[derive(Clone)]
pub struct PluginDiagnosticsStore {
    events: Arc<Mutex<VecDeque<PluginDiagnosticEvent>>>,
    limit: usize,
}

impl PluginDiagnosticsStore {
    pub fn new(limit: usize) -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::with_capacity(limit.min(1024)))),
            limit: limit.max(1),
        }
    }

    pub fn push(&self, input: PluginDiagnosticInput) -> PluginDiagnosticEvent {
        let event = PluginDiagnosticEvent {
            package_id: input.package_id,
            source: input.source,
            severity: input.severity,
            phase: input.phase,
            message: input.message,
            timestamp_ms: now_ms(),
            crash_count: input.crash_count,
        };

        let mut events = self.events.lock().unwrap();
        while events.len() >= self.limit {
            events.pop_front();
        }
        events.push_back(event.clone());
        event
    }

    pub fn list(&self) -> Vec<PluginDiagnosticEvent> {
        self.events.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

impl Default for PluginDiagnosticsStore {
    fn default() -> Self {
        Self::new(DEFAULT_DIAGNOSTIC_LIMIT)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
