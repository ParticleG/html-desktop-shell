use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::Duration,
};

use glib::SourceId;
use gtk4::{gio::prelude::ListModelExt, prelude::*};

use crate::{
    config::ShellConfig,
    providers::{ProviderRegistry, WorkspaceActivation, WorkspaceActivationHandlerId},
    surface::{SurfaceContext, SurfaceError, SurfaceKey, SurfaceKind},
    surface_window::{self, SurfaceInstance},
    web_security::TrustedWebContext,
};

const WORKSPACE_HUD_DURATION: Duration = Duration::from_millis(1500);

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
            (
                state.app.clone(),
                state.config.clone(),
                state.providers.clone(),
                state.web_context.clone(),
            )
        };

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
        let (instance, timer) = manager.borrow_mut().lifecycle.remove(key);
        if let Some(timer) = timer {
            timer.remove();
        }
        if let Some(instance) = instance {
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
        let quick_panel = key(SurfaceKind::QuickPanel, 0);
        let mut registry = LifecycleRegistry::<u64, u8>::new();
        assert_eq!(registry.insert_if_current(quick_panel, 41, true), Ok(()));

        assert_eq!(registry.remove_if(quick_panel, |token| *token == 40), None);
        assert!(registry.contains(&quick_panel));
        assert_eq!(
            registry.remove_if(quick_panel, |token| *token == 41),
            Some((41, None))
        );
        assert_eq!(registry.remove(quick_panel), (None, None));
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
