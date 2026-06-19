use std::{process::Command, rc::Rc};

use gtk4::gio::prelude::ListModelExt;

use crate::messages::BRIDGE_VERSION;

pub trait Provider {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> serde_json::Value;
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Rc<Vec<Box<dyn Provider>>>,
}

impl ProviderRegistry {
    pub fn new(monitors: &gtk4::gio::ListModel) -> Self {
        Self::from_providers(vec![
            Box::new(ClockProvider),
            Box::new(HostProvider {
                monitors: monitors.clone(),
            }),
            Box::new(NiriProvider::new()),
        ])
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let mut state = serde_json::Map::with_capacity(self.providers.len());
        for provider in self.providers.iter() {
            state.insert(provider.name().to_owned(), provider.snapshot());
        }
        serde_json::Value::Object(state)
    }

    fn from_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self {
            providers: Rc::new(providers),
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

struct NiriProvider {
    detected: bool,
}

impl NiriProvider {
    fn new() -> Self {
        Self {
            detected: std::env::var_os("NIRI_SOCKET").is_some(),
        }
    }
}

impl Provider for NiriProvider {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn snapshot(&self) -> serde_json::Value {
        if !self.detected {
            return serde_json::json!({
                "available": false,
                "reason": "niri IPC unavailable",
            });
        }

        match focused_output_name() {
            Ok(focused_output) => serde_json::json!({
                "available": true,
                "focusedOutput": focused_output,
            }),
            Err(reason) => serde_json::json!({
                "available": false,
                "reason": reason,
            }),
        }
    }
}

fn current_time() -> String {
    glib::DateTime::now_local()
        .and_then(|date_time| date_time.format("%H:%M:%S"))
        .map(|time| time.to_string())
        .unwrap_or_else(|_| "--:--:--".to_owned())
}

fn focused_output_name() -> Result<String, String> {
    let output = Command::new("niri")
        .args(["msg", "-j", "focused-output"])
        .output()
        .map_err(|error| format!("failed to run niri msg: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("niri msg failed: {}", stderr.trim()));
    }

    let focused_output = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|error| format!("failed to parse niri focused-output JSON: {error}"))?;
    let Some(name) = focused_output
        .get("name")
        .and_then(serde_json::Value::as_str)
    else {
        return Err("niri focused-output response missing string name".to_owned());
    };

    Ok(name.to_owned())
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
    fn niri_provider_reports_unavailable_when_not_detected() {
        let provider = NiriProvider { detected: false };
        let snapshot = provider.snapshot();

        assert_eq!(snapshot["available"], false);
        assert_eq!(snapshot["reason"], "niri IPC unavailable");
    }

    #[test]
    fn clock_provider_returns_hh_mm_ss_time() {
        let time = current_time();
        let bytes = time.as_bytes();

        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[2], b':');
        assert_eq!(bytes[5], b':');
    }
}
