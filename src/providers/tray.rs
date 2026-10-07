use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use gio::prelude::*;
use glib::variant::ToVariant;
use gtk4::gio;
use serde::Serialize;
use serde_json::{Value, json};

use super::desktop::{ScreenPoint, TrayOrientation};
use super::icons::{self, Icon};
use crate::messages::NativeMethodError;

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const BUS_NAME: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const CALL_TIMEOUT: i32 = 5_000;
const ICON_SIZE: i32 = 24;
const MAX_MENU_NODES: usize = 4096;
const MAX_MENU_DEPTH: usize = 32;
const MAX_TEXT_CHARS: usize = 4096;
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

const WATCHER_XML: &str = r#"<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
    <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
    <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
    <signal name="StatusNotifierHostRegistered"/>
    <signal name="StatusNotifierHostUnregistered"/>
  </interface>
</node>"#;
const HOST_XML: &str = r#"<node><interface name="org.kde.StatusNotifierHost"/></node>"#;

type Properties = HashMap<String, glib::Variant>;

#[derive(Clone)]
pub(crate) struct TrayController {
    state: Rc<RefCell<State>>,
}

struct State {
    stopped: bool,
    available: bool,
    reason: &'static str,
    instance: u64,
    serial: u64,
    next_item: u64,
    startup_cancel: gio::Cancellable,
    host_name: String,
    host_acquired: bool,
    bus: Option<BusResources>,
    watcher: Option<WatcherSession>,
    owned_watcher: Option<OwnedWatcher>,
    items: BTreeMap<String, Item>,
    bindings: BTreeMap<Binding, String>,
}

struct BusResources {
    connection: gio::DBusConnection,
    watcher_registration: Option<gio::RegistrationId>,
    host_registration: Option<gio::RegistrationId>,
    watcher_name: Option<gio::OwnerId>,
    host_name: Option<gio::OwnerId>,
    unwatch_watcher: Option<Box<dyn FnOnce()>>,
    owner_subscription: Option<gio::SignalSubscription>,
    closed_handler: Option<glib::SignalHandlerId>,
}

impl Drop for BusResources {
    fn drop(&mut self) {
        if let Some(handler) = self.closed_handler.take() {
            self.connection.disconnect(handler);
        }
        self.owner_subscription.take();
        if let Some(unwatch) = self.unwatch_watcher.take() {
            unwatch();
        }
        if let Some(owner) = self.host_name.take() {
            gio::bus_unown_name(owner);
        }
        if let Some(owner) = self.watcher_name.take() {
            gio::bus_unown_name(owner);
        }
        if let Some(registration) = self.host_registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
        if let Some(registration) = self.watcher_registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
    }
}

struct WatcherSession {
    owner: String,
    epoch: u64,
    cancellable: gio::Cancellable,
    subscriptions: Vec<gio::SignalSubscription>,
    desired: BTreeMap<String, DesiredItem>,
    joining: bool,
    joined: bool,
    list_inflight: bool,
    list_again: bool,
    list_serial: u64,
}

impl Drop for WatcherSession {
    fn drop(&mut self) {
        self.cancellable.cancel();
    }
}

#[derive(Default)]
struct OwnedWatcher {
    items: BTreeMap<String, WatcherRecord>,
    hosts: BTreeMap<String, String>,
    pending_hosts: BTreeMap<String, BTreeSet<u64>>,
    pending_items: BTreeMap<u64, PendingRegistration>,
}

struct WatcherRecord {
    spec: RegistrationSpec,
    owner: String,
}

struct PendingRegistration {
    spec: RegistrationSpec,
    sender: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RegistrationSpec {
    destination: String,
    path: String,
}

impl RegistrationSpec {
    fn parse(service: &str, sender: Option<&str>) -> Option<Self> {
        let (destination, path) = if service.starts_with('/') {
            (sender?, service)
        } else if let Some(index) = service.find('/') {
            (&service[..index], &service[index..])
        } else {
            (service, ITEM_PATH)
        };
        if !gio::dbus_is_name(destination) || !glib::Variant::is_object_path(path) {
            return None;
        }
        Some(Self {
            destination: destination.into(),
            path: path.into(),
        })
    }

    fn canonical(&self) -> String {
        format!("{}{}", self.destination, self.path)
    }
}

struct DesiredItem {
    spec: RegistrationSpec,
    token: u64,
    bound_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Binding {
    owner: String,
    path: String,
}

struct Item {
    binding: Binding,
    aliases: BTreeSet<String>,
    cancellable: gio::Cancellable,
    subscriptions: Vec<gio::SignalSubscription>,
    menu_subscription: Option<gio::SignalSubscription>,
    properties: Properties,
    view: Option<ItemView>,
    refresh_inflight: bool,
    refresh_again: bool,
    properties_serial: u64,
    menu_path: Option<String>,
    menu_path_serial: u64,
    menu_serial: u64,
    menu: Option<MenuLayout>,
}

impl Item {
    fn new(binding: Binding) -> Self {
        Self {
            binding,
            aliases: BTreeSet::new(),
            cancellable: gio::Cancellable::new(),
            subscriptions: Vec::new(),
            menu_subscription: None,
            properties: Properties::new(),
            view: None,
            refresh_inflight: false,
            refresh_again: false,
            properties_serial: 0,
            menu_path: None,
            menu_path_serial: 0,
            menu_serial: 0,
            menu: None,
        }
    }

    fn invalidate_menu(&mut self) {
        self.menu_serial = self.menu_serial.wrapping_add(1);
        self.menu = None;
    }

    fn invalidate_menu_path(&mut self) {
        self.invalidate_menu();
        self.menu_path_serial = self.menu_path_serial.wrapping_add(1);
        self.menu_path = None;
        self.menu_subscription = None;
        if let Some(view) = self.view.as_mut() {
            view.has_menu = false;
        }
    }
}

impl Drop for Item {
    fn drop(&mut self) {
        self.cancellable.cancel();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemView {
    id: String,
    title: String,
    status: String,
    category: String,
    is_menu: bool,
    has_menu: bool,
    tooltip: String,
    icon: Option<Icon>,
    attention_icon: Option<Icon>,
    overlay_icon: Option<Icon>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MenuNode {
    id: i32,
    label: String,
    enabled: bool,
    visible: bool,
    #[serde(rename = "type")]
    node_type: String,
    toggle_type: Option<String>,
    toggle_state: i32,
    icon: Option<Icon>,
    has_submenu: bool,
    children: Vec<MenuNode>,
}

struct MenuNodeAccess {
    enabled: bool,
    visible: bool,
    activatable: bool,
    submenu: bool,
}

struct MenuLayout {
    revision: u32,
    root: MenuNode,
    nodes: BTreeMap<i32, MenuNodeAccess>,
}

impl MenuLayout {
    fn validate_activation(&self, id: i32, revision: u32) -> Result<(), NativeMethodError> {
        if self.revision != revision {
            return Err(error(
                "stale_tray_menu",
                "The tray menu revision is no longer current",
            ));
        }
        let node = self.nodes.get(&id).ok_or_else(|| {
            error(
                "unknown_tray_menu_item",
                "The tray menu item does not exist",
            )
        })?;
        if !node.enabled || !node.visible {
            return Err(error(
                "tray_menu_item_disabled",
                "The tray menu item is disabled or hidden",
            ));
        }
        if !node.activatable {
            return Err(error(
                "tray_menu_item_not_activatable",
                "The tray menu node is not an action",
            ));
        }
        Ok(())
    }

    fn validate_submenu(&self, id: i32) -> Result<(), NativeMethodError> {
        let node = self.nodes.get(&id).ok_or_else(|| {
            error(
                "unknown_tray_menu_item",
                "The tray menu item does not exist",
            )
        })?;
        if !node.enabled || !node.visible {
            return Err(error(
                "tray_menu_item_disabled",
                "The tray submenu is disabled or hidden",
            ));
        }
        if !node.submenu {
            return Err(error(
                "tray_menu_item_not_activatable",
                "The tray menu node is not a submenu",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct ItemHandle {
    id: String,
    binding: Binding,
    connection: gio::DBusConnection,
    cancellable: gio::Cancellable,
}

struct MenuRequest {
    state: Weak<RefCell<State>>,
    item: ItemHandle,
    path: String,
    path_serial: u64,
    callback: Box<dyn FnOnce(Result<Value, NativeMethodError>)>,
}

impl State {
    fn new() -> Self {
        let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
        Self {
            stopped: false,
            available: false,
            reason: "Connecting to the session bus",
            instance,
            serial: 0,
            next_item: 0,
            startup_cancel: gio::Cancellable::new(),
            host_name: format!(
                "org.kde.StatusNotifierHost-{}-{instance}",
                std::process::id()
            ),
            host_acquired: false,
            bus: None,
            watcher: None,
            owned_watcher: None,
            items: BTreeMap::new(),
            bindings: BTreeMap::new(),
        }
    }

    fn next_serial(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.serial
    }

    fn connection(&self) -> Option<gio::DBusConnection> {
        self.bus.as_ref().map(|bus| bus.connection.clone())
    }

    fn epoch_matches(&self, epoch: u64) -> bool {
        !self.stopped
            && self
                .watcher
                .as_ref()
                .is_some_and(|watcher| watcher.epoch == epoch)
    }

    fn handle(&self, id: &str) -> Result<ItemHandle, NativeMethodError> {
        if self.stopped || !self.available {
            return Err(error(
                "tray_unavailable",
                "The status notifier host is unavailable",
            ));
        }
        let item = self
            .items
            .get(id)
            .filter(|item| item.view.is_some())
            .ok_or_else(|| error("unknown_tray_item", "The tray item is no longer registered"))?;
        let connection = self
            .connection()
            .ok_or_else(|| error("tray_unavailable", "The session bus is unavailable"))?;
        Ok(ItemHandle {
            id: id.into(),
            binding: item.binding.clone(),
            connection,
            cancellable: item.cancellable.clone(),
        })
    }

    fn validate_handle(&self, handle: &ItemHandle) -> Result<(), NativeMethodError> {
        if self.stopped || !self.available {
            return Err(error(
                "tray_unavailable",
                "The status notifier host is unavailable",
            ));
        }
        if self
            .items
            .get(&handle.id)
            .is_none_or(|item| item.binding != handle.binding)
        {
            return Err(error(
                "unknown_tray_item",
                "The tray item is no longer registered",
            ));
        }
        Ok(())
    }

    fn bind(&mut self, canonical: &str, binding: Binding) -> (String, bool) {
        let existing = self.bindings.get(&binding).cloned();
        let (id, added) = match existing {
            Some(id) => (id, false),
            None => {
                self.next_item += 1;
                let id = format!("tray-{}-{}", self.instance, self.next_item);
                self.bindings.insert(binding.clone(), id.clone());
                self.items.insert(id.clone(), Item::new(binding));
                (id, true)
            }
        };
        if let Some(item) = self.items.get_mut(&id) {
            item.aliases.insert(canonical.into());
        }
        if let Some(desired) = self
            .watcher
            .as_mut()
            .and_then(|watcher| watcher.desired.get_mut(canonical))
        {
            desired.bound_id = Some(id.clone());
        }
        (id, added)
    }

    fn forget(&mut self, canonical: &str) {
        let desired = self
            .watcher
            .as_mut()
            .and_then(|watcher| watcher.desired.remove(canonical));
        let Some(id) = desired.and_then(|desired| desired.bound_id) else {
            return;
        };
        let remove = self.items.get_mut(&id).is_some_and(|item| {
            item.aliases.remove(canonical);
            item.aliases.is_empty()
        });
        if remove && let Some(item) = self.items.remove(&id) {
            self.bindings.remove(&item.binding);
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.startup_cancel.cancel();
        self.watcher.take();
        self.items.clear();
        self.bus.take();
    }
}

impl TrayController {
    pub(crate) fn new() -> Self {
        let state = Rc::new(RefCell::new(State::new()));
        let weak = Rc::downgrade(&state);
        let cancellable = state.borrow().startup_cancel.clone();
        gio::bus_get(gio::BusType::Session, Some(&cancellable), move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if state.borrow().stopped {
                return;
            }
            match result {
                Ok(connection) => connect_bus(&state, connection),
                Err(_) => state.borrow_mut().reason = "The session bus is unavailable",
            }
        });
        Self { state }
    }

    pub(crate) fn snapshot(&self) -> Value {
        let state = self.state.borrow();
        if !state.available || state.stopped {
            return json!({"available": false, "reason": state.reason});
        }
        let items = state
            .items
            .values()
            .filter_map(|item| item.view.as_ref())
            .map(|view| json!(view))
            .collect();
        let mut snapshot = json!({ "available": true });
        snapshot["items"] = Value::Array(items);
        snapshot
    }

    pub(crate) fn shutdown(&self) {
        release_bus(&self.state, true, "The tray controller has stopped");
    }

    pub(crate) fn get_menu(
        &self,
        item_id: String,
        menu_item_id: i32,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        let target = {
            let state = self.state.borrow();
            state.handle(&item_id).and_then(|item| {
                let cached = &state.items[&item_id];
                let path = cached.menu_path.clone().ok_or_else(|| {
                    error(
                        "tray_menu_unavailable",
                        "This tray item does not export a DBusMenu",
                    )
                })?;
                if menu_item_id != 0 {
                    cached
                        .menu
                        .as_ref()
                        .ok_or_else(|| {
                            error(
                                "stale_tray_menu",
                                "Fetch the current tray menu before expanding a submenu",
                            )
                        })?
                        .validate_submenu(menu_item_id)?;
                }
                Ok((item, path, cached.menu_path_serial))
            })
        };
        let (item, path, path_serial) = match target {
            Ok(target) => target,
            Err(failure) => {
                callback(Err(failure));
                return;
            }
        };
        prepare_menu_node(
            MenuRequest {
                state: Rc::downgrade(&self.state),
                item,
                path,
                path_serial,
                callback: Box::new(callback),
            },
            menu_item_id,
        );
    }

    pub(crate) fn activate(
        &self,
        item_id: String,
        point: ScreenPoint,
        secondary: bool,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        self.item_action(
            item_id,
            if secondary {
                "SecondaryActivate"
            } else {
                "Activate"
            },
            (point.x, point.y).to_variant(),
            callback,
        );
    }

    pub(crate) fn context_menu(
        &self,
        item_id: String,
        point: ScreenPoint,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        self.item_action(
            item_id,
            "ContextMenu",
            (point.x, point.y).to_variant(),
            callback,
        );
    }

    pub(crate) fn scroll(
        &self,
        item_id: String,
        delta: i32,
        orientation: TrayOrientation,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        self.item_action(
            item_id,
            "Scroll",
            (delta, orientation.as_str()).to_variant(),
            callback,
        );
    }

    pub(crate) fn activate_menu(
        &self,
        item_id: String,
        menu_item_id: i32,
        revision: u32,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        let target = {
            let state = self.state.borrow();
            state.handle(&item_id).and_then(|handle| {
                let item = &state.items[&item_id];
                let path = item.menu_path.clone().ok_or_else(|| {
                    error(
                        "tray_menu_unavailable",
                        "This tray item does not export a DBusMenu",
                    )
                })?;
                let menu = item.menu.as_ref().ok_or_else(|| {
                    error(
                        "stale_tray_menu",
                        "Fetch the current tray menu before activating an item",
                    )
                })?;
                menu.validate_activation(menu_item_id, revision)?;
                Ok((handle, path))
            })
        };
        let (handle, path) = match target {
            Ok(target) => target,
            Err(failure) => {
                callback(Err(failure));
                return;
            }
        };
        let timestamp = event_timestamp();
        let params = (menu_item_id, "clicked", 0_i32.to_variant(), timestamp).to_variant();
        let weak = Rc::downgrade(&self.state);
        let completion_handle = handle.clone();
        handle.connection.call(Some(&handle.binding.owner), &path, MENU_INTERFACE, "Event",
            Some(&params), None, gio::DBusCallFlags::NONE, CALL_TIMEOUT,
            Some(&handle.cancellable), move |result| {
                let outcome = action_completion(&weak, &completion_handle, result)
                    .map(|()| json!({"itemId": completion_handle.id, "menuItemId": menu_item_id, "revision": revision}));
                callback(outcome);
            });
    }

    fn item_action(
        &self,
        item_id: String,
        method: &'static str,
        params: glib::Variant,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        let handle = self.state.borrow().handle(&item_id);
        let handle = match handle {
            Ok(handle) => handle,
            Err(failure) => {
                callback(Err(failure));
                return;
            }
        };
        let weak = Rc::downgrade(&self.state);
        let completion_handle = handle.clone();
        handle.connection.call(
            Some(&handle.binding.owner),
            &handle.binding.path,
            ITEM_INTERFACE,
            method,
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT,
            Some(&handle.cancellable),
            move |result| {
                let outcome = action_completion(&weak, &completion_handle, result)
                    .map(|()| json!({"itemId": completion_handle.id}));
                callback(outcome);
            },
        );
    }
}

impl super::Provider for TrayController {
    fn name(&self) -> &'static str {
        "tray"
    }
    fn snapshot(&self) -> Value {
        TrayController::snapshot(self)
    }

    fn diagnostic_snapshot(&self) -> Value {
        let state = self.state.borrow();
        if !state.available || state.stopped {
            return json!({ "available": false, "reason": state.reason });
        }
        let count = state
            .items
            .values()
            .filter(|item| item.view.is_some())
            .count();
        json!({ "available": true, "count": count })
    }
}

fn error(code: &'static str, message: &'static str) -> NativeMethodError {
    NativeMethodError::new(code, message)
}

fn release_bus(state: &Rc<RefCell<State>>, stopped: bool, reason: &'static str) {
    let (bus, watcher, items) = {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        state.stopped = stopped;
        state.available = false;
        state.reason = reason;
        state.host_acquired = false;
        state.owned_watcher = None;
        state.startup_cancel.cancel();
        state.bindings.clear();
        (
            state.bus.take(),
            state.watcher.take(),
            std::mem::take(&mut state.items),
        )
    };
    drop(watcher);
    drop(items);
    drop(bus);
}

fn connect_bus(state: &Rc<RefCell<State>>, connection: gio::DBusConnection) {
    let resources = BusResources {
        connection: connection.clone(),
        watcher_registration: None,
        host_registration: None,
        watcher_name: None,
        host_name: None,
        unwatch_watcher: None,
        owner_subscription: None,
        closed_handler: None,
    };
    state.borrow_mut().bus = Some(resources);
    let registration = (|| {
        let node = gio::DBusNodeInfo::for_xml(WATCHER_XML).ok()?;
        let interface = node.lookup_interface(WATCHER_INTERFACE)?;
        let weak_methods = Rc::downgrade(state);
        let weak_properties = Rc::downgrade(state);
        let watcher = connection
            .register_object(WATCHER_PATH, &interface)
            .method_call(move |_, sender, _, _, method, params, invocation| {
                let Some(state) = weak_methods.upgrade() else {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed",
                        "The watcher has stopped",
                    );
                    return;
                };
                watcher_method(&state, sender, method, &params, invocation);
            })
            .property(move |_, _, _, _, property| {
                let state = weak_properties.upgrade();
                watcher_property(state.as_ref(), property)
            })
            .build()
            .ok()?;
        state.borrow_mut().bus.as_mut()?.watcher_registration = Some(watcher);
        let node = gio::DBusNodeInfo::for_xml(HOST_XML).ok()?;
        let interface = node.lookup_interface("org.kde.StatusNotifierHost")?;
        let host = connection
            .register_object("/StatusNotifierHost", &interface)
            .build()
            .ok()?;
        state.borrow_mut().bus.as_mut()?.host_registration = Some(host);
        Some(())
    })();
    if registration.is_none() {
        release_bus(
            state,
            false,
            "The status notifier interfaces could not be exported",
        );
        return;
    }
    let weak = Rc::downgrade(state);
    let subscription = connection.subscribe_to_signal(
        Some(BUS_NAME),
        Some(BUS_NAME),
        Some("NameOwnerChanged"),
        Some(BUS_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if let Some(state) = weak.upgrade()
                && let Some((name, old, new)) = signal.parameters.get::<(String, String, String)>()
            {
                owner_changed(&state, &name, &old, &new);
            }
        },
    );
    state.borrow_mut().bus.as_mut().unwrap().owner_subscription = Some(subscription);
    let weak = Rc::downgrade(state);
    let closed = connection.connect_local("closed", false, move |_| {
        if let Some(state) = weak.upgrade() {
            release_bus(&state, false, "The session bus connection is closed");
        }
        None
    });
    state.borrow_mut().bus.as_mut().unwrap().closed_handler = Some(closed);
    let appeared = Rc::downgrade(state);
    let vanished = Rc::downgrade(state);
    let watch = gio::bus_watch_name_on_connection(
        &connection,
        WATCHER_NAME,
        gio::BusNameWatcherFlags::NONE,
        move |_, _, owner| {
            if let Some(state) = appeared.upgrade() {
                replace_watcher(&state, Some(owner));
            }
        },
        move |connection, _| {
            if let Some(state) = vanished.upgrade() {
                if state.borrow().owned_watcher.is_some() {
                    if let Some(owner) = connection.unique_name() {
                        replace_watcher(&state, Some(&owner));
                    }
                } else {
                    replace_watcher(&state, None);
                }
            }
        },
    );
    // GIO's bus watcher token is not publicly nameable in this version.
    state.borrow_mut().bus.as_mut().unwrap().unwatch_watcher =
        Some(Box::new(move || gio::bus_unwatch_name(watch)));
    let acquired = Rc::downgrade(state);
    let lost = Rc::downgrade(state);
    let host_name = state.borrow().host_name.clone();
    let host = gio::bus_own_name_on_connection(
        &connection,
        &host_name,
        gio::BusNameOwnerFlags::NONE,
        move |_, _| {
            if let Some(state) = acquired.upgrade() {
                if state.borrow().stopped || state.borrow().bus.is_none() {
                    return;
                }
                state.borrow_mut().host_acquired = true;
                join_watcher(&state);
            }
        },
        move |_, _| {
            if let Some(state) = lost.upgrade() {
                let mut state = state.borrow_mut();
                if state.stopped || state.bus.is_none() {
                    return;
                }
                state.host_acquired = false;
                state.available = false;
                state.reason = "The status notifier host name is unavailable";
                if let Some(watcher) = state.watcher.as_mut() {
                    watcher.joined = false;
                }
            }
        },
    );
    state.borrow_mut().bus.as_mut().unwrap().host_name = Some(host);
    let acquired = Rc::downgrade(state);
    let lost = Rc::downgrade(state);
    // NONE queues for an occupied name. In particular, never replace the user's watcher.
    let watcher = gio::bus_own_name_on_connection(
        &connection,
        WATCHER_NAME,
        gio::BusNameOwnerFlags::NONE,
        move |connection, _| {
            if let Some(state) = acquired.upgrade() {
                if state.borrow().stopped || state.borrow().bus.is_none() {
                    return;
                }
                state.borrow_mut().owned_watcher = Some(OwnedWatcher::default());
                if let Some(owner) = connection.unique_name() {
                    replace_watcher(&state, Some(&owner));
                }
                join_watcher(&state);
            }
        },
        move |connection, _| {
            if let Some(state) = lost.upgrade() {
                if state.borrow().stopped || state.borrow().bus.is_none() {
                    return;
                }
                let was_owned = state.borrow_mut().owned_watcher.take().is_some();
                if was_owned
                    && state.borrow().watcher.as_ref().is_some_and(|session| {
                        connection
                            .unique_name()
                            .is_some_and(|owner| session.owner == owner)
                    })
                {
                    replace_watcher(&state, None);
                }
            }
        },
    );
    state.borrow_mut().bus.as_mut().unwrap().watcher_name = Some(watcher);
}

fn replace_watcher(state: &Rc<RefCell<State>>, owner: Option<&str>) {
    let (connection, epoch, old_session, old_items) = {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        if state.watcher.as_ref().map(|watcher| watcher.owner.as_str()) == owner {
            return;
        }
        let Some(connection) = state.connection() else {
            return;
        };
        let old_session = state.watcher.take();
        let old_items = std::mem::take(&mut state.items);
        state.bindings.clear();
        state.available = false;
        state.reason = if owner.is_some() {
            "Registering the status notifier host"
        } else {
            "Waiting for a StatusNotifierWatcher"
        };
        let epoch = state.next_serial();
        if let Some(owner) = owner {
            state.watcher = Some(WatcherSession {
                owner: owner.into(),
                epoch,
                cancellable: gio::Cancellable::new(),
                subscriptions: Vec::new(),
                desired: BTreeMap::new(),
                joining: false,
                joined: false,
                list_inflight: false,
                list_again: false,
                list_serial: 0,
            });
        }
        (connection, epoch, old_session, old_items)
    };
    drop(old_session);
    drop(old_items);
    let Some(owner) = owner else {
        return;
    };
    let weak = Rc::downgrade(state);
    let subscription = connection.subscribe_to_signal(
        Some(owner),
        Some(WATCHER_INTERFACE),
        None,
        Some(WATCHER_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if !state.borrow().epoch_matches(epoch) {
                return;
            }
            match signal.signal_name {
                "StatusNotifierItemRegistered" => {
                    if let Some((service,)) = signal.parameters.get::<(String,)>() {
                        bump_list_serial(&state);
                        observe_registration(&state, &service);
                    }
                }
                "StatusNotifierItemUnregistered" => {
                    if let Some((service,)) = signal.parameters.get::<(String,)>() {
                        bump_list_serial(&state);
                        if let Some(spec) = RegistrationSpec::parse(&service, None) {
                            state.borrow_mut().forget(&spec.canonical());
                        }
                    }
                }
                _ => {}
            }
        },
    );
    let weak = Rc::downgrade(state);
    let properties = connection.subscribe_to_signal(
        Some(owner),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
        Some(WATCHER_PATH),
        Some(WATCHER_INTERFACE),
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if !state.borrow().epoch_matches(epoch) {
                return;
            }
            if let Some((_, changed, invalidated)) =
                signal.parameters.get::<(String, Properties, Vec<String>)>()
            {
                if let Some(list) = changed
                    .get("RegisteredStatusNotifierItems")
                    .and_then(|v| v.get::<Vec<String>>())
                {
                    bump_list_serial(&state);
                    reconcile_registrations(&state, &list);
                } else if invalidated
                    .iter()
                    .any(|name| name == "RegisteredStatusNotifierItems")
                {
                    bump_list_serial(&state);
                    fetch_watcher_items(&state);
                }
            }
        },
    );
    if let Some(session) = state.borrow_mut().watcher.as_mut() {
        session.subscriptions.extend([subscription, properties]);
    }
    join_watcher(state);
}

fn join_watcher(state: &Rc<RefCell<State>>) {
    let request = {
        let mut state = state.borrow_mut();
        if state.stopped || !state.host_acquired {
            return;
        }
        let Some(connection) = state.connection() else {
            return;
        };
        let host_name = state.host_name.clone();
        let local_ready = state.owned_watcher.is_some();
        let Some(watcher) = state.watcher.as_mut() else {
            return;
        };
        if watcher.joining || watcher.joined {
            return;
        }
        if connection
            .unique_name()
            .is_some_and(|name| watcher.owner == name)
            && !local_ready
        {
            return;
        }
        watcher.joining = true;
        (
            connection,
            watcher.owner.clone(),
            watcher.epoch,
            watcher.cancellable.clone(),
            host_name,
        )
    };
    let (connection, owner, epoch, cancellable, host_name) = request;
    let weak = Rc::downgrade(state);
    connection.call(
        Some(&owner),
        WATCHER_PATH,
        WATCHER_INTERFACE,
        "RegisterStatusNotifierHost",
        Some(&(host_name,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if !state.borrow().epoch_matches(epoch) {
                return;
            }
            let success = state.borrow().host_acquired
                && result.is_ok_and(|reply| reply.type_().as_str() == "()");
            {
                let mut state = state.borrow_mut();
                let watcher = state.watcher.as_mut().unwrap();
                watcher.joining = false;
                watcher.joined = success;
                if !success {
                    state.available = false;
                    state.reason = "The watcher rejected status notifier host registration";
                }
            }
            if success {
                fetch_watcher_items(&state);
            }
        },
    );
}

fn bump_list_serial(state: &Rc<RefCell<State>>) {
    if let Some(watcher) = state.borrow_mut().watcher.as_mut() {
        watcher.list_serial = watcher.list_serial.wrapping_add(1);
        if watcher.list_inflight {
            watcher.list_again = true;
        }
    }
}

fn fetch_watcher_items(state: &Rc<RefCell<State>>) {
    let request = {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        let Some(connection) = state.connection() else {
            return;
        };
        let Some(watcher) = state.watcher.as_mut() else {
            return;
        };
        if !watcher.joined {
            return;
        }
        if watcher.list_inflight {
            watcher.list_again = true;
            return;
        }
        watcher.list_inflight = true;
        watcher.list_again = false;
        (
            connection,
            watcher.owner.clone(),
            watcher.epoch,
            watcher.list_serial,
            watcher.cancellable.clone(),
        )
    };
    let (connection, owner, epoch, serial, cancellable) = request;
    let weak = Rc::downgrade(state);
    connection.call(
        Some(&owner),
        WATCHER_PATH,
        PROPERTIES_INTERFACE,
        "Get",
        Some(&(WATCHER_INTERFACE, "RegisteredStatusNotifierItems").to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if !state.borrow().epoch_matches(epoch) {
                return;
            }
            let list = result
                .ok()
                .and_then(|reply| reply.get::<(glib::Variant,)>())
                .and_then(|(value,)| value.get::<Vec<String>>());
            let (current, again) = {
                let mut state = state.borrow_mut();
                let watcher = state.watcher.as_mut().unwrap();
                watcher.list_inflight = false;
                let current = watcher.list_serial == serial;
                let again = watcher.list_again;
                watcher.list_again = false;
                if current {
                    state.available = list.is_some()
                        && state.host_acquired
                        && state.watcher.as_ref().is_some_and(|watcher| watcher.joined);
                    if list.is_none() {
                        state.reason = "The watcher did not provide its registered item list";
                    }
                }
                (current, again)
            };
            if current && let Some(list) = list {
                reconcile_registrations(&state, &list);
            }
            if again {
                fetch_watcher_items(&state);
            }
        },
    );
}

fn reconcile_registrations(state: &Rc<RefCell<State>>, list: &[String]) {
    {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        state.available =
            state.host_acquired && state.watcher.as_ref().is_some_and(|watcher| watcher.joined);
    }
    let registrations: BTreeMap<String, RegistrationSpec> = list
        .iter()
        .filter_map(|service| RegistrationSpec::parse(service, None))
        .map(|spec| (spec.canonical(), spec))
        .collect();
    let removed: Vec<String> = state
        .borrow()
        .watcher
        .as_ref()
        .map(|watcher| {
            watcher
                .desired
                .keys()
                .filter(|key| !registrations.contains_key(*key))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for canonical in removed {
        state.borrow_mut().forget(&canonical);
    }
    for (_, spec) in registrations {
        resolve_registration(state, spec, None, None);
    }
}

fn observe_registration(state: &Rc<RefCell<State>>, service: &str) {
    if let Some(spec) = RegistrationSpec::parse(service, None) {
        resolve_registration(state, spec, None, None);
    }
    if !state.borrow().available {
        fetch_watcher_items(state);
    }
}

fn resolve_registration(
    state: &Rc<RefCell<State>>,
    spec: RegistrationSpec,
    expected_sender: Option<String>,
    invocation: Option<gio::DBusMethodInvocation>,
) {
    let canonical = spec.canonical();
    let request = {
        let mut state = state.borrow_mut();
        let token = state.next_serial();
        if let Some(connection) = state.connection().filter(|_| {
            !state.stopped
                && state.watcher.is_some()
                && (invocation.is_none() || state.owned_watcher.is_some())
        }) {
            let token = if invocation.is_some() {
                state.owned_watcher.as_mut().unwrap().pending_items.insert(
                    token,
                    PendingRegistration {
                        spec: spec.clone(),
                        sender: expected_sender.clone().unwrap_or_default(),
                    },
                );
                token
            } else {
                let watcher = state.watcher.as_mut().unwrap();
                let desired =
                    watcher
                        .desired
                        .entry(canonical.clone())
                        .or_insert_with(|| DesiredItem {
                            spec: spec.clone(),
                            token,
                            bound_id: None,
                        });
                if desired.bound_id.is_some() {
                    return;
                }
                desired.token
            };
            let watcher = state.watcher.as_ref().unwrap();
            Some((
                connection,
                watcher.epoch,
                token,
                watcher.cancellable.clone(),
            ))
        } else {
            None
        }
    };
    let Some((connection, epoch, token, cancellable)) = request else {
        if let Some(invocation) = invocation {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.Failed",
                "The watcher is unavailable",
            );
        }
        return;
    };
    let weak = Rc::downgrade(state);
    connection.call(
        Some(BUS_NAME),
        BUS_PATH,
        BUS_NAME,
        "GetNameOwner",
        Some(&(spec.destination.as_str(),).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            let Some(state) = weak.upgrade() else {
                if let Some(invocation) = invocation {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed",
                        "The watcher has stopped",
                    );
                }
                return;
            };
            let valid = {
                let state = state.borrow();
                state.epoch_matches(epoch)
                    && if invocation.is_some() {
                        state
                            .owned_watcher
                            .as_ref()
                            .is_some_and(|watcher| watcher.pending_items.contains_key(&token))
                    } else {
                        state.watcher.as_ref().is_some_and(|watcher| {
                            watcher
                                .desired
                                .get(&canonical)
                                .is_some_and(|desired| desired.token == token)
                        })
                    }
            };
            if !valid {
                if let Some(invocation) = invocation {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed",
                        "The registration is no longer current",
                    );
                }
                return;
            }
            if invocation.is_some() {
                state
                    .borrow_mut()
                    .owned_watcher
                    .as_mut()
                    .unwrap()
                    .pending_items
                    .remove(&token);
            }
            let owner = result
                .ok()
                .and_then(|reply| reply.get::<(String,)>())
                .map(|(owner,)| owner)
                .filter(|owner| gio::dbus_is_unique_name(owner));
            let allowed = owner.as_ref().is_some_and(|owner| {
                expected_sender
                    .as_ref()
                    .is_none_or(|sender| sender == owner)
            });
            if !allowed {
                if invocation.is_none() {
                    state.borrow_mut().forget(&canonical);
                }
                if let Some(invocation) = invocation {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.AccessDenied",
                        "The registering peer does not own the service",
                    );
                }
                return;
            }
            let owner = owner.unwrap();
            let (id, added, advertised) = {
                let mut state = state.borrow_mut();
                if invocation.is_some() {
                    state
                        .watcher
                        .as_mut()
                        .unwrap()
                        .desired
                        .entry(canonical.clone())
                        .or_insert_with(|| DesiredItem {
                            spec: spec.clone(),
                            token,
                            bound_id: None,
                        });
                }
                let advertised = if invocation.is_some() {
                    state
                        .owned_watcher
                        .as_mut()
                        .unwrap()
                        .items
                        .insert(
                            canonical.clone(),
                            WatcherRecord {
                                spec: spec.clone(),
                                owner: owner.clone(),
                            },
                        )
                        .is_none()
                } else {
                    false
                };
                let (id, added) = state.bind(
                    &canonical,
                    Binding {
                        owner,
                        path: spec.path,
                    },
                );
                (id, added, advertised)
            };
            if added {
                observe_item(&state, &id);
            }
            if advertised {
                emit_watcher_signal(
                    &state,
                    "StatusNotifierItemRegistered",
                    Some(&(canonical,).to_variant()),
                );
                emit_watcher_property(&state, "RegisteredStatusNotifierItems");
            }
            if let Some(invocation) = invocation {
                invocation.return_value(None);
            }
        },
    );
}

fn watcher_method(
    state: &Rc<RefCell<State>>,
    sender: Option<&str>,
    method: &str,
    params: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    if state.borrow().stopped || state.borrow().owned_watcher.is_none() {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.Failed",
            "This peer does not own the watcher name",
        );
        return;
    }
    let Some(sender) = sender.filter(|sender| gio::dbus_is_unique_name(sender)) else {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.AccessDenied",
            "A session bus peer is required",
        );
        return;
    };
    let Some((service,)) = params.get::<(String,)>() else {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.InvalidArgs",
            "Expected a service name or object path",
        );
        return;
    };
    match method {
        "RegisterStatusNotifierItem" => {
            let Some(spec) = RegistrationSpec::parse(&service, Some(sender)) else {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.InvalidArgs",
                    "Invalid status notifier registration",
                );
                return;
            };
            let existing = state
                .borrow()
                .owned_watcher
                .as_ref()
                .and_then(|watcher| watcher.items.get(&spec.canonical()))
                .is_some_and(|item| item.owner == sender);
            if existing {
                invocation.return_value(None);
                return;
            }
            resolve_registration(state, spec, Some(sender.into()), Some(invocation));
        }
        "RegisterStatusNotifierHost" => register_host(state, sender, service, invocation),
        _ => invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod",
            "Unknown watcher method",
        ),
    }
}

fn register_host(
    state: &Rc<RefCell<State>>,
    sender: &str,
    service: String,
    invocation: gio::DBusMethodInvocation,
) {
    if !gio::dbus_is_name(&service) {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.InvalidArgs",
            "Invalid status notifier host name",
        );
        return;
    }
    let request = {
        let mut state = state.borrow_mut();
        let token = state.next_serial();
        let connection = state.connection().unwrap();
        let watcher = state.watcher.as_ref().unwrap();
        let epoch = watcher.epoch;
        let cancellable = watcher.cancellable.clone();
        let owned = state.owned_watcher.as_mut().unwrap();
        if owned
            .hosts
            .get(&service)
            .is_some_and(|owner| owner == sender)
        {
            invocation.return_value(None);
            return;
        }
        owned
            .pending_hosts
            .entry(service.clone())
            .or_default()
            .insert(token);
        (connection, epoch, cancellable, token)
    };
    let (connection, epoch, cancellable, token) = request;
    let sender = sender.to_owned();
    let weak = Rc::downgrade(state);
    connection.call(
        Some(BUS_NAME),
        BUS_PATH,
        BUS_NAME,
        "GetNameOwner",
        Some(&(service.as_str(),).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            let Some(state) = weak.upgrade() else {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.Failed",
                    "The watcher has stopped",
                );
                return;
            };
            let current = state.borrow().epoch_matches(epoch)
                && state
                    .borrow()
                    .owned_watcher
                    .as_ref()
                    .is_some_and(|watcher| {
                        watcher
                            .pending_hosts
                            .get(&service)
                            .is_some_and(|tokens| tokens.contains(&token))
                    });
            if !current {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.Failed",
                    "The host registration is no longer current",
                );
                return;
            }
            {
                let mut state = state.borrow_mut();
                let owned = state.owned_watcher.as_mut().unwrap();
                if let Some(tokens) = owned.pending_hosts.get_mut(&service) {
                    tokens.remove(&token);
                    if tokens.is_empty() {
                        owned.pending_hosts.remove(&service);
                    }
                }
            }
            let owner = result
                .ok()
                .and_then(|reply| reply.get::<(String,)>())
                .map(|(owner,)| owner);
            if owner.as_ref() != Some(&sender) {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.AccessDenied",
                    "The registering peer does not own the host name",
                );
                return;
            }
            let added = state
                .borrow_mut()
                .owned_watcher
                .as_mut()
                .unwrap()
                .hosts
                .insert(service, sender)
                .is_none();
            if added {
                emit_watcher_signal(&state, "StatusNotifierHostRegistered", None);
                emit_watcher_property(&state, "IsStatusNotifierHostRegistered");
            }
            invocation.return_value(None);
        },
    );
}

fn watcher_property(state: Option<&Rc<RefCell<State>>>, property: &str) -> glib::Variant {
    match property {
        "RegisteredStatusNotifierItems" => {
            let items: Vec<String> = state
                .and_then(|state| {
                    state
                        .borrow()
                        .owned_watcher
                        .as_ref()
                        .map(|watcher| watcher.items.keys().cloned().collect())
                })
                .unwrap_or_default();
            items.to_variant()
        }
        "IsStatusNotifierHostRegistered" => state
            .is_some_and(|state| {
                state
                    .borrow()
                    .owned_watcher
                    .as_ref()
                    .is_some_and(|watcher| !watcher.hosts.is_empty())
            })
            .to_variant(),
        "ProtocolVersion" => 0_i32.to_variant(),
        _ => ().to_variant(),
    }
}

fn emit_watcher_signal(state: &Rc<RefCell<State>>, signal: &str, params: Option<&glib::Variant>) {
    let connection = {
        let state = state.borrow();
        if state.stopped || state.owned_watcher.is_none() {
            return;
        }
        state.connection()
    };
    if let Some(connection) = connection {
        let _ = connection.emit_signal(None, WATCHER_PATH, WATCHER_INTERFACE, signal, params);
    }
}

fn emit_watcher_property(state: &Rc<RefCell<State>>, property: &str) {
    let value = watcher_property(Some(state), property);
    let changed: Properties = HashMap::from([(property.into(), value)]);
    let params = (WATCHER_INTERFACE, changed, Vec::<String>::new()).to_variant();
    let connection = state.borrow().connection();
    if let Some(connection) = connection {
        let _ = connection.emit_signal(
            None,
            WATCHER_PATH,
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            Some(&params),
        );
    }
}

fn owner_changed(state: &Rc<RefCell<State>>, name: &str, old: &str, new: &str) {
    if old.is_empty() || old == new {
        return;
    }
    let unique_gone = name.starts_with(':') && new.is_empty();
    let (removed_items, removed_hosts) = {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        let desired: Vec<String> = state
            .watcher
            .as_ref()
            .map(|watcher| {
                watcher
                    .desired
                    .iter()
                    .filter(|(_, desired)| {
                        desired.spec.destination == name
                            || (unique_gone
                                && desired
                                    .bound_id
                                    .as_ref()
                                    .and_then(|id| state.items.get(id))
                                    .is_some_and(|item| item.binding.owner == name))
                    })
                    .map(|(canonical, _)| canonical.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !desired.is_empty()
            && let Some(watcher) = state.watcher.as_mut()
        {
            watcher.list_serial = watcher.list_serial.wrapping_add(1);
            if watcher.list_inflight {
                watcher.list_again = true;
            }
        }
        for canonical in desired {
            state.forget(&canonical);
        }
        let mut removed_items = Vec::new();
        let mut removed_hosts = false;
        if let Some(watcher) = state.owned_watcher.as_mut() {
            watcher.items.retain(|canonical, record| {
                let remove =
                    record.spec.destination == name || (unique_gone && record.owner == name);
                if remove {
                    removed_items.push(canonical.clone());
                }
                !remove
            });
            watcher.hosts.retain(|service, owner| {
                let remove = service == name || (unique_gone && owner == name);
                removed_hosts |= remove;
                !remove
            });
            watcher.pending_hosts.remove(name);
            watcher.pending_items.retain(|_, request| {
                request.spec.destination != name && !(unique_gone && request.sender == name)
            });
        }
        (removed_items, removed_hosts)
    };
    for canonical in &removed_items {
        emit_watcher_signal(
            state,
            "StatusNotifierItemUnregistered",
            Some(&(canonical.as_str(),).to_variant()),
        );
    }
    if !removed_items.is_empty() {
        emit_watcher_property(state, "RegisteredStatusNotifierItems");
    }
    if removed_hosts {
        emit_watcher_signal(state, "StatusNotifierHostUnregistered", None);
        emit_watcher_property(state, "IsStatusNotifierHostRegistered");
    }
}

fn observe_item(state: &Rc<RefCell<State>>, id: &str) {
    let request = {
        let state = state.borrow();
        let Some(connection) = state.connection() else {
            return;
        };
        let Some(item) = state.items.get(id) else {
            return;
        };
        (connection, item.binding.clone())
    };
    let (connection, binding) = request;
    let weak = Rc::downgrade(state);
    let item_id = id.to_owned();
    let signals = connection.subscribe_to_signal(
        Some(&binding.owner),
        Some(ITEM_INTERFACE),
        None,
        Some(&binding.path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            match signal.signal_name {
                "NewTitle" | "NewIcon" | "NewAttentionIcon" | "NewOverlayIcon" | "NewToolTip"
                | "NewStatus" | "NewMenu" => {
                    item_changed(&state, &item_id, signal.signal_name == "NewMenu")
                }
                _ => {}
            }
        },
    );
    let weak = Rc::downgrade(state);
    let item_id = id.to_owned();
    let properties = connection.subscribe_to_signal(
        Some(&binding.owner),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
        Some(&binding.path),
        Some(ITEM_INTERFACE),
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if let Some((_, changed, invalidated)) =
                signal.parameters.get::<(String, Properties, Vec<String>)>()
            {
                let menu_changed =
                    changed.contains_key("Menu") || invalidated.iter().any(|name| name == "Menu");
                item_changed(&state, &item_id, menu_changed);
            }
        },
    );
    if let Some(item) = state.borrow_mut().items.get_mut(id) {
        item.subscriptions.extend([signals, properties]);
    }
    refresh_item(state, id);
}

fn item_changed(state: &Rc<RefCell<State>>, id: &str, menu_changed: bool) {
    {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        let Some(item) = state.items.get_mut(id) else {
            return;
        };
        item.properties_serial = item.properties_serial.wrapping_add(1);
        if menu_changed {
            item.invalidate_menu_path();
        }
        if item.refresh_inflight {
            item.refresh_again = true;
        }
    }
    refresh_item(state, id);
}

fn refresh_item(state: &Rc<RefCell<State>>, id: &str) {
    let request = {
        let mut state = state.borrow_mut();
        if state.stopped {
            return;
        }
        let Some(connection) = state.connection() else {
            return;
        };
        let Some(item) = state.items.get_mut(id) else {
            return;
        };
        if item.refresh_inflight {
            item.refresh_again = true;
            return;
        }
        item.refresh_inflight = true;
        item.refresh_again = false;
        (
            connection,
            item.binding.clone(),
            item.cancellable.clone(),
            item.properties_serial,
        )
    };
    let (connection, binding, cancellable, serial) = request;
    let weak = Rc::downgrade(state);
    let id = id.to_owned();
    connection.call(
        Some(&binding.owner),
        &binding.path,
        PROPERTIES_INTERFACE,
        "GetAll",
        Some(&(ITEM_INTERFACE,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let properties = result
                .ok()
                .and_then(|reply| reply.get::<(Properties,)>())
                .map(|(props,)| props);
            let (again, menu_changed) = {
                let mut state = state.borrow_mut();
                if state.stopped {
                    return;
                }
                let Some(item) = state.items.get_mut(&id) else {
                    return;
                };
                // Opaque item IDs are never rebound to another owner or path.
                item.refresh_inflight = false;
                let again = item.refresh_again;
                item.refresh_again = false;
                let mut menu_changed = false;
                if item.properties_serial == serial {
                    let observed = properties.and_then(|properties| {
                        item_view(&id, &properties, &item.properties, item.view.as_ref())
                            .map(|view| (properties, view))
                    });
                    if let Some((properties, view)) = observed {
                        let path = exported_menu_path(&properties);
                        if item.menu_path.as_deref() != path {
                            item.invalidate_menu_path();
                            item.menu_path = path.map(str::to_owned);
                            menu_changed = true;
                        }
                        item.view = Some(view);
                        item.properties = properties;
                    } else {
                        item.view = None;
                        item.properties.clear();
                        item.invalidate_menu_path();
                    }
                }
                (again, menu_changed)
            };
            if menu_changed {
                observe_menu(&state, &id);
            }
            if again {
                refresh_item(&state, &id);
            }
        },
    );
}

fn observe_menu(state: &Rc<RefCell<State>>, id: &str) {
    let request = {
        let state = state.borrow();
        let Some(connection) = state.connection() else {
            return;
        };
        let Some(item) = state.items.get(id) else {
            return;
        };
        let Some(path) = item.menu_path.as_ref() else {
            return;
        };
        (
            connection,
            item.binding.owner.clone(),
            path.clone(),
            item.menu_path_serial,
        )
    };
    let (connection, owner, path, path_serial) = request;
    let weak = Rc::downgrade(state);
    let item_id = id.to_owned();
    let subscription = connection.subscribe_to_signal(
        Some(&owner),
        Some(MENU_INTERFACE),
        None,
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if !matches!(
                signal.signal_name,
                "LayoutUpdated" | "ItemsPropertiesUpdated"
            ) {
                return;
            }
            let Some(state) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.stopped {
                return;
            }
            if let Some(item) = state.items.get_mut(&item_id)
                && item.menu_path_serial == path_serial
            {
                item.invalidate_menu();
            }
        },
    );
    if let Some(item) = state.borrow_mut().items.get_mut(id)
        && item.menu_path_serial == path_serial
    {
        item.menu_subscription = Some(subscription);
    }
}

fn item_view(
    id: &str,
    props: &Properties,
    old_props: &Properties,
    old: Option<&ItemView>,
) -> Option<ItemView> {
    let status = props.get("Status")?.str()?;
    if !matches!(status, "Passive" | "Active" | "NeedsAttention") {
        return None;
    }
    let title = props
        .get("Title")
        .and_then(glib::Variant::str)
        .or_else(|| props.get("Id").and_then(glib::Variant::str))
        .unwrap_or("");
    let category = props
        .get("Category")
        .and_then(glib::Variant::str)
        .unwrap_or("ApplicationStatus");
    let tooltip = props
        .get("ToolTip")
        .filter(|value| value.type_().as_str() == "(sa(iiay)ss)")
        .map(|value| {
            let title = value.child_value(2);
            let description = value.child_value(3);
            let title = plain_text(title.str().unwrap_or(""));
            let description = plain_text(description.str().unwrap_or(""));
            if title.is_empty() {
                description
            } else if description.is_empty() {
                title
            } else {
                format!("{title}\n{description}")
            }
        })
        .unwrap_or_default();
    let cached_icon = |name: &str, pixmap: &str, old_icon: Option<&Option<Icon>>| {
        if old.is_some()
            && props.get(name) == old_props.get(name)
            && props.get(pixmap) == old_props.get(pixmap)
        {
            old_icon.cloned().flatten()
        } else {
            item_icon(props, name, pixmap)
        }
    };
    Some(ItemView {
        id: id.into(),
        title: plain_text(title),
        status: status.into(),
        category: plain_text(category),
        is_menu: props
            .get("ItemIsMenu")
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false),
        has_menu: exported_menu_path(props).is_some(),
        tooltip,
        icon: cached_icon("IconName", "IconPixmap", old.map(|view| &view.icon)),
        attention_icon: cached_icon(
            "AttentionIconName",
            "AttentionIconPixmap",
            old.map(|view| &view.attention_icon),
        ),
        overlay_icon: cached_icon(
            "OverlayIconName",
            "OverlayIconPixmap",
            old.map(|view| &view.overlay_icon),
        ),
    })
}

fn exported_menu_path(props: &Properties) -> Option<&str> {
    props
        .get("Menu")
        .filter(|value| value.type_().as_str() == "o")
        .and_then(glib::Variant::str)
        .filter(|path| *path != "/" && glib::Variant::is_object_path(path))
}

fn safe_theme_icon(name: &str) -> Option<Icon> {
    if name.is_empty()
        || name.len() > 256
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
    {
        return None;
    }
    icons::theme_icon(name, ICON_SIZE)
}

fn item_icon(props: &Properties, name: &str, pixmap: &str) -> Option<Icon> {
    if let Some(icon) = props
        .get(name)
        .and_then(glib::Variant::str)
        .and_then(safe_theme_icon)
    {
        return Some(icon);
    }
    let pixmaps = props.get(pixmap)?;
    if pixmaps.type_().as_str() != "a(iiay)" {
        return None;
    }
    let candidate = (0..pixmaps.n_children())
        .filter_map(|index| {
            let pixmap = pixmaps.child_value(index);
            let width = pixmap.child_value(0).get::<i32>()?;
            let height = pixmap.child_value(1).get::<i32>()?;
            if width <= 0 || height <= 0 || width > 512 || height > 512 {
                return None;
            }
            let bytes = pixmap.child_value(2);
            if bytes.fixed_array::<u8>().ok()?.len() != (width as usize) * (height as usize) * 4 {
                return None;
            }
            let edge = width.max(height);
            let rank = ((edge < ICON_SIZE) as i32, (edge - ICON_SIZE).abs());
            Some((rank, width, height, bytes))
        })
        .min_by_key(|(rank, _, _, _)| *rank)?;
    icons::from_argb(
        candidate.1,
        candidate.2,
        candidate.3.fixed_array::<u8>().ok()?,
    )
}

fn plain_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len().min(MAX_TEXT_CHARS));
    let mut chars = text.chars().peekable();
    let mut count = 0;
    while let Some(ch) = chars.next() {
        if count >= MAX_TEXT_CHARS {
            break;
        }
        if ch == '<'
            && chars
                .peek()
                .is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, '/' | '!'))
        {
            let mut tag = String::new();
            for next in chars.by_ref().take(256) {
                if next == '>' {
                    break;
                }
                tag.push(next);
            }
            if matches!(
                tag.trim().to_ascii_lowercase().as_str(),
                "br" | "br/" | "br /" | "/p"
            ) {
                output.push('\n');
                count += 1;
            }
            continue;
        }
        let decoded = if ch == '&' {
            let mut probe = chars.clone();
            let mut entity = String::new();
            let mut consumed = 0;
            let mut ended = false;
            for next in probe.by_ref().take(12) {
                consumed += 1;
                if next == ';' {
                    ended = true;
                    break;
                }
                if next.is_whitespace() {
                    break;
                }
                entity.push(next);
            }
            let decoded = ended.then(|| decode_entity(&entity)).flatten();
            if let Some(decoded) = decoded {
                for _ in 0..consumed {
                    chars.next();
                }
                decoded
            } else {
                ch
            }
        } else {
            ch
        };
        if !decoded.is_control() || matches!(decoded, '\n' | '\t') {
            output.push(decoded);
            count += 1;
        }
    }
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let value = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                entity.strip_prefix('#')?.parse().ok()?
            };
            char::from_u32(value)
        }
    }
}

fn display_label(label: &str) -> String {
    // DBusMenu labels are literal text, not the markup allowed in SNI tooltips.
    let mut output = String::with_capacity(label.len().min(MAX_TEXT_CHARS));
    let mut chars = label
        .chars()
        .filter(|ch| !ch.is_control() || matches!(*ch, '\n' | '\t'))
        .peekable();
    let mut count = 0;
    while let Some(ch) = chars.next() {
        if count >= MAX_TEXT_CHARS {
            break;
        }
        if ch == '_' {
            if chars.peek() != Some(&'_') {
                continue;
            }
            chars.next();
        }
        output.push(ch);
        count += 1;
    }
    output
}

fn parse_menu(reply: &glib::Variant) -> Result<MenuLayout, NativeMethodError> {
    if reply.type_().as_str() != "(u(ia{sv}av))" {
        return Err(error(
            "tray_menu_unavailable",
            "The tray menu returned an invalid layout",
        ));
    }
    let revision = reply.child_value(0).get::<u32>().unwrap();
    let mut nodes = BTreeMap::new();
    let root = parse_menu_node(&reply.child_value(1), 0, true, true, &mut nodes)?;
    if root.id != 0 {
        return Err(error(
            "tray_menu_unavailable",
            "The tray menu root is invalid",
        ));
    }
    Ok(MenuLayout {
        revision,
        root,
        nodes,
    })
}

fn parse_menu_node(
    value: &glib::Variant,
    depth: usize,
    parent_enabled: bool,
    parent_visible: bool,
    nodes: &mut BTreeMap<i32, MenuNodeAccess>,
) -> Result<MenuNode, NativeMethodError> {
    let invalid = || {
        error(
            "tray_menu_unavailable",
            "The tray menu returned an invalid or oversized tree",
        )
    };
    if depth > MAX_MENU_DEPTH
        || nodes.len() >= MAX_MENU_NODES
        || value.type_().as_str() != "(ia{sv}av)"
    {
        return Err(invalid());
    }
    let id = value.child_value(0).get::<i32>().ok_or_else(invalid)?;
    if nodes.contains_key(&id) || (depth > 0 && id == 0) {
        return Err(invalid());
    }
    let props = value
        .child_value(1)
        .get::<Properties>()
        .ok_or_else(invalid)?;
    let string = |name: &str, default: &str| -> Result<String, NativeMethodError> {
        match props.get(name) {
            Some(value) => value.str().map(str::to_owned).ok_or_else(invalid),
            None => Ok(default.into()),
        }
    };
    let boolean = |name: &str| -> Result<bool, NativeMethodError> {
        match props.get(name) {
            Some(value) => value.get::<bool>().ok_or_else(invalid),
            None => Ok(true),
        }
    };
    let label = display_label(&string("label", "")?);
    let enabled = boolean("enabled")?;
    let visible = boolean("visible")?;
    let node_type = string("type", "standard")?;
    let toggle_type = match string("toggle-type", "")?.as_str() {
        "" => None,
        "checkmark" => Some("checkmark".into()),
        "radio" => Some("radio".into()),
        _ => return Err(invalid()),
    };
    let toggle_state = match props.get("toggle-state") {
        Some(value) => value.get::<i32>().ok_or_else(invalid)?,
        None => -1,
    };
    let children_value = value.child_value(2);
    let submenu = children_value.n_children() > 0 || string("children-display", "")? == "submenu";
    let effective_enabled = parent_enabled && enabled;
    let effective_visible = parent_visible && visible;
    nodes.insert(
        id,
        MenuNodeAccess {
            enabled: effective_enabled,
            visible: effective_visible,
            activatable: id != 0 && node_type == "standard" && !submenu,
            submenu: submenu && node_type == "standard",
        },
    );
    let mut children = Vec::with_capacity(children_value.n_children().min(MAX_MENU_NODES));
    for index in 0..children_value.n_children() {
        let child = children_value
            .child_value(index)
            .as_variant()
            .ok_or_else(invalid)?;
        children.push(parse_menu_node(
            &child,
            depth + 1,
            effective_enabled,
            effective_visible,
            nodes,
        )?);
    }
    let icon = props
        .get("icon-name")
        .and_then(glib::Variant::str)
        .and_then(safe_theme_icon)
        .or_else(|| {
            props
                .get("icon-data")
                .and_then(|bytes| bytes.fixed_array::<u8>().ok())
                .and_then(icons::from_png)
        });
    Ok(MenuNode {
        id,
        label,
        enabled,
        visible,
        has_submenu: submenu && node_type == "standard",
        node_type,
        toggle_type,
        toggle_state,
        icon,
        children,
    })
}

fn validate_menu_request(
    request: &MenuRequest,
) -> Result<(Rc<RefCell<State>>, u64), NativeMethodError> {
    let state = request
        .state
        .upgrade()
        .ok_or_else(|| error("tray_unavailable", "The tray controller has stopped"))?;
    let serial = {
        let state = state.borrow();
        state.validate_handle(&request.item)?;
        let item = &state.items[&request.item.id];
        if item.menu_path_serial != request.path_serial
            || item.menu_path.as_ref() != Some(&request.path)
        {
            return Err(error(
                "stale_tray_menu",
                "The tray item has changed its exported menu",
            ));
        }
        item.menu_serial
    };
    Ok((state, serial))
}

fn prepare_menu_node(request: MenuRequest, id: i32) {
    if let Err(failure) = validate_menu_request(&request) {
        (request.callback)(Err(failure));
        return;
    }
    let connection = request.item.connection.clone();
    let owner = request.item.binding.owner.clone();
    let cancellable = request.item.cancellable.clone();
    let path = request.path.clone();
    connection.call(
        Some(&owner),
        &path,
        MENU_INTERFACE,
        "AboutToShow",
        Some(&(id,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT,
        Some(&cancellable),
        move |result| {
            if let Err(failure) = validate_menu_request(&request) {
                (request.callback)(Err(failure));
                return;
            }
            if !result.is_ok_and(|reply| reply.get::<(bool,)>().is_some()) {
                (request.callback)(Err(error(
                    "tray_menu_unavailable",
                    "The tray menu could not be prepared",
                )));
                return;
            }
            fetch_menu_layout(request);
        },
    );
}

fn fetch_menu_layout(request: MenuRequest) {
    let serial = match validate_menu_request(&request) {
        Ok((_, serial)) => serial,
        Err(failure) => {
            (request.callback)(Err(failure));
            return;
        }
    };
    let connection = request.item.connection.clone();
    let owner = request.item.binding.owner.clone();
    let cancellable = request.item.cancellable.clone();
    let path = request.path.clone();
    let params = (0_i32, -1_i32, Vec::<String>::new()).to_variant();
    connection.call(Some(&owner), &path, MENU_INTERFACE, "GetLayout", Some(&params),
        None, gio::DBusCallFlags::NONE, CALL_TIMEOUT, Some(&cancellable), move |result| {
            let (state, current) = match validate_menu_request(&request) {
                Ok(current) => current,
                Err(failure) => { (request.callback)(Err(failure)); return; }
            };
            if current != serial {
                (request.callback)(Err(error("stale_tray_menu", "The tray menu changed while its layout was being read")));
                return;
            }
            let layout = result.map_err(|_| error("tray_menu_unavailable", "The tray menu layout could not be read"))
                .and_then(|reply| parse_menu(&reply));
            match layout {
                Ok(layout) => {
                    let result = json!({"itemId": request.item.id, "revision": layout.revision, "root": layout.root});
                    state.borrow_mut().items.get_mut(&request.item.id).unwrap().menu = Some(layout);
                    (request.callback)(Ok(result));
                }
                Err(failure) => (request.callback)(Err(failure)),
            }
        });
}

fn action_completion(
    weak: &Weak<RefCell<State>>,
    item: &ItemHandle,
    result: Result<glib::Variant, glib::Error>,
) -> Result<(), NativeMethodError> {
    let state = weak
        .upgrade()
        .ok_or_else(|| error("tray_unavailable", "The tray controller has stopped"))?;
    state.borrow().validate_handle(item)?;
    match result {
        Ok(reply) if reply.type_().as_str() == "()" => Ok(()),
        _ => Err(error(
            "tray_action_failed",
            "The tray item rejected the requested action",
        )),
    }
}

fn event_timestamp() -> u32 {
    (glib::monotonic_time() / 1000) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watcher_session() -> WatcherSession {
        WatcherSession {
            owner: ":1.100".into(),
            epoch: 1,
            cancellable: gio::Cancellable::new(),
            subscriptions: Vec::new(),
            desired: BTreeMap::new(),
            joining: false,
            joined: true,
            list_inflight: false,
            list_again: false,
            list_serial: 0,
        }
    }

    fn desired(state: &mut State, service: &str) -> String {
        let spec = RegistrationSpec::parse(service, None).unwrap();
        let canonical = spec.canonical();
        state.watcher.as_mut().unwrap().desired.insert(
            canonical.clone(),
            DesiredItem {
                spec,
                token: 1,
                bound_id: None,
            },
        );
        canonical
    }

    fn node(id: i32, props: Properties, children: Vec<glib::Variant>) -> glib::Variant {
        (id, props, children).to_variant()
    }

    fn layout(revision: u32, children: Vec<glib::Variant>) -> glib::Variant {
        glib::Variant::tuple_from_iter([
            revision.to_variant(),
            node(0, Properties::new(), children),
        ])
    }

    #[test]
    fn native_registration_forms_cannot_become_arbitrary_destinations() {
        let default = RegistrationSpec::parse("org.example.Tray", None).unwrap();
        assert_eq!(default.destination, "org.example.Tray");
        assert_eq!(default.path, ITEM_PATH);
        let sender_path = RegistrationSpec::parse("/Items/One", Some(":1.42")).unwrap();
        assert_eq!(sender_path.canonical(), ":1.42/Items/One");
        assert_eq!(
            RegistrationSpec::parse("org.example.Tray/Items/Two", None)
                .unwrap()
                .path,
            "/Items/Two"
        );
        for invalid in [
            "",
            "not-a-bus-name",
            "https://example.org/item",
            "org.example.Tray/../Item",
            "org.example.Tray/Bad-Path",
        ] {
            assert!(RegistrationSpec::parse(invalid, Some(":1.42")).is_none());
        }
        assert!(RegistrationSpec::parse("/StatusNotifierItem", None).is_none());
    }

    #[test]
    fn aliases_share_an_opaque_owner_bound_item_until_the_last_unregister() {
        let mut state = State::new();
        state.watcher = Some(watcher_session());
        let first = desired(&mut state, "org.example.Tray/Item");
        let second = desired(&mut state, ":1.42/Item");
        let binding = Binding {
            owner: ":1.42".into(),
            path: "/Item".into(),
        };
        let (id, added) = state.bind(&first, binding.clone());
        assert!(added);
        let (alias_id, added) = state.bind(&second, binding.clone());
        assert!(!added);
        assert_eq!(id, alias_id);
        assert!(!id.contains(&binding.owner));
        assert!(!id.contains(&binding.path));
        let cancelled = state.items[&id].cancellable.clone();
        state.forget(&first);
        assert!(state.items.contains_key(&id));
        assert!(!cancelled.is_cancelled());
        state.forget(&second);
        assert!(!state.items.contains_key(&id));
        assert!(!state.bindings.contains_key(&binding));
        assert!(cancelled.is_cancelled());
        let replacement = desired(&mut state, "org.example.Tray/Item");
        let (replacement_id, _) = state.bind(
            &replacement,
            Binding {
                owner: ":1.43".into(),
                path: "/Item".into(),
            },
        );
        assert_ne!(id, replacement_id);
    }

    #[test]
    fn service_owner_loss_cancels_actions_and_removes_watcher_advertisements() {
        let state = Rc::new(RefCell::new(State::new()));
        let canonical;
        let item_id;
        let cancellable;
        {
            let mut inner = state.borrow_mut();
            inner.watcher = Some(watcher_session());
            inner.owned_watcher = Some(OwnedWatcher::default());
            canonical = desired(&mut inner, "org.example.Tray/Item");
            (item_id, _) = inner.bind(
                &canonical,
                Binding {
                    owner: ":1.42".into(),
                    path: "/Item".into(),
                },
            );
            cancellable = inner.items[&item_id].cancellable.clone();
            inner.owned_watcher.as_mut().unwrap().items.insert(
                canonical.clone(),
                WatcherRecord {
                    spec: RegistrationSpec::parse(&canonical, None).unwrap(),
                    owner: ":1.42".into(),
                },
            );
            inner
                .owned_watcher
                .as_mut()
                .unwrap()
                .hosts
                .insert("org.example.Host".into(), ":1.42".into());
        }
        owner_changed(&state, ":1.42", ":1.42", "");
        assert!(!state.borrow().items.contains_key(&item_id));
        assert!(
            !state
                .borrow()
                .watcher
                .as_ref()
                .unwrap()
                .desired
                .contains_key(&canonical)
        );
        assert!(
            !state
                .borrow()
                .owned_watcher
                .as_ref()
                .unwrap()
                .items
                .contains_key(&canonical)
        );
        assert!(
            !state
                .borrow()
                .owned_watcher
                .as_ref()
                .unwrap()
                .hosts
                .contains_key("org.example.Host")
        );
        assert!(cancellable.is_cancelled());
    }

    #[test]
    fn unknown_item_callbacks_are_reentrant_and_shutdown_is_idempotent() {
        let state = Rc::new(RefCell::new(State::new()));
        state.borrow_mut().available = true;
        let controller = TrayController { state };
        let callback_controller = controller.clone();
        let observed = Rc::new(RefCell::new(None));
        let callback_observed = observed.clone();
        controller.get_menu("org.example.Attacker/Unchecked".into(), 0, move |result| {
            *callback_observed.borrow_mut() = Some(result.unwrap_err().code);
            assert_eq!(callback_controller.snapshot()["available"], true);
            callback_controller.shutdown();
            callback_controller.shutdown();
        });
        assert_eq!(*observed.borrow(), Some("unknown_tray_item"));
        assert_eq!(controller.snapshot()["available"], false);
        assert_eq!(
            controller
                .state
                .borrow()
                .handle("anything")
                .err()
                .unwrap()
                .code,
            "tray_unavailable"
        );
    }

    #[test]
    fn menu_labels_preserve_literal_text_mnemonics_and_control_normalization() {
        let label = "_Open <literal> &lt;literal&gt; &amp;value __name\x1b\t\n";
        let expected = "Open <literal> &lt;literal&gt; &amp;value _name\t\n";
        let menu = parse_menu(&layout(
            7,
            vec![node(
                1,
                Properties::from([("label".into(), label.to_variant())]),
                Vec::new(),
            )],
        ))
        .unwrap();
        let result = serde_json::to_value(&menu.root).unwrap();
        assert_eq!(result["children"][0]["label"], expected);
    }

    #[test]
    fn recursive_menu_preserves_real_revision_toggles_and_action_boundaries() {
        let toggle = node(
            1,
            Properties::from([
                ("label".into(), "_Toggle__value".to_variant()),
                ("toggle-type".into(), "checkmark".to_variant()),
                ("toggle-state".into(), 1_i32.to_variant()),
            ]),
            Vec::new(),
        );
        let separator = node(
            2,
            Properties::from([("type".into(), "separator".to_variant())]),
            Vec::new(),
        );
        let disabled = node(
            3,
            Properties::from([("enabled".into(), false.to_variant())]),
            Vec::new(),
        );
        let hidden_parent = node(
            4,
            Properties::from([("visible".into(), false.to_variant())]),
            vec![node(5, Properties::new(), Vec::new())],
        );
        let lazy_submenu = node(
            6,
            Properties::from([
                ("label".into(), "Lazy".to_variant()),
                ("children-display".into(), "submenu".to_variant()),
            ]),
            Vec::new(),
        );
        let menu = parse_menu(&layout(
            7,
            vec![toggle, separator, disabled, hidden_parent, lazy_submenu],
        ))
        .unwrap();
        assert_eq!(menu.revision, 7);
        assert_eq!(menu.root.children[0].label, "Toggle_value");
        assert_eq!(
            menu.root.children[0].toggle_type.as_deref(),
            Some("checkmark")
        );
        assert_eq!(menu.root.children[0].toggle_state, 1);
        assert!(menu.root.children[4].has_submenu);
        assert!(menu.validate_activation(1, 7).is_ok());
        assert_eq!(
            menu.validate_activation(1, 6).unwrap_err().code,
            "stale_tray_menu"
        );
        assert_eq!(
            menu.validate_activation(99, 7).unwrap_err().code,
            "unknown_tray_menu_item"
        );
        assert_eq!(
            menu.validate_activation(0, 7).unwrap_err().code,
            "tray_menu_item_not_activatable"
        );
        assert_eq!(
            menu.validate_activation(2, 7).unwrap_err().code,
            "tray_menu_item_not_activatable"
        );
        assert_eq!(
            menu.validate_activation(3, 7).unwrap_err().code,
            "tray_menu_item_disabled"
        );
        assert_eq!(
            menu.validate_activation(5, 7).unwrap_err().code,
            "tray_menu_item_disabled"
        );
        assert_eq!(
            menu.validate_activation(6, 7).unwrap_err().code,
            "tray_menu_item_not_activatable"
        );
        assert!(menu.validate_submenu(6).is_ok());
        assert_eq!(
            menu.validate_submenu(1).unwrap_err().code,
            "tray_menu_item_not_activatable"
        );
        assert_eq!(
            menu.validate_submenu(4).unwrap_err().code,
            "tray_menu_item_disabled"
        );
        assert_eq!(
            menu.validate_submenu(99).unwrap_err().code,
            "unknown_tray_menu_item"
        );
    }

    #[test]
    fn duplicate_ids_invalid_property_types_and_deep_menus_are_rejected() {
        let duplicate = layout(
            1,
            vec![
                node(1, Properties::new(), Vec::new()),
                node(1, Properties::new(), Vec::new()),
            ],
        );
        assert_eq!(
            parse_menu(&duplicate).err().unwrap().code,
            "tray_menu_unavailable"
        );
        let malformed = layout(
            1,
            vec![node(
                1,
                Properties::from([("enabled".into(), "false".to_variant())]),
                Vec::new(),
            )],
        );
        assert_eq!(
            parse_menu(&malformed).err().unwrap().code,
            "tray_menu_unavailable"
        );
        let mut deep = node(100, Properties::new(), Vec::new());
        for id in (1..=MAX_MENU_DEPTH as i32 + 1).rev() {
            deep = node(id, Properties::new(), vec![deep]);
        }
        assert_eq!(
            parse_menu(&layout(1, vec![deep])).err().unwrap().code,
            "tray_menu_unavailable"
        );
    }

    #[test]
    fn layout_signals_invalidate_even_when_the_real_revision_does_not_change() {
        let mut item = Item::new(Binding {
            owner: ":1.42".into(),
            path: "/Item".into(),
        });
        item.menu =
            Some(parse_menu(&layout(7, vec![node(1, Properties::new(), Vec::new())])).unwrap());
        let path = glib::variant::ObjectPath::try_from("/Menu").unwrap();
        item.properties.insert("Menu".into(), path.to_variant());
        item.menu_path = exported_menu_path(&item.properties).map(str::to_owned);
        let old_serial = item.menu_serial;
        item.invalidate_menu();
        assert!(item.menu.is_none());
        assert_ne!(item.menu_serial, old_serial);
        assert_eq!(item.menu_path.as_deref(), Some("/Menu"));
        item.invalidate_menu_path();
        assert_eq!(item.menu_path, None);
        assert_eq!(item.menu_path_serial, 1);
    }

    #[test]
    fn observed_properties_never_expose_paths_or_markup_and_decode_argb() {
        let tooltip = (
            "ignored",
            Vec::<(i32, i32, Vec<u8>)>::new(),
            "<b>Title</b>",
            "A &amp; B<br/>plain",
        )
            .to_variant();
        let menu_path = glib::variant::ObjectPath::try_from("/Menu").unwrap();
        let props = Properties::from([
            ("Status".into(), "NeedsAttention".to_variant()),
            ("Category".into(), "Communications".to_variant()),
            ("Title".into(), "Example".to_variant()),
            ("ItemIsMenu".into(), true.to_variant()),
            ("Menu".into(), menu_path.to_variant()),
            ("ToolTip".into(), tooltip),
            ("IconName".into(), "/tmp/untrusted.png".to_variant()),
            ("IconThemePath".into(), "/tmp/unchecked-theme".to_variant()),
            (
                "IconPixmap".into(),
                vec![(1_i32, 1_i32, vec![255_u8, 17, 34, 51])].to_variant(),
            ),
        ]);
        let view = item_view("opaque", &props, &Properties::new(), None).unwrap();
        assert!(!view.tooltip.contains("<b>"));
        assert!(!view.tooltip.contains("<br/>"));
        assert!(view.has_menu);
        assert!(view.is_menu);
        assert_eq!(view.icon.as_ref().unwrap().rgba, "ESIz/w==");
        let serialized = serde_json::to_value(&view).unwrap();
        assert!(serialized.get("Menu").is_none());
        assert!(serialized.get("IconThemePath").is_none());
        assert!(serialized.to_string().find("/tmp/").is_none());
        assert!(safe_theme_icon("https://example.org/icon.png").is_none());
        assert!(safe_theme_icon("../icon").is_none());
        let invalid_path = Properties::from([("Menu".into(), "/Menu".to_variant())]);
        assert!(exported_menu_path(&invalid_path).is_none());
    }
}
