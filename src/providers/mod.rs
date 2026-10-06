mod niri;
mod niri_collector;
mod system;

use std::rc::Rc;

use gtk4::gio::prelude::ListModelExt;

use crate::messages::{BRIDGE_VERSION, NativeMethodError};

use niri::{NiriController, NiriProvider};
pub(crate) use niri::{WorkspaceActivation, WorkspaceActivationHandlerId};

pub trait Provider {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> serde_json::Value;
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Rc<Vec<Box<dyn Provider>>>,
    niri: NiriController,
}

impl ProviderRegistry {
    pub fn new(monitors: &gtk4::gio::ListModel) -> Self {
        let niri = NiriController::new();
        Self::from_providers_with_niri(
            vec![
                Box::new(ClockProvider),
                Box::new(HostProvider {
                    monitors: monitors.clone(),
                }),
                Box::new(NiriProvider::new(niri.clone())),
                Box::new(system::BatteryProvider::new_default()),
                Box::new(system::NetworkProvider::new_default()),
            ],
            niri,
        )
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let mut state = serde_json::Map::with_capacity(self.providers.len());
        for provider in self.providers.iter() {
            state.insert(provider.name().to_owned(), provider.snapshot());
        }
        serde_json::Value::Object(state)
    }

    pub fn focus_workspace(
        &self,
        workspace_index: u64,
    ) -> Result<serde_json::Value, NativeMethodError> {
        self.niri.focus_workspace(workspace_index)
    }

    pub(crate) fn connect_workspace_activation(
        &self,
        handler: impl Fn(WorkspaceActivation) + 'static,
    ) -> WorkspaceActivationHandlerId {
        self.niri.connect_workspace_activation(handler)
    }

    pub(crate) fn disconnect_workspace_activation(&self, id: WorkspaceActivationHandlerId) {
        self.niri.disconnect_workspace_activation(id);
    }

    pub fn shutdown(&self) {
        self.niri.shutdown();
    }

    #[cfg(test)]
    fn from_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self::from_providers_with_niri(providers, NiriController::stopped())
    }

    fn from_providers_with_niri(providers: Vec<Box<dyn Provider>>, niri: NiriController) -> Self {
        Self {
            providers: Rc::new(providers),
            niri,
        }
    }
}

struct ClockProvider;

impl Provider for ClockProvider {
    fn name(&self) -> &'static str {
        "clock"
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({ "time": current_time() })
    }
}

struct HostProvider {
    monitors: gtk4::gio::ListModel,
}

impl Provider for HostProvider {
    fn name(&self) -> &'static str {
        "host"
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "backend": "wayland-layer-shell",
            "monitorCount": self.monitors.n_items(),
            "bridgeVersion": BRIDGE_VERSION,
        })
    }
}

fn current_time() -> String {
    glib::DateTime::now_local()
        .and_then(|date_time| date_time.format("%H:%M:%S"))
        .map(|time| time.to_string())
        .unwrap_or_else(|_| "--:--:--".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        name: &'static str,
        value: serde_json::Value,
    }

    impl Provider for StaticProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn snapshot(&self) -> serde_json::Value {
            self.value.clone()
        }
    }

    #[test]
    fn registry_returns_provider_snapshots_by_name() {
        let registry = ProviderRegistry::from_providers(vec![Box::new(StaticProvider {
            name: "demo",
            value: serde_json::json!({ "ok": true }),
        })]);

        assert_eq!(registry.snapshot()["demo"]["ok"], true);
    }

    #[test]
    fn clock_provider_returns_hh_mm_ss_time() {
        let time = current_time();
        let bytes = time.as_bytes();

        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[2], b':');
        assert_eq!(bytes[5], b':');
    }

    #[test]
    fn workspace_activation_handler_disconnects_by_typed_id() {
        let registry = ProviderRegistry::from_providers(Vec::new());
        let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
        let calls_for_handler = Rc::clone(&calls);
        let id = registry.connect_workspace_activation(move |activation| {
            calls_for_handler.borrow_mut().push(activation);
        });
        let activation = WorkspaceActivation {
            output: "eDP-1".to_owned(),
            workspace_index: 3,
        };

        registry.niri.emit_activation_for_test(activation.clone());
        registry.disconnect_workspace_activation(id);
        registry.niri.emit_activation_for_test(activation.clone());

        assert_eq!(*calls.borrow(), vec![activation]);
    }
}
