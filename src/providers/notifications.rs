use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Duration,
};

use gio::prelude::*;
use glib::variant::ToVariant;
use gtk4::gio;
use serde_json::{Value, json};

use super::{
    Provider,
    icons::{self, Icon},
};
use crate::messages::NativeMethodError;

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const DBUS_NAME: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DO_NOT_QUEUE: u32 = 4;
const HISTORY_LIMIT: usize = 100;
const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_APP_NAME_BYTES: usize = 1024;
const MAX_SUMMARY_BYTES: usize = 4096;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_ACTIONS: usize = 32;
const MAX_ACTION_KEY_CHARS: usize = 256;
const MAX_ACTION_LABEL_BYTES: usize = 1024;
const MAX_HINTS: usize = 64;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSED_EXPIRED: u32 = 1;
const CLOSED_DISMISSED: u32 = 2;
const CLOSED_BY_CLIENT: u32 = 3;
const CLOSED_RESERVED: u32 = 4;

const INTERFACE_XML: &str = r#"
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="GetCapabilities">
      <arg name="capabilities" type="as" direction="out"/>
    </method>
    <method name="Notify">
      <arg name="app_name" type="s" direction="in"/>
      <arg name="replaces_id" type="u" direction="in"/>
      <arg name="app_icon" type="s" direction="in"/>
      <arg name="summary" type="s" direction="in"/>
      <arg name="body" type="s" direction="in"/>
      <arg name="actions" type="as" direction="in"/>
      <arg name="hints" type="a{sv}" direction="in"/>
      <arg name="expire_timeout" type="i" direction="in"/>
      <arg name="id" type="u" direction="out"/>
    </method>
    <method name="CloseNotification">
      <arg name="id" type="u" direction="in"/>
    </method>
    <method name="GetServerInformation">
      <arg name="name" type="s" direction="out"/>
      <arg name="vendor" type="s" direction="out"/>
      <arg name="version" type="s" direction="out"/>
      <arg name="spec_version" type="s" direction="out"/>
    </method>
    <signal name="NotificationClosed">
      <arg name="id" type="u"/>
      <arg name="reason" type="u"/>
    </signal>
    <signal name="ActionInvoked">
      <arg name="id" type="u"/>
      <arg name="action_key" type="s"/>
    </signal>
  </interface>
</node>
"#;

#[derive(Clone)]
pub(super) struct NotificationsController {
    inner: Rc<NotificationsInner>,
}

struct NotificationsInner {
    state: RefCell<NotificationState>,
    resources: RefCell<ServerResources>,
}

#[derive(Default)]
struct ServerResources {
    connection: Option<gio::DBusConnection>,
    registration: Option<gio::RegistrationId>,
    cancellable: gio::Cancellable,
    closed_handler: Option<glib::SignalHandlerId>,
    timers: BTreeMap<u32, ExpiryTimer>,
}

struct ExpiryTimer {
    token: RecordToken,
    source: glib::SourceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecordToken {
    id: u32,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Availability {
    Starting,
    Ready,
    Unavailable(&'static str),
    Stopped,
}

impl Availability {
    fn reason(self) -> &'static str {
        match self {
            Self::Starting => "Notification service is starting",
            Self::Ready => "",
            Self::Unavailable(reason) => reason,
            Self::Stopped => "Notification service has shut down",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
struct NotificationAction {
    key: String,
    label: String,
}

struct NotificationInput {
    replaces_id: u32,
    app_name: String,
    summary: String,
    body: String,
    actions: Vec<NotificationAction>,
    urgency: Urgency,
    resident: bool,
    icon: Option<Icon>,
    timeout: Option<Duration>,
}

struct NotificationRecord {
    token: RecordToken,
    sender: String,
    app_name: String,
    summary: String,
    body: String,
    created_at: i64,
    active: bool,
    read: bool,
    urgency: Urgency,
    resident: bool,
    icon: Option<Icon>,
    actions: Vec<NotificationAction>,
}

#[derive(Debug, PartialEq, Eq)]
struct CloseEvent {
    token: RecordToken,
    sender: String,
    reason: u32,
}

struct RemovedRecord {
    token: RecordToken,
    closed: Option<CloseEvent>,
}

struct NotifyUpdate {
    token: RecordToken,
    previous: Option<RecordToken>,
    evicted: Option<RemovedRecord>,
    timeout: Option<Duration>,
}

struct BoundAction {
    token: RecordToken,
    sender: String,
    key: String,
}

struct NotificationState {
    availability: Availability,
    records: VecDeque<NotificationRecord>,
    next_id: u32,
    next_generation: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum ServerError {
    Unavailable,
    InvalidArgs(&'static str),
    AccessDenied,
    UnknownNotification,
    LimitsExceeded,
}

impl ServerError {
    fn reply(self, invocation: gio::DBusMethodInvocation) {
        let (name, message) = match self {
            Self::Unavailable => (
                "org.freedesktop.DBus.Error.Failed",
                "Notification server is unavailable",
            ),
            Self::InvalidArgs(message) => ("org.freedesktop.DBus.Error.InvalidArgs", message),
            Self::AccessDenied => (
                "org.freedesktop.DBus.Error.AccessDenied",
                "Notification belongs to another client",
            ),
            // The specification requires an empty error for an invalidated ID.
            Self::UnknownNotification => ("org.freedesktop.DBus.Error.InvalidArgs", ""),
            Self::LimitsExceeded => (
                "org.freedesktop.DBus.Error.LimitsExceeded",
                "Notification identifiers are exhausted",
            ),
        };
        invocation.return_dbus_error(name, message);
    }
}

impl NotificationsController {
    pub(super) fn new() -> Self {
        let inner = Rc::new(NotificationsInner {
            state: RefCell::new(NotificationState::new()),
            resources: RefCell::new(ServerResources::default()),
        });
        let weak = Rc::downgrade(&inner);
        let cancellable = inner.resources.borrow().cancellable.clone();
        gio::bus_get(gio::BusType::Session, Some(&cancellable), move |result| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if inner.state.borrow().availability != Availability::Starting {
                return;
            }
            match result {
                Ok(connection) => NotificationsInner::register(&inner, connection),
                Err(_) => inner.unavailable("Session notification bus is unavailable"),
            }
        });
        Self { inner }
    }

    pub(super) fn snapshot(&self) -> Value {
        self.inner.state.borrow().snapshot()
    }

    pub(super) fn get_notifications(&self) -> Result<Value, NativeMethodError> {
        self.inner.state.borrow().get_notifications()
    }

    pub(super) fn dismiss(&self, notification_id: u32) -> Result<Value, NativeMethodError> {
        let removed = self.inner.state.borrow_mut().dismiss(notification_id)?;
        self.inner.cancel_timer(removed.token);
        if let Some(closed) = removed.closed {
            self.inner.emit_close(&closed)?;
        }
        Ok(self.snapshot())
    }

    pub(super) fn invoke_action(
        &self,
        notification_id: u32,
        action_key: String,
    ) -> Result<Value, NativeMethodError> {
        let action = self
            .inner
            .state
            .borrow()
            .bound_action(notification_id, &action_key)?;
        let connection = self.inner.connection()?;
        connection
            .emit_signal(
                Some(&action.sender),
                OBJECT_PATH,
                BUS_NAME,
                "ActionInvoked",
                Some(&(action.token.id, action.key.as_str()).to_variant()),
            )
            .map_err(|_| signal_error())?;
        // Queue the real action before closing a non-resident notification. No
        // RefCell borrow spans GIO calls, and the token guards a replaced record.
        let closed = self.inner.state.borrow_mut().finish_action(action.token)?;
        if let Some(closed) = closed {
            self.inner.cancel_timer(closed.token);
            self.inner.emit_close(&closed)?;
        }
        Ok(self.snapshot())
    }

    pub(super) fn mark_all_read(&self) -> Result<Value, NativeMethodError> {
        let mut state = self.inner.state.borrow_mut();
        state.ensure_available()?;
        for record in &mut state.records {
            record.read = true;
        }
        Ok(state.snapshot())
    }

    pub(super) fn shutdown(&self) {
        self.inner.state.borrow_mut().shutdown();
        self.inner.release_resources();
    }
}

impl Provider for NotificationsController {
    fn name(&self) -> &'static str {
        "notifications"
    }

    fn snapshot(&self) -> Value {
        NotificationsController::snapshot(self)
    }
}

impl NotificationsInner {
    fn register(inner: &Rc<Self>, connection: gio::DBusConnection) {
        if inner.state.borrow().availability != Availability::Starting {
            return;
        }
        let interface = gio::DBusNodeInfo::for_xml(INTERFACE_XML)
            .ok()
            .and_then(|node| node.lookup_interface(BUS_NAME));
        let Some(interface) = interface else {
            inner.unavailable("Notification service interface is unavailable");
            return;
        };
        let weak = Rc::downgrade(inner);
        let registration = connection
            .register_object(OBJECT_PATH, &interface)
            .method_call(
                move |_, sender, path, interface, method, parameters, invocation| {
                    let Some(inner) = weak.upgrade() else {
                        ServerError::Unavailable.reply(invocation);
                        return;
                    };
                    if path != OBJECT_PATH || interface != Some(BUS_NAME) {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.UnknownInterface",
                            "Unknown notification interface",
                        );
                        return;
                    }
                    NotificationsInner::method_call(&inner, sender, method, parameters, invocation);
                },
            )
            .build();
        match registration {
            Ok(registration) => {
                let weak = Rc::downgrade(inner);
                let closed_handler = connection.connect_local("closed", false, move |_| {
                    if let Some(inner) = weak.upgrade() {
                        inner.unavailable("Session notification bus is unavailable");
                    }
                    None
                });
                {
                    let mut resources = inner.resources.borrow_mut();
                    resources.connection = Some(connection.clone());
                    resources.registration = Some(registration);
                    resources.closed_handler = Some(closed_handler);
                }
                let weak = Rc::downgrade(inner);
                let cancellable = inner.resources.borrow().cancellable.clone();
                connection.call(
                    Some(DBUS_NAME),
                    DBUS_PATH,
                    DBUS_NAME,
                    "RequestName",
                    Some(&(BUS_NAME, DO_NOT_QUEUE).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                    Some(&cancellable),
                    move |result| {
                        let Some(inner) = weak.upgrade() else {
                            return;
                        };
                        if inner.state.borrow().availability != Availability::Starting {
                            return;
                        }
                        match result.ok().and_then(|reply| reply.get::<(u32,)>()) {
                            Some((1 | 4,)) => {
                                inner.state.borrow_mut().availability = Availability::Ready
                            }
                            Some((3,)) => inner.unavailable(
                                "Another notification daemon already owns the service",
                            ),
                            _ => inner.unavailable("Notification service ownership is unavailable"),
                        }
                    },
                );
            }
            Err(_) => inner.unavailable("Notification service could not be registered"),
        }
    }

    fn method_call(
        inner: &Rc<Self>,
        sender: Option<&str>,
        method: &str,
        parameters: glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        if inner.state.borrow().availability != Availability::Ready {
            ServerError::Unavailable.reply(invocation);
            return;
        }
        match method {
            "GetCapabilities" if parameters.type_().as_str() == "()" => {
                invocation.return_value(Some(
                    &(vec!["body", "actions", "icon-static"],).to_variant(),
                ));
            }
            "GetServerInformation" if parameters.type_().as_str() == "()" => {
                invocation.return_value(Some(
                    &(
                        "HTML Desktop Shell",
                        "HTML Desktop Shell",
                        env!("CARGO_PKG_VERSION"),
                        "1.2",
                    )
                        .to_variant(),
                ));
            }
            "Notify" => {
                let result = unique_sender(sender).and_then(|sender| {
                    let input = parse_notify(&parameters)?;
                    inner
                        .state
                        .borrow_mut()
                        .notify(sender, input, glib::real_time() / 1000)
                });
                match result {
                    Ok(update) => {
                        if let Some(previous) = update.previous {
                            inner.cancel_timer(previous);
                        }
                        if let Some(evicted) = update.evicted {
                            inner.cancel_timer(evicted.token);
                            if let Some(closed) = evicted.closed {
                                let _ = inner.emit_close(&closed);
                            }
                        }
                        Self::arm_timer(inner, update.token, update.timeout);
                        invocation.return_value(Some(&(update.token.id,).to_variant()));
                    }
                    Err(error) => error.reply(invocation),
                }
            }
            "CloseNotification" => {
                let result = unique_sender(sender).and_then(|sender| {
                    if parameters.type_().as_str() != "(u)" {
                        return Err(ServerError::InvalidArgs(
                            "CloseNotification requires a uint32 identifier",
                        ));
                    }
                    let Some((id,)) = parameters.get::<(u32,)>() else {
                        return Err(ServerError::InvalidArgs(
                            "CloseNotification requires a uint32 identifier",
                        ));
                    };
                    inner.state.borrow_mut().close_by_sender(sender, id)
                });
                match result {
                    Ok(closed) => {
                        inner.cancel_timer(closed.token);
                        if inner.emit_close(&closed).is_err() {
                            ServerError::Unavailable.reply(invocation);
                        } else {
                            invocation.return_value(None);
                        }
                    }
                    Err(error) => error.reply(invocation),
                }
            }
            "GetCapabilities" | "GetServerInformation" => {
                ServerError::InvalidArgs("This notification method takes no parameters")
                    .reply(invocation);
            }
            _ => invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.UnknownMethod",
                "Unknown notification method",
            ),
        }
    }

    fn connection(&self) -> Result<gio::DBusConnection, NativeMethodError> {
        self.state.borrow().ensure_available()?;
        self.resources.borrow().connection.clone().ok_or_else(|| {
            NativeMethodError::new(
                "notifications_unavailable",
                "Notification server is unavailable",
            )
        })
    }

    fn emit_close(&self, closed: &CloseEvent) -> Result<(), NativeMethodError> {
        self.connection()?
            .emit_signal(
                Some(&closed.sender),
                OBJECT_PATH,
                BUS_NAME,
                "NotificationClosed",
                Some(&(closed.token.id, closed.reason).to_variant()),
            )
            .map_err(|_| signal_error())
    }

    fn arm_timer(inner: &Rc<Self>, token: RecordToken, timeout: Option<Duration>) {
        let Some(timeout) = timeout else {
            return;
        };
        if !inner.state.borrow().is_current_active(token) {
            return;
        }
        let weak = Rc::downgrade(inner);
        let source = glib::timeout_add_local_once(timeout, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            {
                let mut resources = inner.resources.borrow_mut();
                if !resources
                    .timers
                    .get(&token.id)
                    .is_some_and(|timer| timer.token == token)
                {
                    return;
                }
                // This source is already executing; dropping its ID is correct.
                // Removing it with SourceId::remove would double-remove it.
                resources.timers.remove(&token.id);
            }
            let closed = inner.state.borrow_mut().expire(token);
            if let Some(closed) = closed {
                let _ = inner.emit_close(&closed);
            }
        });
        let previous = inner
            .resources
            .borrow_mut()
            .timers
            .insert(token.id, ExpiryTimer { token, source });
        if let Some(previous) = previous {
            previous.source.remove();
        }
    }

    fn cancel_timer(&self, token: RecordToken) {
        let timer = {
            let mut resources = self.resources.borrow_mut();
            if resources
                .timers
                .get(&token.id)
                .is_some_and(|timer| timer.token == token)
            {
                resources.timers.remove(&token.id)
            } else {
                None
            }
        };
        if let Some(timer) = timer {
            timer.source.remove();
        }
    }

    fn unavailable(&self, reason: &'static str) {
        {
            let mut state = self.state.borrow_mut();
            if matches!(
                state.availability,
                Availability::Stopped | Availability::Unavailable(_)
            ) {
                return;
            }
            state.availability = Availability::Unavailable(reason);
            for record in &mut state.records {
                record.active = false;
            }
        }
        self.release_resources();
    }

    fn release_resources(&self) {
        // Unregistration/unowning may schedule callbacks. They cannot observe a
        // borrowed RefCell, revive the service, or keep the controller alive.
        let resources = std::mem::take(&mut *self.resources.borrow_mut());
        resources.release();
    }
}

impl ServerResources {
    fn release(self) {
        self.cancellable.cancel();
        for timer in self.timers.into_values() {
            timer.source.remove();
        }
        if let Some(connection) = self.connection {
            if let Some(handler) = self.closed_handler {
                connection.disconnect(handler);
            }
            if let Some(registration) = self.registration {
                let _ = connection.unregister_object(registration);
            }
            if !connection.is_closed() {
                // This follows any queued RequestName on the same connection,
                // including shutdown during acquisition. NOT_OWNER is normal
                // after a DO_NOT_QUEUE conflict and leaves the other daemon alone.
                connection.call(
                    Some(DBUS_NAME),
                    DBUS_PATH,
                    DBUS_NAME,
                    "ReleaseName",
                    Some(&(BUS_NAME,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                    gio::Cancellable::NONE,
                    |_| {},
                );
            }
        }
        // GIO's session-bus connection is shared with other providers. Do not
        // close it when releasing only this service's resources.
    }
}

impl Drop for NotificationsInner {
    fn drop(&mut self) {
        self.state.get_mut().shutdown();
        std::mem::take(self.resources.get_mut()).release();
    }
}

impl NotificationState {
    fn new() -> Self {
        Self {
            availability: Availability::Starting,
            records: VecDeque::with_capacity(HISTORY_LIMIT),
            next_id: 1,
            next_generation: 1,
        }
    }

    fn ensure_available(&self) -> Result<(), NativeMethodError> {
        if self.availability == Availability::Ready {
            Ok(())
        } else {
            Err(NativeMethodError::new(
                "notifications_unavailable",
                self.availability.reason(),
            ))
        }
    }

    fn snapshot(&self) -> Value {
        if self.availability != Availability::Ready {
            return json!({ "available": false, "reason": self.availability.reason() });
        }
        let unread = self.records.iter().filter(|record| !record.read).count();
        let active = self.records.iter().filter(|record| record.active).count();
        // This is deliberately count-only, including for Diagnostics getState.
        json!({
            "available": true,
            "unreadCount": unread,
            "activeCount": active,
            "historyCount": self.records.len(),
        })
    }

    fn get_notifications(&self) -> Result<Value, NativeMethodError> {
        self.ensure_available()?;
        let items: Vec<Value> = self
            .records
            .iter()
            .rev()
            .map(|record| {
                json!({
                    "id": record.token.id,
                    "appName": record.app_name,
                    "summary": record.summary,
                    "body": record.body,
                    "createdAt": record.created_at,
                    "active": record.active,
                    "read": record.read,
                    "urgency": record.urgency.as_str(),
                    "icon": record.icon,
                    "actions": record.actions,
                })
            })
            .collect();
        Ok(json!({ "items": items }))
    }

    fn notify(
        &mut self,
        sender: &str,
        input: NotificationInput,
        created_at: i64,
    ) -> Result<NotifyUpdate, ServerError> {
        if self.availability != Availability::Ready {
            return Err(ServerError::Unavailable);
        }
        let replace_index = self
            .records
            .iter()
            .position(|record| record.token.id == input.replaces_id && record.active);
        if let Some(index) = replace_index
            && self.records[index].sender != sender
        {
            return Err(ServerError::AccessDenied);
        }
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .ok_or(ServerError::LimitsExceeded)?;
        let previous = replace_index.map(|index| self.records[index].token);
        // Closed/unknown IDs are invalidated, not resurrected or reused. Only a
        // current same-sender replacement retains its allocated identifier.
        let id = match previous {
            Some(previous) => previous.id,
            None => self.allocate_id(),
        };
        if let Some(index) = replace_index {
            self.records.remove(index);
        }
        let evicted = if self.records.len() == HISTORY_LIMIT {
            // Prefer pruning already-closed history over closing a sticky item.
            let index = self
                .records
                .iter()
                .position(|record| !record.active)
                .unwrap_or(0);
            self.records
                .remove(index)
                .map(|record| remove_record(record, CLOSED_RESERVED))
        } else {
            None
        };
        let token = RecordToken { id, generation };
        let timeout = input.timeout;
        self.records.push_back(NotificationRecord {
            token,
            sender: sender.to_owned(),
            app_name: input.app_name,
            summary: input.summary,
            body: input.body,
            created_at,
            active: true,
            read: false,
            urgency: input.urgency,
            resident: input.resident,
            icon: input.icon,
            actions: input.actions,
        });
        Ok(NotifyUpdate {
            token,
            previous,
            evicted,
            timeout,
        })
    }

    fn allocate_id(&mut self) -> u32 {
        loop {
            let id = self.next_id;
            self.next_id = id.wrapping_add(1).max(1);
            if id != 0 && !self.records.iter().any(|record| record.token.id == id) {
                return id;
            }
        }
    }

    fn is_current_active(&self, token: RecordToken) -> bool {
        self.availability == Availability::Ready
            && self
                .records
                .iter()
                .any(|record| record.token == token && record.active)
    }

    fn expire(&mut self, token: RecordToken) -> Option<CloseEvent> {
        if self.availability != Availability::Ready {
            return None;
        }
        let record = self
            .records
            .iter_mut()
            .find(|record| record.token == token && record.active)?;
        record.active = false;
        Some(close_event(record, CLOSED_EXPIRED))
    }

    fn close_by_sender(&mut self, sender: &str, id: u32) -> Result<CloseEvent, ServerError> {
        if self.availability != Availability::Ready {
            return Err(ServerError::Unavailable);
        }
        let record = self
            .records
            .iter_mut()
            .find(|record| record.token.id == id && record.active)
            .ok_or(ServerError::UnknownNotification)?;
        if record.sender != sender {
            return Err(ServerError::AccessDenied);
        }
        record.active = false;
        Ok(close_event(record, CLOSED_BY_CLIENT))
    }

    fn dismiss(&mut self, id: u32) -> Result<RemovedRecord, NativeMethodError> {
        self.ensure_available()?;
        let index = self
            .records
            .iter()
            .position(|record| record.token.id == id)
            .ok_or_else(unknown_notification)?;
        let record = self
            .records
            .remove(index)
            .ok_or_else(unknown_notification)?;
        Ok(remove_record(record, CLOSED_DISMISSED))
    }

    fn bound_action(&self, id: u32, key: &str) -> Result<BoundAction, NativeMethodError> {
        self.ensure_available()?;
        let record = self
            .records
            .iter()
            .find(|record| record.token.id == id && record.active)
            .ok_or_else(unknown_notification)?;
        let action = record
            .actions
            .iter()
            .find(|action| action.key == key)
            .ok_or_else(|| {
                NativeMethodError::new(
                    "unknown_notification_action",
                    "Action is not in the active notification",
                )
            })?;
        Ok(BoundAction {
            token: record.token,
            sender: record.sender.clone(),
            key: action.key.clone(),
        })
    }

    fn finish_action(
        &mut self,
        token: RecordToken,
    ) -> Result<Option<CloseEvent>, NativeMethodError> {
        self.ensure_available()?;
        let record = self
            .records
            .iter_mut()
            .find(|record| record.token == token && record.active)
            .ok_or_else(unknown_notification)?;
        record.read = true;
        if record.resident {
            Ok(None)
        } else {
            record.active = false;
            Ok(Some(close_event(record, CLOSED_DISMISSED)))
        }
    }

    fn shutdown(&mut self) {
        self.availability = Availability::Stopped;
        self.records.clear();
    }
}

fn close_event(record: &NotificationRecord, reason: u32) -> CloseEvent {
    CloseEvent {
        token: record.token,
        sender: record.sender.clone(),
        reason,
    }
}

fn remove_record(record: NotificationRecord, reason: u32) -> RemovedRecord {
    RemovedRecord {
        token: record.token,
        closed: record.active.then_some(CloseEvent {
            token: record.token,
            sender: record.sender,
            reason,
        }),
    }
}

fn unique_sender(sender: Option<&str>) -> Result<&str, ServerError> {
    sender
        .filter(|sender| gio::dbus_is_unique_name(sender))
        .ok_or(ServerError::AccessDenied)
}

fn unknown_notification() -> NativeMethodError {
    NativeMethodError::new(
        "unknown_notification",
        "Notification is not in the current session history",
    )
}

fn signal_error() -> NativeMethodError {
    NativeMethodError::new(
        "notification_action_failed",
        "Notification callback could not be sent",
    )
}

#[derive(Default)]
struct NotificationHints {
    urgency: Option<glib::Variant>,
    resident: Option<glib::Variant>,
    image_data: Option<glib::Variant>,
    image_path: Option<glib::Variant>,
    old_image_data: Option<glib::Variant>,
    old_image_path: Option<glib::Variant>,
    icon_data: Option<glib::Variant>,
}

impl NotificationHints {
    fn parse(hints: &glib::Variant) -> Result<Self, ServerError> {
        if hints.type_().as_str() != "a{sv}" || hints.n_children() > MAX_HINTS {
            return Err(ServerError::InvalidArgs(
                "Notification hints exceed the supported bounds",
            ));
        }
        let mut parsed = Self::default();
        for index in 0..hints.n_children() {
            let entry = notification_child(hints, index)?;
            let key = notification_child(&entry, 0)?;
            let slot = match key.str() {
                Some("urgency") => &mut parsed.urgency,
                Some("resident") => &mut parsed.resident,
                Some("image-data") => &mut parsed.image_data,
                Some("image-path") => &mut parsed.image_path,
                Some("image_data") => &mut parsed.old_image_data,
                Some("image_path") => &mut parsed.old_image_path,
                Some("icon_data") => &mut parsed.icon_data,
                _ => continue,
            };
            // Unknown hints (including files, sounds, URIs, coordinates, process
            // IDs and desktop entries) are never interpreted or retained.
            let value = notification_child(&entry, 1)?;
            *slot = if value.type_().as_str() == "v" {
                value.as_variant()
            } else {
                None
            };
        }
        Ok(parsed)
    }

    fn urgency(&self) -> Urgency {
        match self.urgency.as_ref().and_then(|value| value.get::<u8>()) {
            Some(0) => Urgency::Low,
            Some(2) => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }

    fn resident(&self) -> bool {
        self.resident
            .as_ref()
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false)
    }

    fn icon(&self, app_icon: &str) -> Option<Icon> {
        self.image_data
            .as_ref()
            .or(self.old_image_data.as_ref())
            .and_then(image_icon)
            .or_else(|| {
                self.image_path
                    .as_ref()
                    .or(self.old_image_path.as_ref())
                    .and_then(|value| value.str())
                    .and_then(named_icon)
            })
            .or_else(|| named_icon(app_icon))
            .or_else(|| self.icon_data.as_ref().and_then(image_icon))
    }
}

fn parse_notify(parameters: &glib::Variant) -> Result<NotificationInput, ServerError> {
    if parameters.type_().as_str() != "(susssasa{sv}i)" || parameters.size() > MAX_PAYLOAD_BYTES {
        return Err(ServerError::InvalidArgs(
            "Invalid or oversized notification payload",
        ));
    }
    let app_name = bounded_text(&notification_child(parameters, 0)?, MAX_APP_NAME_BYTES)?;
    let replaces_id =
        notification_child(parameters, 1)?
            .get::<u32>()
            .ok_or(ServerError::InvalidArgs(
                "Invalid notification replacement identifier",
            ))?;
    let app_icon = notification_child(parameters, 2)?;
    let summary = bounded_text(&notification_child(parameters, 3)?, MAX_SUMMARY_BYTES)?;
    let body = bounded_text(&notification_child(parameters, 4)?, MAX_BODY_BYTES)?;
    let actions = parse_actions(&notification_child(parameters, 5)?)?;
    let hints = NotificationHints::parse(&notification_child(parameters, 6)?)?;
    let urgency = hints.urgency();
    let timeout =
        notification_child(parameters, 7)?
            .get::<i32>()
            .ok_or(ServerError::InvalidArgs(
                "Invalid notification expiration timeout",
            ))?;
    let timeout = expiration(timeout, urgency)?;
    Ok(NotificationInput {
        replaces_id,
        app_name,
        summary,
        body,
        actions,
        urgency,
        resident: hints.resident(),
        icon: hints.icon(app_icon.str().unwrap_or("")),
        timeout,
    })
}

fn notification_child(value: &glib::Variant, index: usize) -> Result<glib::Variant, ServerError> {
    value
        .try_child_value(index)
        .ok_or(ServerError::InvalidArgs("Invalid notification payload"))
}

fn bounded_text(value: &glib::Variant, max_bytes: usize) -> Result<String, ServerError> {
    let text = value
        .str()
        .ok_or(ServerError::InvalidArgs("Invalid notification text"))?;
    if text.len() > max_bytes {
        return Err(ServerError::InvalidArgs(
            "Notification text exceeds the supported bounds",
        ));
    }
    // Preserve literal text, not HTML/markup. Presentation must use text nodes.
    Ok(text.to_owned())
}

fn parse_actions(value: &glib::Variant) -> Result<Vec<NotificationAction>, ServerError> {
    if value.type_().as_str() != "as"
        || !value.n_children().is_multiple_of(2)
        || value.n_children() > MAX_ACTIONS * 2
    {
        return Err(ServerError::InvalidArgs(
            "Notification actions must be bounded key-label pairs",
        ));
    }
    let mut actions: Vec<NotificationAction> = Vec::with_capacity(value.n_children() / 2);
    for index in (0..value.n_children()).step_by(2) {
        let key_value = notification_child(value, index)?;
        let key = key_value
            .str()
            .ok_or(ServerError::InvalidArgs("Invalid notification action key"))?;
        let label_value = notification_child(value, index + 1)?;
        let label = label_value.str().ok_or(ServerError::InvalidArgs(
            "Invalid notification action label",
        ))?;
        if key.is_empty()
            || key.chars().count() > MAX_ACTION_KEY_CHARS
            || label.len() > MAX_ACTION_LABEL_BYTES
            || actions.iter().any(|action| action.key == key)
        {
            return Err(ServerError::InvalidArgs(
                "Notification action keys and labels are invalid",
            ));
        }
        actions.push(NotificationAction {
            key: key.to_owned(),
            label: label.to_owned(),
        });
    }
    Ok(actions)
}

fn expiration(timeout: i32, urgency: Urgency) -> Result<Option<Duration>, ServerError> {
    match timeout {
        -1 if urgency == Urgency::Critical => Ok(None),
        -1 => Ok(Some(DEFAULT_TIMEOUT)),
        0 => Ok(None),
        timeout if timeout > 0 => Ok(Some(Duration::from_millis(timeout as u64))),
        _ => Err(ServerError::InvalidArgs(
            "Notification expiration must be -1, zero, or positive",
        )),
    }
}

fn safe_icon_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn named_icon(name: &str) -> Option<Icon> {
    if safe_icon_name(name) {
        icons::theme_icon(name, 48)
    } else {
        None
    }
}

fn image_icon(value: &glib::Variant) -> Option<Icon> {
    if value.type_().as_str() != "(iiibiiay)" {
        return None;
    }
    let width = value.try_child_value(0)?.get::<i32>()?;
    let height = value.try_child_value(1)?.get::<i32>()?;
    let rowstride = value.try_child_value(2)?.get::<i32>()?;
    let has_alpha = value.try_child_value(3)?.get::<bool>()?;
    let bits = value.try_child_value(4)?.get::<i32>()?;
    let channels = value.try_child_value(5)?.get::<i32>()?;
    let data = value.try_child_value(6)?;
    // Borrow the raw bytes. The shared converter validates geometry, channel
    // layout, overflow and a 1 MiB bound before allocating packed RGBA.
    icons::from_rgb(
        width,
        height,
        rowstride,
        has_alpha,
        bits,
        channels,
        data.fixed_array::<u8>().ok()?,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn ready() -> NotificationState {
        let mut state = NotificationState::new();
        state.availability = Availability::Ready;
        state
    }

    fn input(replaces_id: u32, summary: &str) -> NotificationInput {
        NotificationInput {
            replaces_id,
            app_name: "Example".into(),
            summary: summary.into(),
            body: "Private notification body".into(),
            actions: vec![NotificationAction {
                key: "default".into(),
                label: "Open".into(),
            }],
            urgency: Urgency::Normal,
            resident: false,
            icon: None,
            timeout: Some(DEFAULT_TIMEOUT),
        }
    }

    fn wire(
        actions: Vec<&str>,
        hints: HashMap<&str, glib::Variant>,
        timeout: i32,
    ) -> glib::Variant {
        (
            "Example",
            0_u32,
            "",
            "<b>Summary</b>",
            "<a href='file:///private'>Body</a>",
            actions,
            hints,
            timeout,
        )
            .to_variant()
    }

    #[test]
    fn replacement_is_atomic_unread_and_old_expiry_cannot_close_it() {
        let mut state = ready();
        let first = state.notify(":1.10", input(0, "First"), 10).unwrap();
        state.finish_action(first.token).unwrap();
        // Use a fresh active resident record to test unread reset on replacement.
        let mut resident = input(0, "Resident");
        resident.resident = true;
        let original = state.notify(":1.10", resident, 20).unwrap();
        state.finish_action(original.token).unwrap();
        let replacement = state
            .notify(":1.10", input(original.token.id, "Updated"), 30)
            .unwrap();
        assert_eq!(replacement.token.id, original.token.id);
        assert_ne!(replacement.token.generation, original.token.generation);
        assert_eq!(replacement.previous, Some(original.token));
        assert!(replacement.evicted.is_none());
        assert!(state.expire(original.token).is_none());
        assert_eq!(
            state.get_notifications().unwrap()["items"][0],
            json!({
                "id": original.token.id,
                "appName": "Example",
                "summary": "Updated",
                "body": "Private notification body",
                "createdAt": 30,
                "active": true,
                "read": false,
                "urgency": "normal",
                "icon": null,
                "actions": [{"key": "default", "label": "Open"}],
            })
        );
        assert_eq!(
            state.expire(replacement.token).unwrap().reason,
            CLOSED_EXPIRED
        );
        assert!(state.expire(replacement.token).is_none());
    }

    #[test]
    fn expired_and_unknown_replacement_ids_allocate_new_identifiers() {
        let mut state = ready();
        let old = state.notify(":1.10", input(0, "Old"), 1).unwrap();
        state.expire(old.token).unwrap();
        let new = state
            .notify(":1.10", input(old.token.id, "New"), 2)
            .unwrap();
        assert_ne!(new.token.id, old.token.id);
        assert_eq!(new.previous, None);
        let unknown = state.notify(":1.10", input(700, "Unknown"), 3).unwrap();
        assert_ne!(unknown.token.id, 700);
        assert_ne!(unknown.token.id, new.token.id);
        assert!(state.expire(old.token).is_none());
        assert!(state.is_current_active(new.token));
        assert_eq!(
            state.close_by_sender(":1.10", old.token.id),
            Err(ServerError::UnknownNotification)
        );
    }

    #[test]
    fn foreign_sender_cannot_replace_or_close_an_active_notification() {
        let mut state = ready();
        let original = state.notify(":1.10", input(0, "Original"), 1).unwrap();
        assert!(matches!(
            state.notify(":1.11", input(original.token.id, "Forged"), 2),
            Err(ServerError::AccessDenied)
        ));
        assert_eq!(
            state.close_by_sender(":1.11", original.token.id),
            Err(ServerError::AccessDenied)
        );
        assert!(state.is_current_active(original.token));
        let bound = state.bound_action(original.token.id, "default").unwrap();
        assert_eq!(bound.sender, ":1.10");
        assert_eq!(
            state.get_notifications().unwrap()["items"][0]["summary"],
            "Original"
        );
        let closed = state.close_by_sender(":1.10", original.token.id).unwrap();
        assert_eq!(closed.reason, CLOSED_BY_CLIENT);
        assert_eq!(closed.sender, ":1.10");
        assert!(state.expire(original.token).is_none());
        // The now-invalid ID does not confer authority over the former record.
        let other = state
            .notify(":1.11", input(original.token.id, "Other"), 3)
            .unwrap();
        assert_ne!(other.token.id, original.token.id);
    }

    #[test]
    fn actions_are_bound_to_current_keys_and_resident_lifecycle() {
        let mut state = ready();
        let original = state.notify(":1.42", input(0, "First"), 1).unwrap();
        let old_action = state.bound_action(original.token.id, "default").unwrap();
        let mut replacement = input(original.token.id, "Updated");
        replacement.actions = vec![NotificationAction {
            key: "reply".into(),
            label: "Reply".into(),
        }];
        replacement.resident = true;
        let current = state.notify(":1.42", replacement, 2).unwrap();
        assert_eq!(
            state
                .bound_action(current.token.id, "default")
                .err()
                .unwrap()
                .code,
            "unknown_notification_action"
        );
        assert_eq!(
            state.finish_action(old_action.token).err().unwrap().code,
            "unknown_notification"
        );
        assert_eq!(state.snapshot()["unreadCount"], 1);
        let bound = state.bound_action(current.token.id, "reply").unwrap();
        assert_eq!(bound.sender, ":1.42");
        assert_eq!(bound.key, "reply");
        assert!(state.finish_action(bound.token).unwrap().is_none());
        assert_eq!(
            state.snapshot(),
            json!({"available":true,"unreadCount":0,"activeCount":1,"historyCount":1})
        );
        // Resident prevents action dismissal, not an explicit expiry timeout.
        assert_eq!(state.expire(current.token).unwrap().reason, CLOSED_EXPIRED);
        assert_eq!(
            state
                .bound_action(current.token.id, "reply")
                .err()
                .unwrap()
                .code,
            "unknown_notification"
        );
    }

    #[test]
    fn nonresident_action_closes_but_history_dismissal_emits_no_second_close() {
        let mut state = ready();
        let current = state.notify(":1.12", input(0, "First"), 1).unwrap();
        let closed = state.finish_action(current.token).unwrap().unwrap();
        assert_eq!(closed.reason, CLOSED_DISMISSED);
        assert_eq!(closed.sender, ":1.12");
        assert!(state.expire(current.token).is_none());
        assert_eq!(
            state.snapshot(),
            json!({"available":true,"unreadCount":0,"activeCount":0,"historyCount":1})
        );
        assert!(state.dismiss(current.token.id).unwrap().closed.is_none());
        assert_eq!(state.snapshot()["historyCount"], 0);
        assert_eq!(
            state.dismiss(current.token.id).err().unwrap().code,
            "unknown_notification"
        );
    }

    #[test]
    fn active_dismissal_invalidates_the_record_and_its_timer() {
        let mut state = ready();
        let current = state.notify(":1.12", input(0, "First"), 1).unwrap();
        let removed = state.dismiss(current.token.id).unwrap();
        assert_eq!(removed.token, current.token);
        assert_eq!(removed.closed.unwrap().reason, CLOSED_DISMISSED);
        assert!(state.expire(current.token).is_none());
        assert!(state.bound_action(current.token.id, "default").is_err());
        let newer = state.notify(":1.12", input(0, "Newer"), 2).unwrap();
        assert!(state.expire(current.token).is_none());
        assert!(state.is_current_active(newer.token));
    }

    #[test]
    fn bounded_history_prefers_inactive_records_and_closes_active_evictions() {
        let mut state = ready();
        let first = state.notify(":1.20", input(0, "First"), 1).unwrap();
        let second = state.notify(":1.20", input(0, "Second"), 2).unwrap();
        state.expire(second.token).unwrap();
        for timestamp in 3..=100 {
            state.notify(":1.20", input(0, "More"), timestamp).unwrap();
        }
        let update = state.notify(":1.20", input(0, "After limit"), 101).unwrap();
        let removed = update.evicted.unwrap();
        assert_eq!(removed.token, second.token);
        assert!(removed.closed.is_none());
        assert!(state.is_current_active(first.token));
        assert_eq!(
            state.snapshot(),
            json!({"available":true,"unreadCount":100,"activeCount":100,"historyCount":100})
        );
        let next = state.notify(":1.20", input(0, "Overflow"), 102).unwrap();
        let closed = next.evicted.unwrap().closed.unwrap();
        assert_eq!(closed.token, first.token);
        assert_eq!(closed.reason, CLOSED_RESERVED);
        assert_eq!(closed.sender, ":1.20");
        assert!(state.expire(first.token).is_none());
        assert_eq!(state.records.len(), HISTORY_LIMIT);
    }

    #[test]
    fn identifier_wrap_skips_zero_and_retained_ids() {
        let mut state = ready();
        let first = state.notify(":1.20", input(0, "First"), 1).unwrap();
        assert_eq!(first.token.id, 1);
        state.next_id = u32::MAX;
        let last = state.notify(":1.20", input(0, "Last"), 2).unwrap();
        assert_eq!(last.token.id, u32::MAX);
        let wrapped = state.notify(":1.20", input(0, "Wrapped"), 3).unwrap();
        assert_eq!(wrapped.token.id, 2);
        assert!(state.is_current_active(first.token));
    }

    #[test]
    fn shutdown_rejects_operations_and_stale_completions_without_retaining_bodies() {
        let mut state = ready();
        let current = state.notify(":1.20", input(0, "First"), 1).unwrap();
        state.shutdown();
        state.shutdown();
        assert!(state.records.is_empty());
        assert!(state.expire(current.token).is_none());
        assert!(matches!(
            state.notify(":1.20", input(0, "Late"), 2),
            Err(ServerError::Unavailable)
        ));
        assert_eq!(
            state.get_notifications().unwrap_err().code,
            "notifications_unavailable"
        );
        assert_eq!(
            state.dismiss(current.token.id).err().unwrap().code,
            "notifications_unavailable"
        );
        assert_eq!(
            state.finish_action(current.token).err().unwrap().code,
            "notifications_unavailable"
        );
        assert_eq!(
            state.snapshot(),
            json!({"available":false,"reason":"Notification service has shut down"})
        );
    }

    #[test]
    fn diagnostic_snapshot_contains_counts_not_notification_payloads_or_bus_identity() {
        let mut state = ready();
        state
            .notify(":1.77", input(0, "Secret summary"), 123)
            .unwrap();
        assert_eq!(
            state.snapshot(),
            json!({"available":true,"unreadCount":1,"activeCount":1,"historyCount":1})
        );
        let history = state.get_notifications().unwrap();
        assert_eq!(history["items"][0]["body"], "Private notification body");
        assert!(history["items"][0].get("sender").is_none());
        assert!(history["items"][0].get("generation").is_none());
        assert!(history["items"][0].get("resident").is_none());
    }

    #[test]
    fn history_reads_do_not_acknowledge_and_mark_all_read_updates_shared_authority() {
        let mut state = ready();
        let older = state.notify(":1.20", input(0, "Older"), 1).unwrap();
        state.expire(older.token).unwrap();
        state.notify(":1.20", input(0, "Newer"), 2).unwrap();
        let controller = NotificationsController {
            inner: Rc::new(NotificationsInner {
                state: RefCell::new(state),
                resources: RefCell::new(ServerResources::default()),
            }),
        };
        let other_surface = controller.clone();
        let history = controller.get_notifications().unwrap();
        assert_eq!(history["items"][0]["summary"], "Newer");
        assert_eq!(history["items"][1]["summary"], "Older");
        assert_eq!(other_surface.snapshot()["unreadCount"], 2);
        assert_eq!(
            controller.mark_all_read().unwrap(),
            json!({
                "available": true, "unreadCount": 0, "activeCount": 1, "historyCount": 2,
            })
        );
        assert_eq!(
            other_surface.get_notifications().unwrap()["items"][0]["read"],
            true
        );
        assert_eq!(
            other_surface.get_notifications().unwrap()["items"][1]["read"],
            true
        );
        assert_eq!(other_surface.snapshot()["unreadCount"], 0);
        controller.shutdown();
        assert_eq!(
            other_surface.mark_all_read().unwrap_err().code,
            "notifications_unavailable"
        );
    }

    #[test]
    fn expiration_honors_sticky_explicit_and_urgency_dependent_default_timeouts() {
        assert_eq!(
            expiration(-1, Urgency::Normal).unwrap(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            expiration(-1, Urgency::Low).unwrap(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(expiration(-1, Urgency::Critical).unwrap(), None);
        assert_eq!(expiration(0, Urgency::Normal).unwrap(), None);
        assert_eq!(
            expiration(37, Urgency::Critical).unwrap(),
            Some(Duration::from_millis(37))
        );
        assert!(expiration(-2, Urgency::Normal).is_err());
    }

    #[test]
    fn malformed_action_pairs_and_oversized_text_are_rejected() {
        for actions in [
            vec!["default"],
            vec!["", "Open"],
            vec!["default", "One", "default", "Two"],
        ] {
            assert!(parse_notify(&wire(actions, HashMap::new(), 0)).is_err());
        }
        let keys: Vec<String> = (0..=MAX_ACTIONS)
            .map(|index| format!("action-{index}"))
            .collect();
        let at_limit: Vec<&str> = keys[..MAX_ACTIONS]
            .iter()
            .flat_map(|key| [key.as_str(), "Open"])
            .collect();
        assert_eq!(
            parse_notify(&wire(at_limit, HashMap::new(), 0))
                .unwrap()
                .actions
                .len(),
            MAX_ACTIONS
        );
        let too_many: Vec<&str> = keys.iter().flat_map(|key| [key.as_str(), "Open"]).collect();
        assert!(parse_notify(&wire(too_many, HashMap::new(), 0)).is_err());
        let oversized_body = "x".repeat(MAX_BODY_BYTES + 1);
        let parameters = (
            "Example",
            0_u32,
            "",
            "Summary",
            oversized_body,
            Vec::<String>::new(),
            HashMap::<String, glib::Variant>::new(),
            0_i32,
        )
            .to_variant();
        assert!(parse_notify(&parameters).is_err());
        assert!(parse_notify(&"wrong shape".to_variant()).is_err());
        assert!(parse_notify(&(1_u32,).to_variant()).is_err());
    }

    #[test]
    fn unknown_hint_count_and_total_raw_payload_are_bounded_before_retention() {
        let keys: Vec<String> = (0..=MAX_HINTS)
            .map(|index| format!("unknown-{index}"))
            .collect();
        let at_limit: HashMap<&str, glib::Variant> = keys[..MAX_HINTS]
            .iter()
            .map(|key| (key.as_str(), false.to_variant()))
            .collect();
        assert_eq!(
            parse_notify(&wire(Vec::new(), at_limit, 0))
                .unwrap()
                .summary,
            "<b>Summary</b>"
        );
        let too_many: HashMap<&str, glib::Variant> = keys
            .iter()
            .map(|key| (key.as_str(), false.to_variant()))
            .collect();
        assert!(parse_notify(&wire(Vec::new(), too_many, 0)).is_err());
        let oversized = HashMap::from([("unknown", "x".repeat(MAX_PAYLOAD_BYTES).to_variant())]);
        assert!(parse_notify(&wire(Vec::new(), oversized, 0)).is_err());
    }

    #[test]
    fn text_stays_literal_and_unsupported_hints_confer_no_file_uri_or_sound_authority() {
        let hints = HashMap::from([
            ("image-path", "file:///private/secret.png".to_variant()),
            ("sound-file", "/private/secret.wav".to_variant()),
            ("desktop-entry", "../../private".to_variant()),
            ("urgency", 2_u8.to_variant()),
            ("resident", true.to_variant()),
        ]);
        let parsed = parse_notify(&wire(vec!["default", "<b>Open</b>"], hints, -1)).unwrap();
        assert_eq!(parsed.summary, "<b>Summary</b>");
        assert_eq!(parsed.body, "<a href='file:///private'>Body</a>");
        assert_eq!(parsed.actions[0].label, "<b>Open</b>");
        assert_eq!(parsed.urgency, Urgency::Critical);
        assert!(parsed.resident);
        assert!(parsed.icon.is_none());
        assert_eq!(parsed.timeout, None);
        for name in [
            "/etc/passwd",
            "../private",
            "file:///tmp/icon",
            "https://example/icon",
            "foo/bar",
            "foo\\bar",
            ".hidden",
        ] {
            assert!(!safe_icon_name(name));
        }
        assert!(safe_icon_name("mail-unread-symbolic"));
    }

    #[test]
    fn raw_images_use_supported_priority_and_reject_invalid_layouts_and_hint_types() {
        let preferred = (
            1_i32,
            1_i32,
            4_i32,
            true,
            8_i32,
            4_i32,
            vec![10_u8, 20, 30, 40],
        )
            .to_variant();
        let deprecated = (
            1_i32,
            1_i32,
            3_i32,
            false,
            8_i32,
            3_i32,
            vec![90_u8, 80, 70],
        )
            .to_variant();
        let hints = HashMap::from([("image-data", preferred), ("icon_data", deprecated)]);
        let parsed = parse_notify(&wire(Vec::new(), hints, 0)).unwrap();
        let icon = parsed.icon.unwrap();
        assert_eq!((icon.width, icon.height), (1, 1));
        assert_eq!(glib::base64_decode(&icon.rgba), [10, 20, 30, 40]);
        for image in [
            (-1_i32, 1_i32, 4_i32, true, 8_i32, 4_i32, vec![0_u8; 4]).to_variant(),
            (
                513_i32,
                1_i32,
                2052_i32,
                true,
                8_i32,
                4_i32,
                vec![0_u8; 2052],
            )
                .to_variant(),
            (1_i32, 2_i32, i32::MAX, true, 8_i32, 4_i32, vec![0_u8; 8]).to_variant(),
            (2_i32, 1_i32, 8_i32, true, 8_i32, 4_i32, vec![0_u8; 7]).to_variant(),
            "wrong image type".to_variant(),
        ] {
            assert!(image_icon(&image).is_none());
        }
        let hints = HashMap::from([
            ("urgency", "critical".to_variant()),
            ("resident", 1_u32.to_variant()),
        ]);
        let parsed = parse_notify(&wire(Vec::new(), hints, -1)).unwrap();
        assert_eq!(parsed.urgency, Urgency::Normal);
        assert!(!parsed.resident);
        assert_eq!(parsed.timeout, Some(DEFAULT_TIMEOUT));
    }
}
