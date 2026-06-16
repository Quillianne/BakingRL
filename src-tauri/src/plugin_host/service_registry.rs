use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

pub(crate) struct ServiceCallRequest {
    pub service_ref: String,
    pub method: String,
    pub input: serde_json::Value,
    pub response: oneshot::Sender<Result<serde_json::Value, String>>,
}

#[derive(Clone)]
pub(crate) struct ServiceCallClient {
    label: String,
    tx: mpsc::Sender<ServiceCallRequest>,
}

impl ServiceCallClient {
    pub(crate) fn new(label: String, tx: mpsc::Sender<ServiceCallRequest>) -> Self {
        Self { label, tx }
    }

    fn call(
        &self,
        service_ref: &str,
        method: String,
        input: serde_json::Value,
        response: oneshot::Sender<Result<serde_json::Value, String>>,
    ) -> Result<(), String> {
        self.tx
            .try_send(ServiceCallRequest {
                service_ref: service_ref.to_string(),
                method,
                input,
                response,
            })
            .map_err(|_| format!("Service runtime '{}' is overloaded or stopped.", self.label))
    }
}

#[derive(Clone, Default)]
pub(crate) struct ServiceCallRouter {
    clients: Arc<Mutex<HashMap<String, ServiceCallClient>>>,
}

impl ServiceCallRouter {
    pub(crate) fn insert(&self, service_ref: String, client: ServiceCallClient) {
        self.clients.lock().unwrap().insert(service_ref, client);
    }

    pub(crate) fn remove(&self, service_ref: &str) {
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
        client.call(service_ref, method, input, response_tx)?;
        tokio::time::timeout(Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| format!("Service runtime '{service_ref}' timed out."))?
            .map_err(|_| format!("Service runtime '{service_ref}' did not answer."))?
    }
}
