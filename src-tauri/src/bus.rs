use crate::models::GameEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

/// L'Event Bus interne de l'application.
/// Il transporte des Arc pour éviter de cloner des données en mémoire pour chaque plugin.
#[derive(Debug, Clone)]
pub enum BusEvent {
    /// Événement de télémétrie Rocket League.
    GameData(Arc<GameEvent>),
    /// Événement applicatif publié par un plugin.
    PluginEvent(Arc<GameEvent>),
    /// Donnée brute (JSON non parsé, au cas où)
    RawJson(Arc<String>),
}

impl BusEvent {
    pub fn name(&self) -> &str {
        match self {
            Self::GameData(event) | Self::PluginEvent(event) => event.event.as_str(),
            Self::RawJson(_) => "RawJson",
        }
    }
}

pub struct EventBus {
    sender: broadcast::Sender<BusEvent>,
    latest_game_event: std::sync::Mutex<Option<Arc<GameEvent>>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            latest_game_event: std::sync::Mutex::new(None),
        }
    }

    /// Publie un événement sur le bus
    pub fn publish(&self, event: BusEvent) {
        if let BusEvent::GameData(event) = &event {
            if let Ok(mut latest) = self.latest_game_event.lock() {
                *latest = Some(event.clone());
            }
        }
        // broadcast renvoie une erreur s'il n'y a pas de souscripteurs, on peut l'ignorer
        let _ = self.sender.send(event);
    }

    /// Crée un nouveau souscripteur pour écouter le bus
    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }

    pub fn latest_game_event(&self) -> Option<Arc<GameEvent>> {
        self.latest_game_event.lock().ok()?.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_game_event_tracks_published_game_data() {
        let bus = EventBus::new(16);
        assert!(bus.latest_game_event().is_none());

        bus.publish(BusEvent::GameData(Arc::new(GameEvent {
            event: "UpdateState".to_string(),
            data: serde_json::json!({ "MatchGuid": "mock-guid" }),
        })));

        let snapshot = bus.latest_game_event().expect("latest game event");
        assert_eq!(snapshot.event, "UpdateState");
        assert_eq!(snapshot.data["MatchGuid"], "mock-guid");
    }

    #[test]
    fn raw_json_does_not_replace_latest_game_event() {
        let bus = EventBus::new(16);
        bus.publish(BusEvent::GameData(Arc::new(GameEvent {
            event: "UpdateState".to_string(),
            data: serde_json::json!({ "MatchGuid": "first" }),
        })));
        bus.publish(BusEvent::RawJson(Arc::new(
            r#"{"Event":"Raw","Data":{}}"#.to_string(),
        )));

        let snapshot = bus.latest_game_event().expect("latest game event");
        assert_eq!(snapshot.event, "UpdateState");
        assert_eq!(snapshot.data["MatchGuid"], "first");
    }

    #[test]
    fn plugin_event_does_not_replace_latest_game_event() {
        let bus = EventBus::new(16);
        bus.publish(BusEvent::GameData(Arc::new(GameEvent {
            event: "UpdateState".to_string(),
            data: serde_json::json!({ "MatchGuid": "rl-snapshot" }),
        })));
        bus.publish(BusEvent::PluginEvent(Arc::new(GameEvent {
            event: "plugin.example.state".to_string(),
            data: serde_json::json!({ "status": "ready" }),
        })));

        let snapshot = bus.latest_game_event().expect("latest game event");
        assert_eq!(snapshot.event, "UpdateState");
        assert_eq!(snapshot.data["MatchGuid"], "rl-snapshot");
    }
}
