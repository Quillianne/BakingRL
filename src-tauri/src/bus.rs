use crate::models::GameEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

/// L'Event Bus interne de l'application.
/// Il transporte des Arc pour éviter de cloner des données en mémoire pour chaque plugin.
#[derive(Debug, Clone)]
pub enum BusEvent {
    /// Événement typé (provient de Rocket League ou d'un plugin)
    GameData(Arc<GameEvent>),
    /// Donnée brute (JSON non parsé, au cas où)
    RawJson(Arc<String>),
}

impl BusEvent {
    pub fn name(&self) -> &str {
        match self {
            Self::GameData(event) => event.event.as_str(),
            Self::RawJson(_) => "RawJson",
        }
    }
}

pub struct EventBus {
    sender: broadcast::Sender<BusEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publie un événement sur le bus
    pub fn publish(&self, event: BusEvent) {
        // broadcast renvoie une erreur s'il n'y a pas de souscripteurs, on peut l'ignorer
        let _ = self.sender.send(event);
    }

    /// Crée un nouveau souscripteur pour écouter le bus
    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }
}
