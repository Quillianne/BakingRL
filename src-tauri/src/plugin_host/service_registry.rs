use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::sidecar_runtime::SidecarRuntimeController;
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
    backend: ServiceCallBackend,
}

#[derive(Clone)]
enum ServiceCallBackend {
    ExtensionHost(mpsc::Sender<ServiceCallRequest>),
    Sidecar {
        package_id: String,
        sidecar_name: String,
        sidecars: SidecarRuntimeController,
    },
}

impl ServiceCallClient {
    pub(crate) fn new_extension_host(label: String, tx: mpsc::Sender<ServiceCallRequest>) -> Self {
        Self {
            label,
            backend: ServiceCallBackend::ExtensionHost(tx),
        }
    }

    pub(crate) fn new_sidecar(
        label: String,
        package_id: String,
        sidecar_name: String,
        sidecars: SidecarRuntimeController,
    ) -> Self {
        Self {
            label,
            backend: ServiceCallBackend::Sidecar {
                package_id,
                sidecar_name,
                sidecars,
            },
        }
    }

    fn call(
        &self,
        service_ref: &str,
        method: String,
        input: serde_json::Value,
        response: oneshot::Sender<Result<serde_json::Value, String>>,
    ) -> Result<(), String> {
        match &self.backend {
            ServiceCallBackend::ExtensionHost(tx) => tx
                .try_send(ServiceCallRequest {
                    service_ref: service_ref.to_string(),
                    method,
                    input,
                    response,
                })
                .map_err(|_| format!("Service runtime '{}' is overloaded or stopped.", self.label)),
            ServiceCallBackend::Sidecar {
                package_id,
                sidecar_name,
                sidecars,
            } => {
                let result = sidecars.call(package_id, sidecar_name, &method, input);
                let _ = response.send(result);
                Ok(())
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sidecar_clients_return_runtime_not_running_error() {
        let sidecars = SidecarRuntimeController::default();
        let client = ServiceCallClient::new_sidecar(
            "sidecar".to_string(),
            "pkg".to_string(),
            "helper".to_string(),
            sidecars,
        );

        let router = ServiceCallRouter::default();
        router.insert("pkg/service".to_string(), client);
        let error = router
            .call("pkg/service", "missing".to_string(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(error, "Sidecar runtime 'pkg/helper' is not running.");
    }
}
