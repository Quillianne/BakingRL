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

pub(crate) struct CommandCallRequest {
    pub command_ref: String,
    pub args: Vec<serde_json::Value>,
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

#[derive(Clone)]
pub(crate) struct CommandCallClient {
    label: String,
    tx: mpsc::Sender<CommandCallRequest>,
}

impl CommandCallClient {
    pub(crate) fn new_extension_host(label: String, tx: mpsc::Sender<CommandCallRequest>) -> Self {
        Self { label, tx }
    }

    fn call(
        &self,
        command_ref: &str,
        args: Vec<serde_json::Value>,
        response: oneshot::Sender<Result<serde_json::Value, String>>,
    ) -> Result<(), String> {
        self.tx
            .try_send(CommandCallRequest {
                command_ref: command_ref.to_string(),
                args,
                response,
            })
            .map_err(|_| format!("Command runtime '{}' is overloaded or stopped.", self.label))
    }
}

#[derive(Clone, Default)]
pub(crate) struct CommandCallRouter {
    clients: Arc<Mutex<HashMap<String, CommandCallClient>>>,
}

impl CommandCallRouter {
    pub(crate) fn insert(&self, command_ref: String, client: CommandCallClient) {
        self.clients.lock().unwrap().insert(command_ref, client);
    }

    pub(crate) fn remove(&self, command_ref: &str) {
        self.clients.lock().unwrap().remove(command_ref);
    }

    pub(crate) async fn call(
        &self,
        command_ref: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let client = self
            .clients
            .lock()
            .unwrap()
            .get(command_ref)
            .cloned()
            .ok_or_else(|| format!("Command runtime '{command_ref}' is not running."))?;
        let (response_tx, response_rx) = oneshot::channel();
        client.call(command_ref, args, response_tx)?;
        tokio::time::timeout(Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| format!("Command runtime '{command_ref}' timed out."))?
            .map_err(|_| format!("Command runtime '{command_ref}' did not answer."))?
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

    #[tokio::test]
    async fn command_router_reports_missing_runtime() {
        let router = CommandCallRouter::default();
        let error = router
            .call("pkg/open", vec![serde_json::json!({ "source": "test" })])
            .await
            .unwrap_err();
        assert_eq!(error, "Command runtime 'pkg/open' is not running.");
    }
}
