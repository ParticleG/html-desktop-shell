mod applications;
mod audio;
pub(crate) mod desktop;
mod icons;
mod niri;
mod niri_collector;
mod notifications;
mod system;
mod tray;

use std::rc::Rc;

use gtk4::gio::prelude::ListModelExt;

use crate::messages::{BRIDGE_VERSION, NativeMethodError};

use niri::{NiriController, NiriProvider};
pub(crate) use niri::{WorkspaceActivation, WorkspaceActivationHandlerId};

use applications::ApplicationsController;
use audio::AudioController;
use desktop::{DesktopRequest, ScreenPoint};
use notifications::NotificationsController;
use tray::TrayController;
pub(crate) use tray::TrayMenuSession;

struct DesktopProviders {
    audio: AudioController,
    tray: TrayController,
    notifications: NotificationsController,
    applications: ApplicationsController,
}

impl DesktopProviders {
    fn shutdown(&self) {
        self.audio.shutdown();
        self.tray.shutdown();
        self.notifications.shutdown();
        self.applications.shutdown();
    }
}

impl Drop for DesktopProviders {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> serde_json::Value;

    fn diagnostic_snapshot(&self) -> serde_json::Value {
        self.snapshot()
    }
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Rc<Vec<Box<dyn Provider>>>,
    niri: NiriController,
    desktop: Option<Rc<DesktopProviders>>,
}

impl ProviderRegistry {
    pub fn new(monitors: &gtk4::gio::ListModel) -> Self {
        let niri = NiriController::new();
        let desktop = Rc::new(DesktopProviders {
            audio: AudioController::new(),
            tray: TrayController::new(),
            notifications: NotificationsController::new(),
            applications: ApplicationsController::new(),
        });
        Self {
            providers: Rc::new(vec![
                Box::new(ClockProvider),
                Box::new(HostProvider {
                    monitors: monitors.clone(),
                }),
                Box::new(NiriProvider::new(niri.clone())),
                Box::new(system::BatteryProvider::new_default()),
                Box::new(system::NetworkProvider::new_default()),
                Box::new(desktop.audio.clone()),
                Box::new(desktop.tray.clone()),
                Box::new(desktop.notifications.clone()),
                Box::new(desktop.applications.clone()),
            ]),
            niri,
            desktop: Some(desktop),
        }
    }

    pub fn snapshot_for(&self, kind: crate::surface::SurfaceKind) -> serde_json::Value {
        let mut state = serde_json::Map::with_capacity(self.providers.len());
        for provider in self.providers.iter() {
            let snapshot = if kind == crate::surface::SurfaceKind::DiagnosticsDialog {
                provider.diagnostic_snapshot()
            } else {
                provider.snapshot()
            };
            state.insert(provider.name().to_owned(), snapshot);
        }
        serde_json::Value::Object(state)
    }

    pub(crate) fn execute_desktop(
        &self,
        request: DesktopRequest,
        point: ScreenPoint,
        launch_context: gtk4::gio::AppLaunchContext,
        reply: impl FnOnce(Result<serde_json::Value, NativeMethodError>) + 'static,
    ) {
        let Some(desktop) = &self.desktop else {
            reply(Err(NativeMethodError::new(
                "service_unavailable",
                "desktop providers are unavailable",
            )));
            return;
        };
        match request {
            DesktopRequest::SetAudioVolume { volume } => desktop.audio.set_volume(volume, reply),
            DesktopRequest::SetAudioMuted { muted } => desktop.audio.set_muted(muted, reply),
            DesktopRequest::GetTrayMenu {
                item_id,
                menu_item_id,
            } => desktop.tray.get_menu(item_id, menu_item_id, reply),
            DesktopRequest::ActivateTrayItem { item_id } => {
                desktop.tray.activate(item_id, point, false, reply)
            }
            DesktopRequest::SecondaryActivateTrayItem { item_id } => {
                desktop.tray.activate(item_id, point, true, reply)
            }
            DesktopRequest::ScrollTrayItem {
                item_id,
                delta,
                orientation,
            } => desktop.tray.scroll(item_id, delta, orientation, reply),
            DesktopRequest::ActivateTrayMenuItem {
                item_id,
                menu_item_id,
                revision,
            } => desktop
                .tray
                .activate_menu(item_id, menu_item_id, revision, reply),
            DesktopRequest::GetNotifications => reply(desktop.notifications.get_notifications()),
            DesktopRequest::DismissNotification { notification_id } => {
                reply(desktop.notifications.dismiss(notification_id))
            }
            DesktopRequest::InvokeNotificationAction {
                notification_id,
                action_key,
            } => reply(
                desktop
                    .notifications
                    .invoke_action(notification_id, action_key),
            ),
            DesktopRequest::MarkAllNotificationsRead => {
                reply(desktop.notifications.mark_all_read())
            }
            DesktopRequest::SearchApplications { query, limit } => {
                reply(desktop.applications.search(&query, limit))
            }
            DesktopRequest::LaunchApplication { application_id } => {
                desktop
                    .applications
                    .launch(application_id, launch_context, reply)
            }
        }
    }

    pub(crate) fn bind_tray_menu(
        &self,
        item_id: &str,
        invalidated: impl FnOnce() + 'static,
    ) -> Result<TrayMenuSession, NativeMethodError> {
        self.desktop
            .as_ref()
            .ok_or_else(|| {
                NativeMethodError::new("tray_unavailable", "desktop providers are unavailable")
            })?
            .tray
            .bind_menu(item_id, invalidated)
    }

    pub(crate) fn validate_tray_menu(
        &self,
        session: &TrayMenuSession,
    ) -> Result<(), NativeMethodError> {
        self.desktop
            .as_ref()
            .ok_or_else(|| {
                NativeMethodError::new("tray_unavailable", "desktop providers are unavailable")
            })?
            .tray
            .validate_menu_session(session)
    }

    pub(crate) fn execute_tray_menu(
        &self,
        session: TrayMenuSession,
        request: DesktopRequest,
        reply: impl FnOnce(Result<serde_json::Value, NativeMethodError>) + 'static,
    ) {
        let Some(desktop) = &self.desktop else {
            reply(Err(NativeMethodError::new(
                "tray_unavailable",
                "desktop providers are unavailable",
            )));
            return;
        };
        match request {
            DesktopRequest::GetTrayMenu {
                item_id,
                menu_item_id,
            } if item_id == session.item_id() => {
                desktop.tray.get_bound_menu(session, menu_item_id, reply);
            }
            DesktopRequest::ActivateTrayMenuItem {
                item_id,
                menu_item_id,
                revision,
            } if item_id == session.item_id() => {
                desktop
                    .tray
                    .activate_bound_menu(session, menu_item_id, revision, reply);
            }
            _ => reply(Err(NativeMethodError::new(
                "surface_forbidden",
                "native tray menu is bound to a different item or operation",
            ))),
        }
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
        if let Some(desktop) = &self.desktop {
            desktop.shutdown();
        }
    }

    #[cfg(test)]
    fn from_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self::from_providers_with_niri(providers, NiriController::stopped())
    }

    #[cfg(test)]
    fn from_providers_with_niri(providers: Vec<Box<dyn Provider>>, niri: NiriController) -> Self {
        Self {
            providers: Rc::new(providers),
            niri,
            desktop: None,
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
