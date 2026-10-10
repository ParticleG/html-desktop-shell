use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::Duration,
};

use glib::SourceId;
use gtk4::{gio::prelude::ListModelExt, prelude::*};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use webkit6::prelude::*;

use crate::{
    config::ShellConfig,
    messages::NativeMethodError,
    providers::{
        ProviderRegistry, TrayMenuSession, WorkspaceActivation, WorkspaceActivationHandlerId,
    },
    surface::{SurfaceContext, SurfaceError, SurfaceKey, SurfaceKind},
    surface_window::{self, SurfaceInstance, TrayMenuInstance, TrayMenuPresentation},
    tray_overlay::{self, MenuAnchor, MenuAnchorSpace, MenuPlacement, NativeMenuTrigger},
    web_security::{TrustedWebContext, WebViewTrust},
};

const WORKSPACE_HUD_DURATION: Duration = Duration::from_millis(1500);
const TRAY_MENU_FIT_INTERVAL: Duration = Duration::from_millis(150);

type TrayMenuReply = Box<dyn FnOnce(Result<serde_json::Value, NativeMethodError>)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrayMenuGeometryPhase {
    AwaitingCaptures,
    AwaitingWorkarea,
    AwaitingMenuSize,
    Ready,
}

struct TrayMenuState {
    generation: u64,
    caller: SurfaceContext,
    caller_window: glib::WeakRef<gtk4::ApplicationWindow>,
    caller_view: glib::WeakRef<webkit6::WebView>,
    caller_trust: WebViewTrust,
    session: TrayMenuSession,
    replies: Vec<TrayMenuReply>,
    anchor: Option<MenuAnchor>,
    placement: Option<MenuPlacement>,
    popup_size: Option<(i32, i32)>,
    instance: Option<TrayMenuInstance>,
    bounds: (i32, i32),
    geometry_phase: TrayMenuGeometryPhase,
    dismiss_windows: Vec<gtk4::ApplicationWindow>,
    suspended_keyboard: Vec<(glib::WeakRef<gtk4::ApplicationWindow>, KeyboardMode)>,
    fit_timer: Option<SourceId>,
    measurement_inflight: bool,
    monitor_handlers: Vec<(gtk4::gdk::Monitor, [glib::SignalHandlerId; 3])>,
}

#[derive(Clone)]
struct MonitorBinding {
    monitor_index: u32,
    monitor: gtk4::gdk::Monitor,
    output: Option<String>,
}

struct HudTimer<T> {
    generation: u64,
    value: T,
}

type LifecycleContents<I, T> = (BTreeMap<SurfaceKey, I>, BTreeMap<u32, HudTimer<T>>);

struct LifecycleRegistry<I, T> {
    transients: BTreeMap<SurfaceKey, I>,
    hud_generations: BTreeMap<u32, u64>,
    hud_timers: BTreeMap<u32, HudTimer<T>>,
    stopped: bool,
}

impl<I, T> LifecycleRegistry<I, T> {
    fn new() -> Self {
        Self {
            transients: BTreeMap::new(),
            hud_generations: BTreeMap::new(),
            hud_timers: BTreeMap::new(),
            stopped: false,
        }
    }

    fn is_stopped(&self) -> bool {
        self.stopped
    }

    fn contains(&self, key: &SurfaceKey) -> bool {
        self.transients.contains_key(key)
    }

    fn insert_if_current(&mut self, key: SurfaceKey, instance: I, current: bool) -> Result<(), I> {
        if self.stopped || !current || self.transients.contains_key(&key) {
            Err(instance)
        } else {
            self.transients.insert(key, instance);
            Ok(())
        }
    }

    fn remove(&mut self, key: SurfaceKey) -> (Option<I>, Option<T>) {
        let instance = self.transients.remove(&key);
        let timer = (key.kind == SurfaceKind::WorkspaceHud)
            .then(|| self.hud_timers.remove(&key.monitor_index))
            .flatten()
            .map(|timer| timer.value);
        (instance, timer)
    }

    fn remove_if(
        &mut self,
        key: SurfaceKey,
        is_expected: impl FnOnce(&I) -> bool,
    ) -> Option<(I, Option<T>)> {
        if !self.transients.get(&key).is_some_and(is_expected) {
            return None;
        }
        let (instance, timer) = self.remove(key);
        instance.map(|instance| (instance, timer))
    }

    fn begin_hud(&mut self, monitor_index: u32) -> Option<(u64, Option<I>, Option<T>)> {
        if self.stopped {
            return None;
        }
        let generation = self
            .hud_generations
            .get(&monitor_index)
            .copied()
            .unwrap_or_default()
            .wrapping_add(1);
        self.hud_generations.insert(monitor_index, generation);
        let key = SurfaceKey::new(SurfaceKind::WorkspaceHud, monitor_index);
        let (previous, timer) = self.remove(key);
        Some((generation, previous, timer))
    }

    fn hud_is_current(&self, monitor_index: u32, generation: u64) -> bool {
        !self.stopped && self.hud_generations.get(&monitor_index) == Some(&generation)
    }

    fn install_hud_timer(
        &mut self,
        monitor_index: u32,
        generation: u64,
        timer: T,
    ) -> Result<(), T> {
        let key = SurfaceKey::new(SurfaceKind::WorkspaceHud, monitor_index);
        if self.hud_is_current(monitor_index, generation) && self.contains(&key) {
            self.hud_timers.insert(
                monitor_index,
                HudTimer {
                    generation,
                    value: timer,
                },
            );
            Ok(())
        } else {
            Err(timer)
        }
    }

    fn hud_timeout(&mut self, monitor_index: u32, generation: u64) -> Option<I> {
        let timer_is_current = self
            .hud_timers
            .get(&monitor_index)
            .is_some_and(|timer| timer.generation == generation);
        if !timer_is_current || !self.hud_is_current(monitor_index, generation) {
            return None;
        }
        self.hud_timers.remove(&monitor_index);
        self.transients
            .remove(&SurfaceKey::new(SurfaceKind::WorkspaceHud, monitor_index))
    }

    fn reset(&mut self) -> LifecycleContents<I, T> {
        self.hud_generations.clear();
        (
            std::mem::take(&mut self.transients),
            std::mem::take(&mut self.hud_timers),
        )
    }

    fn stop(&mut self) -> Option<LifecycleContents<I, T>> {
        if self.stopped {
            return None;
        }
        self.stopped = true;
        Some(self.reset())
    }
}

pub struct SurfaceManager {
    app: gtk4::Application,
    config: ShellConfig,
    providers: ProviderRegistry,
    web_context: TrustedWebContext,
    monitors: Vec<MonitorBinding>,
    bars: BTreeMap<SurfaceKey, SurfaceInstance>,
    lifecycle: LifecycleRegistry<SurfaceInstance, SourceId>,
    workspace_activation_handler: Option<WorkspaceActivationHandlerId>,
    tray_menu: Option<TrayMenuState>,
    tray_menu_generation: u64,
}

impl SurfaceManager {
    pub fn new(
        app: &gtk4::Application,
        monitors: &gtk4::gio::ListModel,
        config: ShellConfig,
        providers: ProviderRegistry,
        web_context: TrustedWebContext,
    ) -> Result<Rc<RefCell<Self>>, String> {
        let manager = Rc::new(RefCell::new(Self {
            app: app.clone(),
            config,
            providers: providers.clone(),
            web_context,
            monitors: Vec::new(),
            bars: BTreeMap::new(),
            lifecycle: LifecycleRegistry::new(),
            workspace_activation_handler: None,
            tray_menu: None,
            tray_menu_generation: 0,
        }));

        if let Err(error) = Self::build_initial_bars(&manager, monitors) {
            providers.shutdown();
            return Err(error);
        }

        let weak = Rc::downgrade(&manager);
        let handler = providers.connect_workspace_activation(move |activation| {
            let Some(manager) = weak.upgrade() else {
                return;
            };
            if let Err(error) = Self::show_workspace_hud(&manager, activation) {
                eprintln!("failed to show workspace HUD: {}", error.message());
            }
        });
        manager.borrow_mut().workspace_activation_handler = Some(handler);

        Ok(manager)
    }

    fn build_initial_bars(
        manager: &Rc<RefCell<Self>>,
        monitors: &gtk4::gio::ListModel,
    ) -> Result<(), String> {
        if monitor_bindings(monitors).is_empty() {
            return Err("no usable GDK monitors available".to_owned());
        }
        Self::rebuild_for_monitors(manager, monitors)
    }

    pub fn rebuild_for_monitors(
        manager: &Rc<RefCell<Self>>,
        monitors: &gtk4::gio::ListModel,
    ) -> Result<(), String> {
        let bindings = monitor_bindings(monitors);
        let (app, config, providers, web_context) = {
            let state = manager.borrow();
            if state.lifecycle.is_stopped() {
                return Ok(());
            }
            if state.monitors.len() == bindings.len()
                && state.monitors.iter().zip(&bindings).all(|(current, next)| {
                    current.monitor_index == next.monitor_index
                        && current.monitor == next.monitor
                        && current.output == next.output
                })
            {
                return Ok(());
            }
            (
                state.app.clone(),
                state.config.clone(),
                state.providers.clone(),
                state.web_context.clone(),
            )
        };
        Self::retire_tray_menu(manager);

        let mut new_bars = BTreeMap::new();
        for binding in &bindings {
            let context = SurfaceContext::Bar {
                monitor_index: binding.monitor_index,
                output: binding.output.clone(),
            };
            match surface_window::surface_window_for_monitor(
                &app,
                &binding.monitor,
                &config,
                &web_context,
                providers.clone(),
                context,
                Rc::downgrade(manager),
            ) {
                Ok(instance) => {
                    new_bars.insert(instance.key(), instance);
                }
                Err(error) => {
                    close_instances(new_bars);
                    return Err(error);
                }
            }
        }

        let (old_bars, old_transients, old_timers, new_windows) = {
            let mut state = manager.borrow_mut();
            if state.lifecycle.is_stopped() {
                close_instances(new_bars);
                return Ok(());
            }
            let old_bars = std::mem::replace(&mut state.bars, new_bars);
            let (old_transients, old_timers) = state.lifecycle.reset();
            state.monitors = bindings;
            let new_windows = state
                .bars
                .values()
                .map(|instance| instance.window.clone())
                .collect::<Vec<_>>();
            (old_bars, old_transients, old_timers, new_windows)
        };

        remove_timers(old_timers);
        close_instances(old_transients);
        close_instances(old_bars);
        for window in new_windows {
            window.present();
        }
        Ok(())
    }

    pub fn present_bars(manager: &Rc<RefCell<Self>>) {
        let windows = manager
            .borrow()
            .bars
            .values()
            .map(|instance| instance.window.clone())
            .collect::<Vec<_>>();
        for window in windows {
            window.present();
        }
    }

    pub(crate) fn desktop_context(
        manager: &Rc<RefCell<Self>>,
        caller: &SurfaceContext,
        window: &gtk4::ApplicationWindow,
    ) -> Result<
        (
            crate::providers::desktop::ScreenPoint,
            gtk4::gio::AppLaunchContext,
        ),
        SurfaceError,
    > {
        let state = manager.borrow();
        if !state.surface_window_is_current(caller, window) {
            return Err(SurfaceError::Unavailable {
                monitor_index: caller.monitor_index(),
            });
        }
        drop(state);
        let binding = current_binding(manager, caller)?;
        let geometry = binding.monitor.geometry();
        let point = crate::providers::desktop::ScreenPoint {
            x: geometry
                .x()
                .saturating_add(geometry.width())
                .saturating_sub(16),
            y: geometry
                .y()
                .saturating_add(manager.borrow().config.panel_height),
        };
        let context = binding.monitor.display().app_launch_context();
        Ok((point, context.upcast()))
    }

    pub fn set_quick_panel_open(
        manager: &Rc<RefCell<Self>>,
        caller: &SurfaceContext,
        open: bool,
    ) -> Result<bool, SurfaceError> {
        let binding = current_binding(manager, caller)?;
        let key = SurfaceKey::new(SurfaceKind::QuickPanel, binding.monitor_index);
        if !open {
            Self::close_surface(manager, key);
            return Ok(false);
        }
        if manager.borrow().lifecycle.contains(&key) {
            return Ok(true);
        }

        let context = SurfaceContext::QuickPanel {
            monitor_index: binding.monitor_index,
            output: binding.output,
        };
        Self::open_transient(manager, &binding.monitor, context)?;
        Ok(true)
    }

    pub fn set_diagnostics_dialog_open(
        manager: &Rc<RefCell<Self>>,
        caller: &SurfaceContext,
        open: bool,
    ) -> Result<bool, SurfaceError> {
        let binding = current_binding(manager, caller)?;
        let key = SurfaceKey::new(SurfaceKind::DiagnosticsDialog, binding.monitor_index);
        if !open {
            Self::close_surface(manager, key);
            return Ok(false);
        }

        if !manager.borrow().lifecycle.contains(&key) {
            let context = SurfaceContext::DiagnosticsDialog {
                monitor_index: binding.monitor_index,
                output: binding.output,
            };
            Self::open_transient(manager, &binding.monitor, context)?;
        }

        if caller.kind() == SurfaceKind::QuickPanel {
            Self::close_surface(
                manager,
                SurfaceKey::new(SurfaceKind::QuickPanel, binding.monitor_index),
            );
        }
        Ok(true)
    }

    pub(crate) fn surface_window_is_current(
        &self,
        context: &SurfaceContext,
        window: &gtk4::ApplicationWindow,
    ) -> bool {
        if !self.context_is_current(context) {
            return false;
        }
        let instance = if context.kind() == SurfaceKind::Bar {
            self.bars.get(&context.key())
        } else {
            self.lifecycle.transients.get(&context.key())
        };
        instance.is_some_and(|instance| instance.context == *context && instance.window == *window)
    }

    pub(crate) fn surface_view_is_current(
        &self,
        context: &SurfaceContext,
        presentation: &gtk4::Widget,
        view: &webkit6::WebView,
    ) -> bool {
        if !self.context_is_current(context) {
            return false;
        }
        if context.kind() == SurfaceKind::TrayMenu {
            return self.tray_menu.as_ref().is_some_and(|menu| {
                self.tray_menu_generation_is_current(menu.generation)
                    && menu
                        .instance
                        .as_ref()
                        .is_some_and(|instance| instance.is_current(context, presentation, view))
            });
        }
        let instance = if context.kind() == SurfaceKind::Bar {
            self.bars.get(&context.key())
        } else {
            self.lifecycle.transients.get(&context.key())
        };
        instance.is_some_and(|instance| {
            instance.context == *context
                && instance.window.upcast_ref::<gtk4::Widget>() == presentation
                && instance.window.child().as_ref() == Some(view.upcast_ref())
        })
    }

    pub(crate) fn tray_menu_session(
        manager: &Rc<RefCell<Self>>,
        context: &SurfaceContext,
        presentation: &gtk4::Widget,
        view: &webkit6::WebView,
    ) -> Result<TrayMenuSession, NativeMethodError> {
        let state = manager.borrow();
        if !state.surface_view_is_current(context, presentation, view) {
            return Err(NativeMethodError::new(
                "stale_tray_menu",
                "The native tray menu presentation has been retired",
            ));
        }
        let SurfaceContext::TrayMenu { tray_item_id, .. } = context else {
            return Err(NativeMethodError::new(
                "surface_forbidden",
                "This surface is not a native tray menu",
            ));
        };
        let menu = state
            .tray_menu
            .as_ref()
            .filter(|menu| {
                menu.session.item_id() == tray_item_id
                    && menu.caller.monitor_index() == context.monitor_index()
            })
            .ok_or_else(|| {
                NativeMethodError::new("stale_tray_menu", "The native tray menu has been retired")
            })?;
        state.providers.validate_tray_menu(&menu.session)?;
        Ok(menu.session.clone())
    }

    pub(crate) fn show_tray_menu(
        manager: &Rc<RefCell<Self>>,
        caller: &SurfaceContext,
        trigger: NativeMenuTrigger<'_>,
        item_id: String,
        reply: impl FnOnce(Result<serde_json::Value, NativeMethodError>) + 'static,
    ) {
        let caller_window = trigger.window;
        let binding = match current_binding(manager, caller) {
            Ok(binding)
                if manager.borrow().surface_view_is_current(
                    caller,
                    caller_window.upcast_ref(),
                    trigger.web_view,
                ) =>
            {
                binding
            }
            _ => {
                reply(Err(NativeMethodError::new(
                    "surface_unavailable",
                    "The tray menu caller is no longer available",
                )));
                return;
            }
        };
        {
            let mut state = manager.borrow_mut();
            if let Some(menu) = state.tray_menu.as_mut()
                && menu.session.is_active()
                && menu.session.item_id() == item_id
                && menu.caller == *caller
                && menu.caller_window.upgrade().as_ref() == Some(caller_window)
                && menu.caller_view.upgrade().as_ref() == Some(trigger.web_view)
            {
                if menu.geometry_phase == TrayMenuGeometryPhase::Ready {
                    drop(state);
                    reply(Ok(serde_json::Value::Null));
                } else {
                    menu.replies.push(Box::new(reply));
                }
                return;
            }
        }
        Self::retire_tray_menu(manager);
        let (generation, providers) = {
            let mut state = manager.borrow_mut();
            state.tray_menu_generation = state.tray_menu_generation.wrapping_add(1);
            (state.tray_menu_generation, state.providers.clone())
        };
        let weak = Rc::downgrade(manager);
        let session = match providers.bind_tray_menu(&item_id, move || {
            Self::retire_tray_menu_generation(&weak, generation);
        }) {
            Ok(session) => session,
            Err(error) => {
                reply(Err(error));
                return;
            }
        };
        let monitor_handlers = manager
            .borrow()
            .monitors
            .iter()
            .map(|binding| {
                let geometry_manager = Rc::downgrade(manager);
                let geometry = binding.monitor.connect_geometry_notify(move |_| {
                    Self::retire_tray_menu_generation(&geometry_manager, generation);
                });
                let scale_manager = Rc::downgrade(manager);
                let scale = binding.monitor.connect_scale_factor_notify(move |_| {
                    Self::retire_tray_menu_generation(&scale_manager, generation);
                });
                let valid_manager = Rc::downgrade(manager);
                let valid = binding.monitor.connect_valid_notify(move |_| {
                    Self::retire_tray_menu_generation(&valid_manager, generation);
                });
                (binding.monitor.clone(), [geometry, scale, valid])
            })
            .collect();
        manager.borrow_mut().tray_menu = Some(TrayMenuState {
            generation,
            caller: caller.clone(),
            caller_window: caller_window.downgrade(),
            caller_view: trigger.web_view.downgrade(),
            caller_trust: trigger.trust.clone(),
            session: session.clone(),
            replies: vec![Box::new(reply)],
            anchor: None,
            placement: None,
            popup_size: None,
            instance: None,
            bounds: (0, 0),
            geometry_phase: TrayMenuGeometryPhase::AwaitingMenuSize,
            dismiss_windows: Vec::new(),
            suspended_keyboard: Vec::new(),
            fit_timer: None,
            measurement_inflight: false,
            monitor_handlers,
        });
        let weak = Rc::downgrade(manager);
        tray_overlay::capture_anchor(
            trigger,
            &binding.monitor,
            if caller.kind() == SurfaceKind::Bar {
                MenuAnchorSpace::Parent
            } else {
                MenuAnchorSpace::Output
            },
            &item_id,
            session.cancellable(),
            move |anchor| {
                let Some(manager) = weak.upgrade() else {
                    return;
                };
                if !manager.borrow().tray_menu_generation_is_current(generation) {
                    Self::retire_tray_menu_generation(&weak, generation);
                    return;
                }
                let anchor = match anchor {
                    Ok(anchor) => anchor,
                    Err(error) => {
                        Self::fail_tray_menu(&manager, generation, error);
                        return;
                    }
                };
                let (providers, session) = {
                    let mut state = manager.borrow_mut();
                    let providers = state.providers.clone();
                    let menu = state
                        .tray_menu
                        .as_mut()
                        .expect("current tray menu generation");
                    menu.anchor = Some(anchor);
                    (providers, menu.session.clone())
                };
                let request = crate::providers::desktop::DesktopRequest::GetTrayMenu {
                    item_id: session.item_id().to_owned(),
                    menu_item_id: 0,
                };
                let weak = Rc::downgrade(&manager);
                providers.execute_tray_menu(session, request, move |result| {
                    let Some(manager) = weak.upgrade() else {
                        return;
                    };
                    if !manager.borrow().tray_menu_generation_is_current(generation) {
                        Self::retire_tray_menu_generation(&weak, generation);
                        return;
                    }
                    match result {
                        Ok(_) => {
                            if let Err(error) = Self::present_tray_menu(&manager, generation) {
                                Self::fail_tray_menu(&manager, generation, error);
                            }
                        }
                        Err(error) => Self::fail_tray_menu(&manager, generation, error),
                    }
                });
            },
        );
    }

    fn tray_menu_generation_is_current(&self, generation: u64) -> bool {
        self.tray_menu.as_ref().is_some_and(|menu| {
            menu.generation == generation
                && menu.session.is_active()
                && menu
                    .caller_window
                    .upgrade()
                    .zip(menu.caller_view.upgrade())
                    .is_some_and(|(window, view)| {
                        menu.caller_trust.allows_bridge(&view)
                            && self.surface_view_is_current(
                                &menu.caller,
                                window.upcast_ref(),
                                &view,
                            )
                    })
        })
    }

    fn present_tray_menu(
        manager: &Rc<RefCell<Self>>,
        generation: u64,
    ) -> Result<(), NativeMethodError> {
        let (caller, parent, item_id, anchor, bindings, app, config, providers, web_context) = {
            let state = manager.borrow();
            if !state.tray_menu_generation_is_current(generation) {
                return Err(NativeMethodError::new(
                    "stale_tray_menu",
                    "The native tray menu request has been retired",
                ));
            }
            let menu = state
                .tray_menu
                .as_ref()
                .expect("current tray menu generation");
            state.providers.validate_tray_menu(&menu.session)?;
            (
                menu.caller.clone(),
                menu.caller_window.upgrade().expect("current menu caller"),
                menu.session.item_id().to_owned(),
                menu.anchor.expect("captured native anchor"),
                if menu.caller.kind() == SurfaceKind::QuickPanel {
                    state.monitors.clone()
                } else {
                    Vec::new()
                },
                state.app.clone(),
                state.config.clone(),
                state.providers.clone(),
                state.web_context.clone(),
            )
        };
        let binding = current_binding(manager, &caller)
            .map_err(|error| NativeMethodError::new(error.code(), error.message()))?;
        let context = SurfaceContext::TrayMenu {
            monitor_index: binding.monitor_index,
            output: binding.output.clone(),
            tray_item_id: item_id,
        };
        let geometry = binding.monitor.geometry();
        let bounds = (geometry.width(), geometry.height());
        let parent_relative = caller.kind() == SurfaceKind::Bar;
        let popup_size = parent_relative.then(|| {
            tray_overlay::popover_size(bounds.0, bounds.1, tray_overlay::INITIAL_MENU_HEIGHT)
        });
        let instance = if let Some(size) = popup_size {
            surface_window::tray_popover_for_parent(
                &parent,
                &web_context,
                providers,
                context,
                anchor,
                size,
                Rc::downgrade(manager),
            )
            .map_err(|message| NativeMethodError::new("surface_failed", message))?
        } else {
            let instance = surface_window::surface_window_for_monitor(
                &app,
                &binding.monitor,
                &config,
                &web_context,
                providers,
                context,
                Rc::downgrade(manager),
            )
            .map_err(|message| NativeMethodError::new("surface_failed", message))?;
            let window = instance.window;
            let view = window
                .child()
                .and_downcast::<webkit6::WebView>()
                .expect("trusted menu view");
            window.set_keyboard_mode(KeyboardMode::None);
            view.set_visible(false);
            // Preserve the Quick Panel's existing zero-zone top/right coordinate domain.
            // Fixed GTK size hints would clamp the all-anchored native workarea probe.
            window.set_resizable(true);
            window.set_exclusive_zone(0);
            for edge in [
                gtk4_layer_shell::Edge::Left,
                gtk4_layer_shell::Edge::Right,
                gtk4_layer_shell::Edge::Top,
                gtk4_layer_shell::Edge::Bottom,
            ] {
                window.set_anchor(edge, true);
                window.set_margin(edge, 0);
            }
            window.set_default_size(0, 0);
            window.connect_realize(|window| {
                if let Some(surface) = window.surface() {
                    surface.set_input_region(Some(&gtk4::cairo::Region::create()));
                }
            });
            TrayMenuInstance {
                context: instance.context,
                presentation: TrayMenuPresentation::LayerWindow(window),
                web_view: view,
                trust: instance.trust,
            }
        };
        let popup = match &instance.presentation {
            TrayMenuPresentation::ParentPopup { popover, .. } => Some(popover.clone()),
            TrayMenuPresentation::LayerWindow(_) => None,
        };
        let dismiss_windows = if parent_relative {
            Vec::new()
        } else {
            bindings
                .iter()
                .map(|binding| {
                    let weak = Rc::downgrade(manager);
                    tray_overlay::dismiss_window(
                        &app,
                        &binding.monitor,
                        binding.monitor_index,
                        move || {
                            Self::retire_tray_menu_generation(&weak, generation);
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        let rejected = {
            let mut state = manager.borrow_mut();
            if state.tray_menu_generation_is_current(generation)
                && state.binding_is_current(&binding)
                && state
                    .tray_menu
                    .as_ref()
                    .is_some_and(|menu| menu.instance.is_none())
            {
                let suspended = if parent_relative {
                    Vec::new()
                } else {
                    state
                        .bars
                        .values()
                        .chain(state.lifecycle.transients.values())
                        .map(|instance| {
                            (instance.window.downgrade(), instance.window.keyboard_mode())
                        })
                        .collect::<Vec<_>>()
                };
                let menu = state
                    .tray_menu
                    .as_mut()
                    .expect("current tray menu generation");
                menu.instance = Some(instance);
                menu.popup_size = popup_size;
                menu.bounds = bounds;
                menu.geometry_phase = if parent_relative {
                    TrayMenuGeometryPhase::AwaitingMenuSize
                } else {
                    TrayMenuGeometryPhase::AwaitingCaptures
                };
                menu.dismiss_windows = dismiss_windows.clone();
                menu.suspended_keyboard = suspended;
                None
            } else {
                Some(instance)
            }
        };
        if let Some(instance) = rejected {
            for capture in dismiss_windows {
                capture.destroy();
            }
            instance.presentation.retire();
            return Err(NativeMethodError::new(
                "stale_tray_menu",
                "The native tray menu request has been retired",
            ));
        }
        let suspended = manager
            .borrow()
            .tray_menu
            .as_ref()
            .expect("current tray menu generation")
            .suspended_keyboard
            .iter()
            .filter_map(|(window, _)| window.upgrade())
            .collect::<Vec<_>>();
        for window in suspended {
            window.set_keyboard_mode(KeyboardMode::None);
        }
        if let Some(popover) = popup {
            // Ownership precedes mapping and the first trusted-document method.
            popover.popup();
        } else {
            // Map every Quick capture before its invisible workarea probe.
            for capture in dismiss_windows {
                capture.present();
            }
        }
        let weak = Rc::downgrade(manager);
        let fit_timer = glib::timeout_add_local(TRAY_MENU_FIT_INTERVAL, move || {
            let Some(manager) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            Self::fit_tray_menu(&manager, generation);
            glib::ControlFlow::Continue
        });
        let mut state = manager.borrow_mut();
        if let Some(menu) = state
            .tray_menu
            .as_mut()
            .filter(|menu| menu.generation == generation)
        {
            menu.fit_timer = Some(fit_timer);
        } else {
            fit_timer.remove();
        }
        Ok(())
    }

    fn advance_tray_menu_geometry(manager: &Rc<RefCell<Self>>, generation: u64) -> bool {
        let (window, view, phase, anchor, bounds, placement) = {
            let state = manager.borrow();
            if !state.tray_menu_generation_is_current(generation) {
                return false;
            }
            let menu = state
                .tray_menu
                .as_ref()
                .expect("current tray menu generation");
            if menu.geometry_phase == TrayMenuGeometryPhase::Ready {
                return true;
            }
            let Some(instance) = menu.instance.as_ref() else {
                return false;
            };
            let Some(window) = instance.presentation.layer_window() else {
                drop(state);
                return Self::advance_tray_popover_geometry(manager, generation);
            };
            let view = instance.web_view.clone();
            if menu.geometry_phase == TrayMenuGeometryPhase::AwaitingCaptures {
                if menu
                    .dismiss_windows
                    .iter()
                    .any(|window| !window.is_mapped())
                {
                    return false;
                }
                let window = window.clone();
                drop(state);
                manager
                    .borrow_mut()
                    .tray_menu
                    .as_mut()
                    .expect("current tray menu")
                    .geometry_phase = TrayMenuGeometryPhase::AwaitingWorkarea;
                window.present();
                return false;
            }
            (
                window.clone(),
                view,
                menu.geometry_phase,
                menu.anchor.expect("captured anchor"),
                menu.bounds,
                menu.placement,
            )
        };
        let Some(surface) = window.surface().filter(|_| window.is_mapped()) else {
            return false;
        };
        let size = (surface.width(), surface.height());
        if size.0 <= 0 || size.1 <= 0 || window.width() <= 0 || window.height() <= 0 {
            return false;
        }
        if !surface.display().supports_input_shapes() {
            Self::fail_tray_menu(
                manager,
                generation,
                NativeMethodError::new(
                    "surface_failed",
                    "GDK does not support native tray menu input regions",
                ),
            );
            return false;
        }
        if phase == TrayMenuGeometryPhase::AwaitingWorkarea {
            let anchor = tray_overlay::anchor_in_workarea(anchor, bounds.0, size.0, size.1);
            let Some(anchor) = anchor.filter(|_| size.0 <= bounds.0 && size.1 <= bounds.1) else {
                Self::fail_tray_menu(
                    manager,
                    generation,
                    NativeMethodError::new(
                        "tray_anchor_unavailable",
                        "The tray control is outside the current native workarea",
                    ),
                );
                return false;
            };
            let placement = tray_overlay::menu_placement(
                anchor,
                size.0,
                size.1,
                tray_overlay::INITIAL_MENU_HEIGHT,
            );
            {
                let mut state = manager.borrow_mut();
                let Some(menu) = state
                    .tray_menu
                    .as_mut()
                    .filter(|menu| menu.generation == generation)
                else {
                    return false;
                };
                menu.anchor = Some(anchor);
                menu.bounds = size;
                menu.placement = Some(placement);
                menu.geometry_phase = TrayMenuGeometryPhase::AwaitingMenuSize;
            }
            window.set_resizable(false);
            tray_overlay::apply_placement(&window, placement, size.0);
            return false;
        }
        let placement = placement.expect("configured menu placement");
        if size != (placement.width, placement.height) {
            return false;
        }
        let replies = {
            let mut state = manager.borrow_mut();
            let Some(menu) = state
                .tray_menu
                .as_mut()
                .filter(|menu| menu.generation == generation)
            else {
                return false;
            };
            menu.geometry_phase = TrayMenuGeometryPhase::Ready;
            std::mem::take(&mut menu.replies)
        };
        // The invisible probe was mapped after every capture layer and never unmapped.
        // Only now does the final narrow menu accept input and load its managed immutable context.
        surface.set_input_region(None);
        view.set_visible(true);
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        view.load_uri(crate::web_security::TRUSTED_ENTRY_URI);
        view.grab_focus();
        for reply in replies {
            reply(Ok(serde_json::Value::Null));
        }
        true
    }

    fn advance_tray_popover_geometry(manager: &Rc<RefCell<Self>>, generation: u64) -> bool {
        let (popover, view, parent, bounds) = {
            let state = manager.borrow();
            if !state.tray_menu_generation_is_current(generation) {
                return false;
            }
            let menu = state.tray_menu.as_ref().expect("current tray menu");
            let Some(instance) = menu.instance.as_ref() else {
                return false;
            };
            let TrayMenuPresentation::ParentPopup { popover, .. } = &instance.presentation else {
                return false;
            };
            (
                popover.clone(),
                instance.web_view.clone(),
                menu.caller_window.upgrade().expect("current menu caller"),
                menu.bounds,
            )
        };
        let Some(surface) = popover.surface().filter(|_| popover.is_mapped()) else {
            return false;
        };
        if surface
            .dynamic_cast_ref::<gtk4::gdk::Popup>()
            .is_none_or(|popup| !popup.is_autohide() || popup.parent() != parent.surface())
            || surface.width() > bounds.0
            || surface.height() > bounds.1
        {
            Self::fail_tray_menu(
                manager,
                generation,
                NativeMethodError::new(
                    "surface_failed",
                    "The Bar tray menu requires a bounded native parent popup with an input grab",
                ),
            );
            return false;
        }
        if surface.width() <= 0 || surface.height() <= 0 || view.width() <= 0 || view.height() <= 0
        {
            return false;
        }
        let replies = {
            let mut state = manager.borrow_mut();
            let Some(menu) = state
                .tray_menu
                .as_mut()
                .filter(|menu| menu.generation == generation)
            else {
                return false;
            };
            menu.geometry_phase = TrayMenuGeometryPhase::Ready;
            std::mem::take(&mut menu.replies)
        };
        view.load_uri(crate::web_security::TRUSTED_ENTRY_URI);
        view.grab_focus();
        for reply in replies {
            reply(Ok(serde_json::Value::Null));
        }
        true
    }

    fn fit_tray_menu(manager: &Rc<RefCell<Self>>, generation: u64) {
        let caller_lost = {
            let state = manager.borrow();
            state
                .tray_menu
                .as_ref()
                .is_some_and(|menu| menu.generation == generation)
                && !state.tray_menu_generation_is_current(generation)
        };
        if caller_lost {
            Self::retire_tray_menu(manager);
            return;
        }
        if !Self::advance_tray_menu_geometry(manager, generation) {
            return;
        }
        let (context, presentation, view, trust, session, anchor, bounds) = {
            let mut state = manager.borrow_mut();
            if !state.tray_menu_generation_is_current(generation) {
                return;
            }
            let menu = state
                .tray_menu
                .as_mut()
                .expect("current tray menu generation");
            if menu.measurement_inflight {
                return;
            }
            let Some(instance) = menu.instance.as_ref() else {
                return;
            };
            if !instance.trust.allows_bridge(&instance.web_view) {
                return;
            }
            let resources = (
                instance.context.clone(),
                instance.presentation.clone(),
                instance.web_view.clone(),
                instance.trust.clone(),
                menu.session.clone(),
                menu.anchor.expect("captured native anchor"),
                menu.bounds,
            );
            menu.measurement_inflight = true;
            resources
        };
        let weak = Rc::downgrade(manager);
        let response_view = view.clone();
        tray_overlay::measure_content(&view, trust, session.cancellable(), move |height| {
            let Some(manager) = weak.upgrade() else {
                return;
            };
            let mut state = manager.borrow_mut();
            if !state.tray_menu_generation_is_current(generation)
                || !state.surface_view_is_current(&context, presentation.widget(), &response_view)
            {
                return;
            }
            let menu = state.tray_menu.as_mut().expect("current menu");
            menu.measurement_inflight = false;
            let Some(height) = height else {
                return;
            };
            match presentation {
                TrayMenuPresentation::LayerWindow(window) => {
                    let placement =
                        tray_overlay::menu_placement(anchor, bounds.0, bounds.1, height);
                    if menu.placement == Some(placement) {
                        return;
                    }
                    menu.placement = Some(placement);
                    drop(state);
                    tray_overlay::apply_placement(&window, placement, bounds.0);
                }
                TrayMenuPresentation::ParentPopup { popover, content } => {
                    let size = tray_overlay::popover_size(bounds.0, bounds.1, height);
                    if menu.popup_size == Some(size) {
                        return;
                    }
                    menu.popup_size = Some(size);
                    drop(state);
                    tray_overlay::apply_popover_size(&popover, &content, size);
                }
            }
        });
    }

    fn fail_tray_menu(manager: &Rc<RefCell<Self>>, generation: u64, error: NativeMethodError) {
        let replies = {
            let mut state = manager.borrow_mut();
            let Some(menu) = state
                .tray_menu
                .as_mut()
                .filter(|menu| menu.generation == generation)
            else {
                return;
            };
            std::mem::take(&mut menu.replies)
        };
        Self::retire_tray_menu(manager);
        for reply in replies {
            reply(Err(NativeMethodError::new(
                error.code,
                error.message.clone(),
            )));
        }
    }

    pub(crate) fn retire_tray_menu_generation(manager: &Weak<RefCell<Self>>, generation: u64) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        if manager
            .borrow()
            .tray_menu
            .as_ref()
            .is_some_and(|menu| menu.generation == generation)
        {
            Self::retire_tray_menu(&manager);
        }
    }

    pub(crate) fn retire_tray_menu_view(
        manager: &Weak<RefCell<Self>>,
        context: &SurfaceContext,
        presentation: &gtk4::Widget,
        view: &webkit6::WebView,
    ) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        if context.kind() == SurfaceKind::TrayMenu
            && manager
                .borrow()
                .surface_view_is_current(context, presentation, view)
        {
            Self::retire_tray_menu(&manager);
        }
    }

    pub(crate) fn retire_tray_menu_presentation(
        manager: &Weak<RefCell<Self>>,
        context: &SurfaceContext,
        presentation: &gtk4::Widget,
    ) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        let current = manager.borrow().tray_menu.as_ref().is_some_and(|menu| {
            menu.instance.as_ref().is_some_and(|instance| {
                instance.context == *context && instance.presentation.widget() == presentation
            })
        });
        if current {
            Self::retire_tray_menu(&manager);
        }
    }

    fn retire_tray_menu(manager: &Rc<RefCell<Self>>) {
        let menu = {
            let mut state = manager.borrow_mut();
            let Some(menu) = state.tray_menu.take() else {
                return;
            };
            menu
        };
        menu.session.retire();
        if let Some(timer) = menu.fit_timer {
            timer.remove();
        }
        for (monitor, handlers) in menu.monitor_handlers {
            for handler in handlers {
                monitor.disconnect(handler);
            }
        }
        for (window, mode) in menu.suspended_keyboard {
            if let Some(window) = window.upgrade() {
                window.set_keyboard_mode(mode);
            }
        }
        for capture in menu.dismiss_windows {
            capture.destroy();
        }
        if let Some(instance) = menu.instance {
            instance.presentation.retire();
        }
        for reply in menu.replies {
            reply(Err(NativeMethodError::new(
                "stale_tray_menu",
                "The native tray menu request was dismissed",
            )));
        }
    }

    pub(crate) fn menu_suspends_focus_loss(
        manager: &Weak<RefCell<Self>>,
        window: &gtk4::ApplicationWindow,
    ) -> bool {
        manager.upgrade().is_some_and(|manager| {
            manager.borrow().tray_menu.as_ref().is_some_and(|menu| {
                menu.suspended_keyboard
                    .iter()
                    .any(|(suspended, _)| suspended.upgrade().as_ref() == Some(window))
            })
        })
    }

    fn retire_tray_menu_for_owner(manager: &Rc<RefCell<Self>>, window: &gtk4::ApplicationWindow) {
        let owned = manager
            .borrow()
            .tray_menu
            .as_ref()
            .is_some_and(|menu| menu.caller_window.upgrade().as_ref() == Some(window));
        if owned {
            Self::retire_tray_menu(manager);
        }
    }
    fn open_transient(
        manager: &Rc<RefCell<Self>>,
        monitor: &gtk4::gdk::Monitor,
        context: SurfaceContext,
    ) -> Result<(), SurfaceError> {
        let key = context.key();
        let (app, config, providers, web_context) = construction_dependencies(manager)?;
        let instance = surface_window::surface_window_for_monitor(
            &app,
            monitor,
            &config,
            &web_context,
            providers,
            context.clone(),
            Rc::downgrade(manager),
        )
        .map_err(SurfaceError::Failed)?;
        let window = instance.window.clone();

        let inserted = {
            let mut state = manager.borrow_mut();
            let current = state.context_is_current(&context);
            state
                .lifecycle
                .insert_if_current(key, instance, current)
                .is_ok()
        };
        if inserted {
            let suspend = {
                let mut state = manager.borrow_mut();
                if let Some(menu) = state
                    .tray_menu
                    .as_mut()
                    .filter(|menu| !menu.dismiss_windows.is_empty())
                {
                    menu.suspended_keyboard
                        .push((window.downgrade(), window.keyboard_mode()));
                    true
                } else {
                    false
                }
            };
            if suspend {
                window.set_keyboard_mode(KeyboardMode::None);
            }
            window.present();
            Ok(())
        } else {
            window.destroy();
            if manager.borrow().context_is_current(&context) {
                Ok(())
            } else {
                Err(SurfaceError::Unavailable {
                    monitor_index: context.monitor_index(),
                })
            }
        }
    }

    pub fn show_workspace_hud(
        manager: &Rc<RefCell<Self>>,
        activation: WorkspaceActivation,
    ) -> Result<(), SurfaceError> {
        let (binding, generation, previous, previous_timer, app, config, providers, web_context) = {
            let mut state = manager.borrow_mut();
            if state.lifecycle.is_stopped() {
                return Ok(());
            }
            let Some(binding) = state
                .monitors
                .iter()
                .find(|binding| binding.output.as_deref() == Some(activation.output.as_str()))
                .cloned()
            else {
                return Ok(());
            };
            let Some((generation, previous, previous_timer)) =
                state.lifecycle.begin_hud(binding.monitor_index)
            else {
                return Ok(());
            };
            (
                binding,
                generation,
                previous,
                previous_timer,
                state.app.clone(),
                state.config.clone(),
                state.providers.clone(),
                state.web_context.clone(),
            )
        };

        if let Some(timer) = previous_timer {
            timer.remove();
        }
        if let Some(instance) = previous {
            instance.window.destroy();
        }

        let context = SurfaceContext::WorkspaceHud {
            monitor_index: binding.monitor_index,
            output: binding.output.clone(),
            workspace_index: activation.workspace_index,
        };
        let instance = surface_window::surface_window_for_monitor(
            &app,
            &binding.monitor,
            &config,
            &web_context,
            providers,
            context,
            Rc::downgrade(manager),
        )
        .map_err(SurfaceError::Failed)?;
        let key = instance.key();
        let window = instance.window.clone();

        let inserted = {
            let mut state = manager.borrow_mut();
            let current = state.binding_is_current(&binding)
                && state
                    .lifecycle
                    .hud_is_current(binding.monitor_index, generation);
            state
                .lifecycle
                .insert_if_current(key, instance, current)
                .is_ok()
        };
        if !inserted {
            window.destroy();
            return Ok(());
        }

        window.present();
        let weak = Rc::downgrade(manager);
        let monitor_index = binding.monitor_index;
        let source = glib::timeout_add_local_once(WORKSPACE_HUD_DURATION, move || {
            Self::hud_timeout(&weak, monitor_index, generation);
        });
        let mut state = manager.borrow_mut();
        if let Err(source) = state
            .lifecycle
            .install_hud_timer(monitor_index, generation, source)
        {
            source.remove();
        }
        Ok(())
    }

    fn hud_timeout(manager: &Weak<RefCell<Self>>, monitor_index: u32, generation: u64) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        let instance = manager
            .borrow_mut()
            .lifecycle
            .hud_timeout(monitor_index, generation);
        if let Some(instance) = instance {
            instance.window.destroy();
        }
    }

    pub fn close_surface_window_from_weak(
        manager: &Weak<RefCell<Self>>,
        key: SurfaceKey,
        window: &gtk4::ApplicationWindow,
    ) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        Self::retire_tray_menu_for_owner(&manager, window);
        if key.kind == SurfaceKind::TrayMenu {
            let current = manager.borrow().tray_menu.as_ref().is_some_and(|menu| {
                menu.instance.as_ref().is_some_and(|instance| {
                    instance.context.key() == key
                        && instance.presentation.layer_window() == Some(window)
                })
            });
            if current {
                Self::retire_tray_menu(&manager);
            }
            return;
        }
        let Some((instance, timer)) = manager
            .borrow_mut()
            .lifecycle
            .remove_if(key, |instance| instance.window == *window)
        else {
            return;
        };
        if let Some(timer) = timer {
            timer.remove();
        }
        instance.window.destroy();
    }

    pub fn close_surface(manager: &Rc<RefCell<Self>>, key: SurfaceKey) {
        if key.kind == SurfaceKind::TrayMenu {
            Self::retire_tray_menu(manager);
            return;
        }
        let (instance, timer) = manager.borrow_mut().lifecycle.remove(key);
        if let Some(timer) = timer {
            timer.remove();
        }
        if let Some(instance) = instance {
            Self::retire_tray_menu_for_owner(manager, &instance.window);
            instance.window.destroy();
        }
    }

    pub fn surface_closed(
        manager: &Weak<RefCell<Self>>,
        key: SurfaceKey,
        window: &gtk4::ApplicationWindow,
    ) {
        let Some(manager) = manager.upgrade() else {
            return;
        };
        Self::retire_tray_menu_for_owner(&manager, window);
        if key.kind == SurfaceKind::TrayMenu {
            let current = manager.borrow().tray_menu.as_ref().is_some_and(|menu| {
                menu.instance.as_ref().is_some_and(|instance| {
                    instance.context.key() == key
                        && instance.presentation.layer_window() == Some(window)
                })
            });
            if current {
                Self::retire_tray_menu(&manager);
            }
            return;
        }
        let Some((_instance, timer)) = manager
            .borrow_mut()
            .lifecycle
            .remove_if(key, |instance| instance.window == *window)
        else {
            return;
        };
        if let Some(timer) = timer {
            timer.remove();
        }
    }

    pub fn shutdown(manager: &Rc<RefCell<Self>>) {
        Self::retire_tray_menu(manager);
        let (providers, handler, bars, transients, timers) = {
            let mut state = manager.borrow_mut();
            let Some((transients, timers)) = state.lifecycle.stop() else {
                return;
            };
            (
                state.providers.clone(),
                state.workspace_activation_handler.take(),
                std::mem::take(&mut state.bars),
                transients,
                timers,
            )
        };

        if let Some(handler) = handler {
            providers.disconnect_workspace_activation(handler);
        }
        remove_timers(timers);
        close_instances(transients);
        close_instances(bars);
        providers.shutdown();
    }

    fn context_is_current(&self, context: &SurfaceContext) -> bool {
        !self.lifecycle.is_stopped()
            && self.monitors.iter().any(|binding| {
                binding.monitor_index == context.monitor_index()
                    && binding.output.as_deref() == context.output()
                    && binding.monitor.is_valid()
            })
    }

    fn binding_is_current(&self, expected: &MonitorBinding) -> bool {
        !self.lifecycle.is_stopped()
            && self.monitors.iter().any(|binding| {
                binding.monitor_index == expected.monitor_index
                    && binding.output == expected.output
                    && binding.monitor == expected.monitor
                    && binding.monitor.is_valid()
            })
    }
}

fn current_binding(
    manager: &Rc<RefCell<SurfaceManager>>,
    caller: &SurfaceContext,
) -> Result<MonitorBinding, SurfaceError> {
    let state = manager.borrow();
    state
        .monitors
        .iter()
        .find(|binding| {
            binding.monitor_index == caller.monitor_index()
                && binding.output.as_deref() == caller.output()
                && binding.monitor.is_valid()
        })
        .cloned()
        .ok_or(SurfaceError::Unavailable {
            monitor_index: caller.monitor_index(),
        })
}

fn construction_dependencies(
    manager: &Rc<RefCell<SurfaceManager>>,
) -> Result<
    (
        gtk4::Application,
        ShellConfig,
        ProviderRegistry,
        TrustedWebContext,
    ),
    SurfaceError,
> {
    let state = manager.borrow();
    if state.lifecycle.is_stopped() {
        return Err(SurfaceError::Failed(
            "surface manager is shutting down".to_owned(),
        ));
    }
    Ok((
        state.app.clone(),
        state.config.clone(),
        state.providers.clone(),
        state.web_context.clone(),
    ))
}

fn monitor_bindings(monitors: &gtk4::gio::ListModel) -> Vec<MonitorBinding> {
    collect_indexed(monitors.n_items(), |monitor_index| {
        let Some(monitor) = monitors
            .item(monitor_index)
            .and_downcast::<gtk4::gdk::Monitor>()
        else {
            eprintln!("skipping non-monitor GDK list item at index {monitor_index}");
            return None;
        };
        if !monitor.is_valid() {
            eprintln!("skipping invalid GDK monitor at index {monitor_index}");
            return None;
        }
        Some(MonitorBinding {
            monitor_index,
            output: monitor.connector().map(|connector| connector.to_string()),
            monitor,
        })
    })
}

fn collect_indexed<T>(item_count: u32, mut item: impl FnMut(u32) -> Option<T>) -> Vec<T> {
    (0..item_count).filter_map(&mut item).collect()
}

fn close_instances(instances: BTreeMap<SurfaceKey, SurfaceInstance>) {
    for instance in instances.into_values() {
        instance.window.destroy();
    }
}

fn remove_timers(timers: BTreeMap<u32, HudTimer<SourceId>>) {
    for timer in timers.into_values() {
        timer.value.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(kind: SurfaceKind, monitor_index: u32) -> SurfaceKey {
        SurfaceKey::new(kind, monitor_index)
    }

    #[test]
    fn duplicate_insert_preserves_existing_instance() {
        let quick_panel = key(SurfaceKind::QuickPanel, 2);
        let mut registry = LifecycleRegistry::<&str, u8>::new();

        assert_eq!(
            registry.insert_if_current(quick_panel, "first", true),
            Ok(())
        );
        assert_eq!(
            registry.insert_if_current(quick_panel, "duplicate", true),
            Err("duplicate")
        );
        assert_eq!(registry.remove(quick_panel), (Some("first"), None));
    }

    #[test]
    fn close_is_idempotent_and_stale_instance_token_is_rejected() {
        let surface = key(SurfaceKind::QuickPanel, 0);
        let mut registry = LifecycleRegistry::<u64, u8>::new();
        assert_eq!(registry.insert_if_current(surface, 41, true), Ok(()));

        assert_eq!(registry.remove_if(surface, |token| *token == 40), None);
        assert!(registry.contains(&surface));
        assert_eq!(
            registry.remove_if(surface, |token| *token == 41),
            Some((41, None))
        );
        assert_eq!(registry.remove(surface), (None, None));
    }

    #[test]
    fn stale_context_insert_is_rejected_without_replacing_current_instance() {
        let dialog = key(SurfaceKind::DiagnosticsDialog, 3);
        let mut registry = LifecycleRegistry::<&str, u8>::new();
        assert_eq!(registry.insert_if_current(dialog, "current", true), Ok(()));

        assert_eq!(
            registry.insert_if_current(dialog, "stale", false),
            Err("stale")
        );
        assert_eq!(registry.remove(dialog), (Some("current"), None));
    }

    #[test]
    fn hud_replacement_advances_generation_and_rejects_stale_timer() {
        let hud = key(SurfaceKind::WorkspaceHud, 4);
        let mut registry = LifecycleRegistry::<&str, &'static str>::new();
        let (first_generation, previous, timer) = registry.begin_hud(4).unwrap();
        assert_eq!((first_generation, previous, timer), (1, None, None));
        assert_eq!(registry.insert_if_current(hud, "first", true), Ok(()));
        assert_eq!(
            registry.install_hud_timer(4, first_generation, "first timer"),
            Ok(())
        );

        let (second_generation, previous, timer) = registry.begin_hud(4).unwrap();
        assert_eq!(second_generation, 2);
        assert_eq!(previous, Some("first"));
        assert_eq!(timer, Some("first timer"));
        assert_eq!(registry.insert_if_current(hud, "second", true), Ok(()));
        assert_eq!(
            registry.install_hud_timer(4, first_generation, "stale timer"),
            Err("stale timer")
        );
        assert_eq!(
            registry.install_hud_timer(4, second_generation, "second timer"),
            Ok(())
        );
        assert_eq!(registry.hud_timeout(4, first_generation), None);
        assert!(registry.contains(&hud));
        assert_eq!(registry.hud_timeout(4, second_generation), Some("second"));
        assert!(!registry.contains(&hud));
    }

    #[test]
    fn topology_reset_clears_transients_timers_and_generations() {
        let hud = key(SurfaceKind::WorkspaceHud, 1);
        let dialog = key(SurfaceKind::DiagnosticsDialog, 1);
        let mut registry = LifecycleRegistry::<&str, &str>::new();
        let (generation, _, _) = registry.begin_hud(1).unwrap();
        registry.insert_if_current(hud, "hud", true).unwrap();
        registry.install_hud_timer(1, generation, "timer").unwrap();
        registry.insert_if_current(dialog, "dialog", true).unwrap();

        let (transients, timers) = registry.reset();
        assert_eq!(transients.len(), 2);
        assert_eq!(timers.len(), 1);
        assert!(!registry.contains(&hud));
        assert_eq!(registry.hud_timeout(1, generation), None);
        assert_eq!(registry.begin_hud(1).unwrap().0, 1);
    }

    #[test]
    fn stop_is_idempotent_and_rejects_future_work() {
        let quick_panel = key(SurfaceKind::QuickPanel, 0);
        let mut registry = LifecycleRegistry::<&str, u8>::new();
        registry
            .insert_if_current(quick_panel, "panel", true)
            .unwrap();

        let (transients, timers) = registry.stop().unwrap();
        assert_eq!(transients.get(&quick_panel), Some(&"panel"));
        assert!(timers.is_empty());
        assert!(registry.is_stopped());
        assert!(registry.stop().is_none());
        assert_eq!(
            registry.insert_if_current(quick_panel, "late", true),
            Err("late")
        );
        assert!(registry.begin_hud(0).is_none());
    }

    #[test]
    fn indexed_collection_handles_empty_and_preserves_indices_across_skips() {
        let empty = collect_indexed::<u32>(0, Some);
        assert!(empty.is_empty());

        let retained = collect_indexed(5, |index| (index != 1 && index != 3).then_some(index));
        assert_eq!(retained, vec![0, 2, 4]);
    }
}
