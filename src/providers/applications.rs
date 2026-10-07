use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use gtk4::gio::{self, prelude::*};
use serde_json::{Value, json};

use crate::messages::NativeMethodError;

use super::icons::{self, Icon};

const ICON_SIZE: i32 = 32;

type LaunchCallback = Box<dyn FnOnce(Result<Value, NativeMethodError>)>;

#[derive(Clone)]
pub struct ApplicationsController {
    inner: Rc<Inner>,
}

struct Inner {
    context: glib::MainContext,
    state: RefCell<State>,
}

struct State {
    status: DatabaseStatus,
    generation: u64,
    entries: Vec<Entry>,
    monitor: Option<(gio::AppInfoMonitor, glib::SignalHandlerId)>,
    refresh: Option<(u64, gio::Cancellable)>,
    refresh_task: Option<glib::JoinHandle<()>>,
    next_launch: u64,
    launches: BTreeMap<u64, PendingLaunch>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DatabaseStatus {
    Loading,
    Refreshing,
    Ready,
    Failed,
    Stopped,
}

impl DatabaseStatus {
    fn reason(self) -> &'static str {
        match self {
            Self::Loading => "The installed application database is loading",
            Self::Refreshing => "The installed application database is refreshing",
            Self::Failed => "The installed application database could not be loaded",
            Self::Stopped => "The applications provider has stopped",
            Self::Ready => "The installed application database is available",
        }
    }
}

struct Entry {
    info: gio::AppInfo,
    metadata: Metadata,
    // None means unqueried; Some(None) caches a real failed icon lookup.
    icon: Option<Option<Icon>>,
}

struct Metadata {
    id: String,
    name: String,
    description: Option<String>,
    folded_name: String,
    folded_id: String,
    folded_description: String,
}

struct PendingLaunch {
    application_id: String,
    generation: u64,
    cancellable: gio::Cancellable,
    callback: LaunchCallback,
}

impl State {
    fn new() -> Self {
        Self {
            status: DatabaseStatus::Loading,
            generation: 0,
            entries: Vec::new(),
            monitor: None,
            refresh: None,
            refresh_task: None,
            next_launch: 0,
            launches: BTreeMap::new(),
        }
    }

    fn require_ready(&self) -> Result<(), NativeMethodError> {
        if self.status == DatabaseStatus::Ready {
            Ok(())
        } else {
            Err(unavailable(self.status.reason()))
        }
    }

    fn allowed(&self, id: &str) -> Result<&Entry, NativeMethodError> {
        self.require_ready()?;
        self.entries
            .iter()
            .find(|entry| entry.metadata.id == id)
            .ok_or_else(|| {
                NativeMethodError::new(
                    "unknown_application",
                    "The application is not in the current installed application database",
                )
            })
    }
}

impl ApplicationsController {
    pub fn new() -> Self {
        let inner = Rc::new(Inner {
            context: glib::MainContext::ref_thread_default(),
            state: RefCell::new(State::new()),
        });
        let monitor = gio::AppInfoMonitor::get();
        let weak = Rc::downgrade(&inner);
        let handler = monitor.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                invalidate(&inner);
            }
        });
        inner.state.borrow_mut().monitor = Some((monitor, handler));
        start_refresh(&inner);
        Self { inner }
    }

    pub fn snapshot(&self) -> Value {
        let state = self.inner.state.borrow();
        if state.status == DatabaseStatus::Ready {
            json!({"available": true, "count": state.entries.len()})
        } else {
            json!({"available": false, "reason": state.status.reason()})
        }
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Value, NativeMethodError> {
        if !(1..=50).contains(&limit) || query.chars().count() > 256 {
            return Err(NativeMethodError::new(
                "invalid_params",
                "Application search requires a limit from 1 to 50 and at most 256 query characters",
            ));
        }
        let folded_query = query.trim().to_lowercase();
        let tokens: Vec<&str> = folded_query.split_whitespace().collect();
        let (generation, selected) = {
            let state = self.inner.state.borrow();
            state.require_ready()?;
            let selected = select_matches(&state.entries, &folded_query, &tokens, limit as usize);
            (state.generation, selected)
        };
        let mut items = Vec::with_capacity(selected.len());
        for index in selected {
            let (id, name, description, cached, info) = {
                let state = self.inner.state.borrow();
                state.require_ready()?;
                let entry = &state.entries[index];
                (
                    entry.metadata.id.clone(),
                    entry.metadata.name.clone(),
                    entry.metadata.description.clone(),
                    entry.icon.clone(),
                    entry.info.clone(),
                )
            };
            let icon = match cached {
                Some(icon) => icon,
                None => {
                    // GIcon resolution/GTK theme access stays on the caller's main context.
                    let icon = info
                        .icon()
                        .and_then(|icon| icons::from_gicon(&icon, ICON_SIZE));
                    let mut state = self.inner.state.borrow_mut();
                    state.require_ready()?;
                    if state.generation != generation {
                        return Err(unavailable(DatabaseStatus::Refreshing.reason()));
                    }
                    state.entries[index].icon = Some(icon.clone());
                    icon
                }
            };
            items.push(json!({"id": id, "name": name, "description": description, "icon": icon}));
        }
        Ok(json!({"items": items}))
    }

    pub fn launch(
        &self,
        application_id: String,
        context: gio::AppLaunchContext,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        let info = {
            let state = self.inner.state.borrow();
            state
                .allowed(&application_id)
                .map(|entry| entry.info.clone())
        };
        let info = match info {
            Ok(info) => info,
            Err(error) => {
                self.inner
                    .context
                    .spawn_local(async move { callback(Err(error)) });
                return;
            }
        };
        let cancellable = gio::Cancellable::new();
        let token = {
            let mut state = self.inner.state.borrow_mut();
            let token = state.next_launch;
            state.next_launch = state.next_launch.wrapping_add(1);
            let generation = state.generation;
            state.launches.insert(
                token,
                PendingLaunch {
                    application_id,
                    generation,
                    cancellable: cancellable.clone(),
                    callback: Box::new(callback),
                },
            );
            token
        };
        let weak = Rc::downgrade(&self.inner);
        // The ID resolves exclusively to a native cached AppInfo. No command, file, or URI
        // comes from the browser; GIO handles Exec and DBus activation itself.
        info.launch_uris_async(&[], Some(&context), Some(&cancellable), move |result| {
            finish_launch(&weak, token, result);
        });
    }

    pub fn shutdown(&self) {
        let resources = {
            let mut state = self.inner.state.borrow_mut();
            if state.status == DatabaseStatus::Stopped {
                return;
            }
            state.status = DatabaseStatus::Stopped;
            state.generation = state.generation.wrapping_add(1);
            state.entries.clear();
            (
                state.monitor.take(),
                state.refresh.take(),
                state.refresh_task.take(),
                std::mem::take(&mut state.launches),
            )
        };
        cleanup(resources.0, resources.1, resources.2, resources.3);
    }
}

impl super::Provider for ApplicationsController {
    fn name(&self) -> &'static str {
        "applications"
    }

    fn snapshot(&self) -> Value {
        ApplicationsController::snapshot(self)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        cleanup(
            state.monitor.take(),
            state.refresh.take(),
            state.refresh_task.take(),
            std::mem::take(&mut state.launches),
        );
    }
}

fn unavailable(reason: &'static str) -> NativeMethodError {
    NativeMethodError::new("applications_unavailable", reason)
}

fn cleanup(
    monitor: Option<(gio::AppInfoMonitor, glib::SignalHandlerId)>,
    refresh: Option<(u64, gio::Cancellable)>,
    task: Option<glib::JoinHandle<()>>,
    launches: BTreeMap<u64, PendingLaunch>,
) {
    if let Some((monitor, handler)) = monitor {
        monitor.disconnect(handler);
    }
    if let Some((_, cancellable)) = refresh {
        cancellable.cancel();
    }
    if let Some(task) = task {
        task.abort();
    }
    for pending in launches.into_values() {
        pending.cancellable.cancel();
        (pending.callback)(Err(unavailable(DatabaseStatus::Stopped.reason())));
    }
}

fn invalidate(inner: &Rc<Inner>) {
    let (cancel, launches) = {
        let mut state = inner.state.borrow_mut();
        if state.status == DatabaseStatus::Stopped {
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        state.status = DatabaseStatus::Refreshing;
        // Fail closed as soon as the monitor reports a change, including removals.
        state.entries.clear();
        (
            state.refresh.as_ref().map(|(_, cancel)| cancel.clone()),
            std::mem::take(&mut state.launches),
        )
    };
    if let Some(cancel) = cancel {
        cancel.cancel();
    }
    for pending in launches.into_values() {
        pending.cancellable.cancel();
        (pending.callback)(Err(unavailable(DatabaseStatus::Refreshing.reason())));
    }
    start_refresh(inner);
}

fn start_refresh(inner: &Rc<Inner>) {
    let (generation, cancel) = {
        let mut state = inner.state.borrow_mut();
        if state.status == DatabaseStatus::Stopped || state.refresh.is_some() {
            return;
        }
        let cancel = gio::Cancellable::new();
        let generation = state.generation;
        state.refresh = Some((generation, cancel.clone()));
        (generation, cancel)
    };
    // GIO's established I/O worker pool avoids desktop database disk I/O on GTK.
    // g_app_info_get_all itself has no cancellation parameter; check cooperatively
    // around that call and during collection, then gate publication by generation.
    let worker = gio::spawn_blocking(move || enumerate(&cancel));
    let weak = Rc::downgrade(inner);
    let task = inner.context.spawn_local(async move {
        let entries = match worker.await {
            Ok(Some(metadata)) => materialize(&weak, generation, metadata).await,
            _ => None,
        };
        finish_refresh(&weak, generation, entries);
    });
    inner.state.borrow_mut().refresh_task = Some(task);
}

fn enumerate(cancel: &gio::Cancellable) -> Option<Vec<Metadata>> {
    if cancel.is_cancelled() {
        return None;
    }
    let apps = gio::AppInfo::all();
    let mut entries = Vec::with_capacity(apps.len());
    for info in apps {
        if cancel.is_cancelled() {
            return None;
        }
        let Some(desktop) = info.downcast_ref::<gio_unix::DesktopAppInfo>() else {
            continue;
        };
        if !native_launchable(desktop) {
            continue;
        }
        let id = info.id();
        let name = info.display_name();
        let description = info.description();
        let Some(metadata) = Metadata::visible(
            id.as_deref(),
            &name,
            description.as_deref(),
            info.should_show(),
        ) else {
            continue;
        };
        entries.push(metadata);
    }
    entries.sort_by(|left, right| {
        left.folded_name
            .cmp(&right.folded_name)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut seen = std::collections::HashSet::with_capacity(entries.len());
    entries.retain(|entry| seen.insert(entry.id.clone()));
    // Enumerated AppInfo references stay on this worker. Only owned strings
    // cross the catalog boundary: AppInfo's interface wrapper is not Send.
    Some(entries)
}

fn native_launchable(info: &gio_unix::DesktopAppInfo) -> bool {
    // A visible Type=Application entry may still have no launch target. Query
    // native metadata only; never parse Exec or retain/export its command line.
    info.boolean("DBusActivatable")
        || info
            .string("Exec")
            .is_some_and(|exec| !exec.trim().is_empty())
}

fn refresh_is_current(weak: &Weak<Inner>, generation: u64) -> bool {
    let Some(inner) = weak.upgrade() else {
        return false;
    };
    let state = inner.state.borrow();
    state.status != DatabaseStatus::Stopped
        && state.generation == generation
        && state.refresh.as_ref().map(|(epoch, _)| *epoch) == Some(generation)
}

async fn materialize(
    weak: &Weak<Inner>,
    generation: u64,
    metadata: Vec<Metadata>,
) -> Option<Vec<Entry>> {
    let mut entries = Vec::with_capacity(metadata.len());
    for (index, metadata) in metadata.into_iter().enumerate() {
        if index % 8 == 0 {
            // Resolve native objects on GTK, yielding between small batches so
            // database enumeration and one large resolution pass never block it.
            glib::timeout_future(std::time::Duration::from_millis(1)).await;
            if !refresh_is_current(weak, generation) {
                return None;
            }
        }
        let Some(info) = gio_unix::DesktopAppInfo::new(&metadata.id) else {
            continue;
        };
        // Revalidate the authoritative object if a file changed between worker
        // enumeration and resolution. Search/actions reuse this cached object.
        if !info.should_show() || info.is_hidden() || !native_launchable(&info) {
            continue;
        }
        entries.push(Entry {
            info: info.upcast(),
            metadata,
            icon: None,
        });
    }
    Some(entries)
}

fn finish_refresh(weak: &Weak<Inner>, generation: u64, entries: Option<Vec<Entry>>) {
    let Some(inner) = weak.upgrade() else { return };
    let restart = {
        let mut state = inner.state.borrow_mut();
        if state.status == DatabaseStatus::Stopped
            || state.refresh.as_ref().map(|(epoch, _)| *epoch) != Some(generation)
        {
            return;
        }
        state.refresh = None;
        state.refresh_task = None;
        if state.generation != generation {
            true
        } else {
            match entries {
                Some(entries) => {
                    state.entries = entries;
                    state.status = DatabaseStatus::Ready;
                }
                None => state.status = DatabaseStatus::Failed,
            }
            false
        }
    };
    if restart {
        start_refresh(&inner);
    }
}

impl Metadata {
    fn visible(
        id: Option<&str>,
        name: &str,
        description: Option<&str>,
        visible: bool,
    ) -> Option<Self> {
        let id = id?;
        if !visible || name.trim().is_empty() || !is_desktop_id(id) {
            return None;
        }
        let description = description.filter(|description| !description.trim().is_empty());
        Some(Self {
            id: id.to_owned(),
            name: name.to_owned(),
            description: description.map(str::to_owned),
            folded_name: name.to_lowercase(),
            folded_id: id.to_lowercase(),
            folded_description: description.unwrap_or_default().to_lowercase(),
        })
    }

    fn rank(&self, query: &str, tokens: &[&str]) -> Option<u8> {
        if tokens.is_empty() {
            return Some(0);
        }
        if !tokens.iter().all(|token| {
            self.folded_name.contains(token)
                || self.folded_id.contains(token)
                || self.folded_description.contains(token)
        }) {
            return None;
        }
        Some(if self.folded_name == query {
            0
        } else if self.folded_name.starts_with(query) {
            1
        } else if tokens.iter().all(|token| {
            self.folded_name
                .split_whitespace()
                .any(|word| word.starts_with(token))
        }) {
            2
        } else if self.folded_name.contains(query) {
            3
        } else if self.folded_id == query {
            4
        } else if self.folded_id.starts_with(query) {
            5
        } else if self.folded_id.contains(query) {
            6
        } else if tokens
            .iter()
            .all(|token| self.folded_name.contains(token) || self.folded_id.contains(token))
        {
            7
        } else {
            8
        })
    }
}

fn is_desktop_id(id: &str) -> bool {
    id.len() > ".desktop".len()
        && id.ends_with(".desktop")
        && !id
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
}

fn select_matches(entries: &[Entry], query: &str, tokens: &[&str], limit: usize) -> Vec<usize> {
    // Keep only the best bounded set rather than allocate/sort the entire database
    // for each query. Entries already have a deterministic localized-name order.
    let mut best = Vec::with_capacity(limit + 1);
    for (index, entry) in entries.iter().enumerate() {
        let Some(rank) = entry.metadata.rank(query, tokens) else {
            continue;
        };
        let candidate = (rank, index);
        let position = best
            .binary_search(&candidate)
            .unwrap_or_else(|position| position);
        if position < limit {
            best.insert(position, candidate);
            best.truncate(limit);
        }
    }
    best.into_iter().map(|(_, index)| index).collect()
}

fn finish_launch(weak: &Weak<Inner>, token: u64, result: Result<(), glib::Error>) {
    let Some(inner) = weak.upgrade() else { return };
    let (pending, current) = {
        let mut state = inner.state.borrow_mut();
        let Some(pending) = state.launches.remove(&token) else {
            return;
        };
        let current =
            state.status == DatabaseStatus::Ready && state.generation == pending.generation;
        (pending, current)
    };
    let result = if !current {
        Err(unavailable(DatabaseStatus::Refreshing.reason()))
    } else {
        result
            .map(|()| json!({"applicationId": pending.application_id}))
            .map_err(|_| {
                // GIO errors may include Exec, working directories or absolute filenames.
                NativeMethodError::new(
                    "application_launch_failed",
                    "The application could not be launched",
                )
            })
    };
    (pending.callback)(result);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(id: &str, name: &str, description: Option<&str>) -> Metadata {
        Metadata::visible(Some(id), name, description, true).unwrap()
    }

    fn entry(id: &str, name: &str, description: Option<&str>) -> Entry {
        // This creates AppInfo metadata only; tests never call a launch method.
        let info = gio::AppInfo::create_from_commandline(
            "true",
            Some(name),
            gio::AppInfoCreateFlags::NONE,
        )
        .unwrap();
        Entry {
            info,
            metadata: metadata(id, name, description),
            icon: Some(None),
        }
    }

    fn native_fixture(extra: &str) -> Option<gio_unix::DesktopAppInfo> {
        let key_file = glib::KeyFile::new();
        key_file
            .load_from_data(
                &format!("[Desktop Entry]\nType=Application\nName=Fixture\n{extra}"),
                glib::KeyFileFlags::NONE,
            )
            .unwrap();
        gio_unix::DesktopAppInfo::from_keyfile(&key_file)
    }

    #[test]
    fn native_launchability_excludes_visible_entries_without_exec_or_bus_activation() {
        for extra in ["", "Exec=\n", "Exec=   \n", "DBusActivatable=false\n"] {
            assert!(
                !native_fixture(extra).is_some_and(|info| native_launchable(&info)),
                "{extra:?}"
            );
        }
        let activation_only = native_fixture("DBusActivatable=true\n").unwrap();
        assert!(activation_only.should_show());
        assert!(native_launchable(&activation_only));
        let executable = native_fixture("Exec=/proc/self/exe\n").unwrap();
        assert!(executable.should_show());
        assert!(native_launchable(&executable));
    }

    #[test]
    fn stale_refresh_completion_cannot_reintroduce_removed_application_ids() {
        let mut state = State::new();
        state.status = DatabaseStatus::Refreshing;
        state.generation = 2;
        state.refresh = Some((2, gio::Cancellable::new()));
        let inner = Rc::new(Inner {
            context: glib::MainContext::new(),
            state: RefCell::new(state),
        });
        let weak = Rc::downgrade(&inner);
        finish_refresh(
            &weak,
            1,
            Some(vec![entry("removed.desktop", "Removed", None)]),
        );
        assert_eq!(
            inner
                .state
                .borrow()
                .allowed("removed.desktop")
                .err()
                .unwrap()
                .code,
            "applications_unavailable"
        );
        finish_refresh(
            &weak,
            2,
            Some(vec![entry("current.desktop", "Current", None)]),
        );
        assert_eq!(
            inner
                .state
                .borrow()
                .allowed("removed.desktop")
                .err()
                .unwrap()
                .code,
            "unknown_application"
        );
        assert!(inner.state.borrow().allowed("current.desktop").is_ok());
        ApplicationsController {
            inner: Rc::clone(&inner),
        }
        .shutdown();
        finish_refresh(
            &weak,
            2,
            Some(vec![entry("removed.desktop", "Removed", None)]),
        );
        assert_eq!(
            inner
                .state
                .borrow()
                .allowed("current.desktop")
                .err()
                .unwrap()
                .code,
            "applications_unavailable"
        );
    }

    #[test]
    fn visibility_and_native_id_filter_exclude_nonapplications_and_paths() {
        assert!(Metadata::visible(Some("valid.desktop"), "Visible", None, true).is_some());
        assert!(Metadata::visible(Some("hidden.desktop"), "Hidden", None, false).is_none());
        assert!(Metadata::visible(None, "Anonymous", None, true).is_none());
        assert!(Metadata::visible(Some("empty.desktop"), "  ", None, true).is_none());
        for id in [
            "/tmp/app.desktop",
            "../app.desktop",
            "dir/app.desktop",
            "dir\\app.desktop",
            "app\n.desktop",
            "app",
            ".desktop",
        ] {
            assert!(
                Metadata::visible(Some(id), "Unsafe", None, true).is_none(),
                "{id:?}"
            );
        }
    }

    #[test]
    fn localized_matching_is_case_insensitive_and_requires_every_query_token() {
        let application = metadata("org.editor.desktop", "Éditeur 中文", Some("Edit documents"));
        assert_eq!(application.rank("éditeur", &["éditeur"]), Some(1));
        assert_eq!(
            application.rank("中文 documents", &["中文", "documents"]),
            Some(8)
        );
        assert_eq!(application.rank("中文 missing", &["中文", "missing"]), None);
        assert_eq!(application.rank("org.editor", &["org.editor"]), Some(5));
    }

    #[test]
    fn names_rank_ahead_of_id_and_description_matches_with_bounded_stable_results() {
        // Input follows production's name/id order; rank must override that order.
        let entries = vec![
            entry("alpha.desktop", "Alpha", Some("editor")),
            entry("beta-editor.desktop", "Beta", None),
            entry("exact.desktop", "Editor", None),
            entry("prefix.desktop", "Editor Pro", None),
            entry("word.desktop", "Text Editor", None),
        ];
        assert_eq!(
            select_matches(&entries, "editor", &["editor"], 3),
            vec![2, 3, 4]
        );
        assert_eq!(
            select_matches(&entries, "editor", &["editor"], 50),
            vec![2, 3, 4, 1, 0]
        );
        assert_eq!(select_matches(&entries, "", &[], 2), vec![0, 1]);
    }

    #[test]
    fn allowlist_replacement_removes_old_ids_and_shutdown_rejects_all_actions() {
        let mut state = State::new();
        state.status = DatabaseStatus::Ready;
        state.entries = vec![entry("old.desktop", "Old", None)];
        assert!(state.allowed("old.desktop").is_ok());
        assert_eq!(
            state.allowed("/tmp/old.desktop").err().unwrap().code,
            "unknown_application"
        );
        state.status = DatabaseStatus::Refreshing;
        state.entries.clear();
        assert_eq!(
            state.allowed("old.desktop").err().unwrap().code,
            "applications_unavailable"
        );
        state.entries = vec![entry("new.desktop", "New", None)];
        state.status = DatabaseStatus::Ready;
        assert_eq!(
            state.allowed("old.desktop").err().unwrap().code,
            "unknown_application"
        );
        assert!(state.allowed("new.desktop").is_ok());
        state.status = DatabaseStatus::Stopped;
        assert_eq!(
            state.allowed("new.desktop").err().unwrap().code,
            "applications_unavailable"
        );
    }

    #[test]
    fn monitor_invalidation_cancels_pending_work_and_ignores_late_success() {
        let mut state = State::new();
        state.status = DatabaseStatus::Ready;
        state.entries = vec![entry("old.desktop", "Old", None)];
        let refresh_cancel = gio::Cancellable::new();
        state.refresh = Some((0, refresh_cancel.clone()));
        let inner = Rc::new(Inner {
            context: glib::MainContext::new(),
            state: RefCell::new(state),
        });
        let outcomes = Rc::new(RefCell::new(Vec::new()));
        let outcomes_for_callback = Rc::clone(&outcomes);
        let weak = Rc::downgrade(&inner);
        let weak_for_callback = weak.clone();
        let launch_cancel = gio::Cancellable::new();
        inner.state.borrow_mut().launches.insert(
            0,
            PendingLaunch {
                application_id: "old.desktop".to_owned(),
                generation: 0,
                cancellable: launch_cancel.clone(),
                callback: Box::new(move |result| {
                    // A consumer can reenter the controller without a RefCell panic.
                    assert!(
                        weak_for_callback
                            .upgrade()
                            .unwrap()
                            .state
                            .borrow()
                            .entries
                            .is_empty()
                    );
                    outcomes_for_callback
                        .borrow_mut()
                        .push(result.unwrap_err().code);
                }),
            },
        );
        invalidate(&inner);
        assert!(refresh_cancel.is_cancelled());
        assert!(launch_cancel.is_cancelled());
        assert_eq!(inner.state.borrow().generation, 1);
        assert_eq!(
            inner
                .state
                .borrow()
                .allowed("old.desktop")
                .err()
                .unwrap()
                .code,
            "applications_unavailable"
        );
        finish_launch(&weak, 0, Ok(()));
        assert_eq!(*outcomes.borrow(), vec!["applications_unavailable"]);
    }

    #[test]
    fn launch_errors_never_disclose_native_command_or_file_details() {
        let mut state = State::new();
        state.status = DatabaseStatus::Ready;
        let outcome = Rc::new(RefCell::new(None));
        let outcome_for_callback = Rc::clone(&outcome);
        state.launches.insert(
            0,
            PendingLaunch {
                application_id: "safe.desktop".to_owned(),
                generation: 0,
                cancellable: gio::Cancellable::new(),
                callback: Box::new(move |result| {
                    *outcome_for_callback.borrow_mut() = Some(result.unwrap_err());
                }),
            },
        );
        let inner = Rc::new(Inner {
            context: glib::MainContext::new(),
            state: RefCell::new(state),
        });
        finish_launch(
            &Rc::downgrade(&inner),
            0,
            Err(glib::Error::new(
                gio::IOErrorEnum::Failed,
                "/private/path SecretCommand SECRET=value",
            )),
        );
        let outcome = outcome.borrow();
        let error = outcome.as_ref().unwrap();
        assert_eq!(error.code, "application_launch_failed");
        assert_eq!(error.message, "The application could not be launched");
    }

    #[test]
    fn search_rejects_invalid_bounds_and_stopped_database() {
        let controller = ApplicationsController {
            inner: Rc::new(Inner {
                context: glib::MainContext::new(),
                state: RefCell::new(State::new()),
            }),
        };
        assert_eq!(controller.search("", 0).unwrap_err().code, "invalid_params");
        assert_eq!(
            controller.search("", 51).unwrap_err().code,
            "invalid_params"
        );
        assert_eq!(
            controller.search(&"界".repeat(257), 1).unwrap_err().code,
            "invalid_params"
        );
        controller.shutdown();
        controller.shutdown();
        assert_eq!(
            controller.search("", 1).unwrap_err().code,
            "applications_unavailable"
        );
        assert_eq!(controller.snapshot()["available"], false);
    }
}
