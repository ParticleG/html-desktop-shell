use std::{
    cell::RefCell,
    ffi::OsString,
    rc::{Rc, Weak},
    time::Duration,
};

use glib::{Priority, SourceId};
use gtk4::gio::{self, prelude::*};

use super::niri::{
    NiriReducer, ReducerPublication, SharedNiriPublication, SharedWorkspaceActivationObservers,
    WorkspaceActivation,
};

const STDERR_CHUNK_SIZE: usize = 4096;
const RECONNECT_BASE: Duration = Duration::from_millis(250);
const RECONNECT_MAX: Duration = Duration::from_secs(5);

pub(super) struct NiriCollector {
    state: Rc<RefCell<CollectorState>>,
}

struct CollectorState {
    publication: SharedNiriPublication,
    observers: SharedWorkspaceActivationObservers,
    command: Vec<OsString>,
    generation: u64,
    stopped: bool,
    retiring: bool,
    child: Option<gio::Subprocess>,
    stdout: Option<gio::DataInputStream>,
    stderr: Option<gio::InputStream>,
    io_cancel: Option<gio::Cancellable>,
    reconnect: Option<SourceId>,
    completion: CompletionGate,
    backoff: ReconnectBackoff,
    reducer: NiriReducer,
    activation_gate: ActivationGate,
}

#[derive(Default)]
struct ActivationGate {
    coherent_published: bool,
    baseline: Option<WorkspaceActivation>,
}

impl ActivationGate {
    fn reset(&mut self) {
        self.coherent_published = false;
        self.baseline = None;
    }

    fn observe(
        &mut self,
        focused_activation: Option<WorkspaceActivation>,
    ) -> Option<WorkspaceActivation> {
        if !self.coherent_published {
            self.coherent_published = true;
            self.baseline = focused_activation;
            return None;
        }

        let changed = focused_activation.is_some() && focused_activation != self.baseline;
        self.baseline = focused_activation.clone();
        changed.then_some(focused_activation).flatten()
    }
}

#[derive(Default)]
struct CompletionGate {
    stdout_done: bool,
    wait_done: bool,
    reconnect_claimed: bool,
}

impl CompletionGate {
    fn mark_stdout_done(&mut self) -> bool {
        self.stdout_done = true;
        self.claim_if_complete()
    }

    fn mark_wait_done(&mut self) -> bool {
        self.wait_done = true;
        self.claim_if_complete()
    }

    fn claim_if_complete(&mut self) -> bool {
        if self.stdout_done && self.wait_done && !self.reconnect_claimed {
            self.reconnect_claimed = true;
            true
        } else {
            false
        }
    }
}

struct ReconnectBackoff {
    base: Duration,
    maximum: Duration,
    attempt: u32,
}

impl ReconnectBackoff {
    fn new(base: Duration, maximum: Duration) -> Self {
        Self {
            base,
            maximum,
            attempt: 0,
        }
    }

    fn next_delay(&mut self) -> Duration {
        let multiplier = 1_u32.checked_shl(self.attempt.min(31)).unwrap_or(u32::MAX);
        self.attempt = self.attempt.saturating_add(1);
        self.base.saturating_mul(multiplier).min(self.maximum)
    }

    fn reset(&mut self) {
        self.attempt = 0;
    }
}

impl NiriCollector {
    pub(super) fn start(
        publication: SharedNiriPublication,
        observers: SharedWorkspaceActivationObservers,
    ) -> Self {
        Self::start_with_command(
            publication,
            observers,
            ["niri", "msg", "-j", "event-stream"]
                .map(OsString::from)
                .to_vec(),
            RECONNECT_BASE,
            RECONNECT_MAX,
        )
    }

    fn start_with_command(
        publication: SharedNiriPublication,
        observers: SharedWorkspaceActivationObservers,
        command: Vec<OsString>,
        reconnect_base: Duration,
        reconnect_maximum: Duration,
    ) -> Self {
        let state = Rc::new(RefCell::new(CollectorState {
            publication,
            observers,
            command,
            generation: 0,
            stopped: false,
            retiring: false,
            child: None,
            stdout: None,
            stderr: None,
            io_cancel: None,
            reconnect: None,
            completion: CompletionGate::default(),
            backoff: ReconnectBackoff::new(reconnect_base, reconnect_maximum),
            reducer: NiriReducer::default(),
            activation_gate: ActivationGate::default(),
        }));
        start_generation(&state);
        Self { state }
    }

    #[cfg(test)]
    fn start_for_test(
        publication: SharedNiriPublication,
        observers: SharedWorkspaceActivationObservers,
        command: Vec<OsString>,
        reconnect_delay: Duration,
    ) -> Self {
        Self::start_with_command(
            publication,
            observers,
            command,
            reconnect_delay,
            reconnect_delay,
        )
    }

    #[cfg(test)]
    pub(super) fn stopped(
        publication: SharedNiriPublication,
        observers: SharedWorkspaceActivationObservers,
    ) -> Self {
        Self {
            state: Rc::new(RefCell::new(CollectorState {
                publication,
                observers,
                command: Vec::new(),
                generation: 1,
                stopped: true,
                retiring: false,
                child: None,
                stdout: None,
                stderr: None,
                io_cancel: None,
                reconnect: None,
                completion: CompletionGate::default(),
                backoff: ReconnectBackoff::new(RECONNECT_BASE, RECONNECT_MAX),
                reducer: NiriReducer::default(),
                activation_gate: ActivationGate::default(),
            })),
        }
    }

    pub(super) fn shutdown(&self) {
        let (timer, cancellable, child, publication) = {
            let mut state = self.state.borrow_mut();
            if state.stopped {
                return;
            }
            state.stopped = true;
            state.generation = state.generation.wrapping_add(1);
            state.retiring = true;
            state.stdout.take();
            state.stderr.take();
            (
                state.reconnect.take(),
                state.io_cancel.take(),
                state.child.take(),
                Rc::clone(&state.publication),
            )
        };
        publication.borrow_mut().publish_unavailable();
        if let Some(timer) = timer {
            timer.remove();
        }
        if let Some(cancellable) = cancellable {
            cancellable.cancel();
        }
        if let Some(child) = child {
            child.force_exit();
        }
    }
}

fn start_generation(shared: &Rc<RefCell<CollectorState>>) {
    let (generation, command) = {
        let mut state = shared.borrow_mut();
        if state.stopped {
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        state.retiring = false;
        state.completion = CompletionGate::default();
        state.reducer = NiriReducer::default();
        state.activation_gate.reset();
        (state.generation, state.command.clone())
    };

    let argv = command.iter().map(OsString::as_os_str).collect::<Vec<_>>();
    let child = match gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    ) {
        Ok(child) => child,
        Err(_) => {
            let publication = {
                let state = shared.borrow();
                if !is_current(&state, generation) {
                    return;
                }
                Rc::clone(&state.publication)
            };
            publication.borrow_mut().publish_unavailable();
            schedule_reconnect(shared, generation);
            return;
        }
    };
    let stdout_pipe = child.stdout_pipe().expect("STDOUT_PIPE requested");
    let stderr = child.stderr_pipe().expect("STDERR_PIPE requested");
    let stdout = gio::DataInputStream::new(&stdout_pipe);
    let cancellable = gio::Cancellable::new();

    {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) {
            child.force_exit();
            return;
        }
        state.child = Some(child.clone());
        state.stdout = Some(stdout.clone());
        state.stderr = Some(stderr.clone());
        state.io_cancel = Some(cancellable.clone());
    }

    let weak = Rc::downgrade(shared);
    child.wait_async(gio::Cancellable::NONE, move |_| {
        wait_finished(&weak, generation);
    });
    read_stdout_next(
        Rc::downgrade(shared),
        generation,
        stdout,
        cancellable.clone(),
    );
    drain_stderr_next(Rc::downgrade(shared), generation, stderr, cancellable);
}

fn read_stdout_next(
    weak: Weak<RefCell<CollectorState>>,
    generation: u64,
    stdout: gio::DataInputStream,
    cancellable: gio::Cancellable,
) {
    let Some(shared) = weak.upgrade() else {
        return;
    };
    {
        let state = shared.borrow();
        if !is_current(&state, generation) || state.retiring {
            return;
        }
    }
    drop(shared);

    let weak_for_callback = weak.clone();
    let stdout_for_callback = stdout.clone();
    let cancellable_for_callback = cancellable.clone();
    stdout.read_line_async(Priority::DEFAULT, Some(&cancellable), move |result| {
        let Some(shared) = weak_for_callback.upgrade() else {
            return;
        };
        {
            let state = shared.borrow();
            if !is_current(&state, generation) {
                return;
            }
            if state.retiring {
                drop(state);
                stdout_finished(&shared, generation);
                return;
            }
        }

        match result {
            Ok(Some(line)) => {
                let update = {
                    let mut state = shared.borrow_mut();
                    state.reducer.apply_line(&line)
                };
                match update {
                    Ok(Some(update)) => {
                        publish_update(&shared, generation, update);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        retire_generation(&shared, generation, true, true);
                        return;
                    }
                }
                read_stdout_next(
                    Rc::downgrade(&shared),
                    generation,
                    stdout_for_callback,
                    cancellable_for_callback,
                );
            }
            Ok(None) => retire_generation(&shared, generation, true, true),
            Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {
                stdout_finished(&shared, generation);
            }
            Err(_) => retire_generation(&shared, generation, true, true),
        }
    });
}

fn publish_update(
    shared: &Rc<RefCell<CollectorState>>,
    generation: u64,
    update: ReducerPublication,
) {
    let (publication, observers, activation) = {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) || state.retiring {
            return;
        }
        let was_initial = !state.activation_gate.coherent_published;
        let activation = state
            .activation_gate
            .observe(update.focused_activation.clone());
        if was_initial {
            state.backoff.reset();
        }
        (
            Rc::clone(&state.publication),
            Rc::clone(&state.observers),
            activation,
        )
    };

    publication.borrow_mut().publish(update);
    let Some(activation) = activation else {
        return;
    };
    let callbacks = observers.borrow().callbacks();
    for callback in callbacks {
        callback(activation.clone());
    }
}

fn drain_stderr_next(
    weak: Weak<RefCell<CollectorState>>,
    generation: u64,
    stderr: gio::InputStream,
    cancellable: gio::Cancellable,
) {
    let Some(shared) = weak.upgrade() else {
        return;
    };
    {
        let state = shared.borrow();
        if !is_current(&state, generation) || state.retiring {
            return;
        }
    }
    drop(shared);

    let weak_for_callback = weak.clone();
    let stderr_for_callback = stderr.clone();
    let cancellable_for_callback = cancellable.clone();
    stderr.read_bytes_async(
        STDERR_CHUNK_SIZE,
        Priority::DEFAULT,
        Some(&cancellable),
        move |result| {
            let Some(shared) = weak_for_callback.upgrade() else {
                return;
            };
            {
                let state = shared.borrow();
                if !is_current(&state, generation) || state.retiring {
                    return;
                }
            }
            match result {
                Ok(bytes) if !bytes.is_empty() => drain_stderr_next(
                    Rc::downgrade(&shared),
                    generation,
                    stderr_for_callback,
                    cancellable_for_callback,
                ),
                Ok(_) => {}
                Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(_) => retire_generation(&shared, generation, false, true),
            }
        },
    );
}

fn wait_finished(weak: &Weak<RefCell<CollectorState>>, generation: u64) {
    let Some(shared) = weak.upgrade() else {
        return;
    };
    let (first_failure, cancellable, publication, schedule) = {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) {
            return;
        }
        let first_failure = !state.retiring;
        state.retiring = true;
        let schedule = state.completion.mark_wait_done();
        (
            first_failure,
            state.io_cancel.clone(),
            Rc::clone(&state.publication),
            schedule,
        )
    };
    if first_failure {
        publication.borrow_mut().publish_unavailable();
        if let Some(cancellable) = cancellable {
            cancellable.cancel();
        }
    }
    if schedule {
        finish_generation(&shared, generation);
    }
}

fn retire_generation(
    shared: &Rc<RefCell<CollectorState>>,
    generation: u64,
    stdout_done: bool,
    force_exit: bool,
) {
    let (first_failure, cancellable, child, publication, schedule) = {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) {
            return;
        }
        let first_failure = !state.retiring;
        state.retiring = true;
        let schedule = if stdout_done {
            state.completion.mark_stdout_done()
        } else {
            false
        };
        (
            first_failure,
            state.io_cancel.clone(),
            state.child.clone(),
            Rc::clone(&state.publication),
            schedule,
        )
    };
    if first_failure {
        publication.borrow_mut().publish_unavailable();
        if let Some(cancellable) = cancellable {
            cancellable.cancel();
        }
        if force_exit && let Some(child) = child {
            child.force_exit();
        }
    }
    if schedule {
        finish_generation(shared, generation);
    }
}

fn stdout_finished(shared: &Rc<RefCell<CollectorState>>, generation: u64) {
    let schedule = {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) || !state.retiring {
            return;
        }
        state.completion.mark_stdout_done()
    };
    if schedule {
        finish_generation(shared, generation);
    }
}

fn finish_generation(shared: &Rc<RefCell<CollectorState>>, generation: u64) {
    {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) || !state.retiring {
            return;
        }
        state.child.take();
        state.stdout.take();
        state.stderr.take();
        state.io_cancel.take();
    }
    schedule_reconnect(shared, generation);
}

fn schedule_reconnect(shared: &Rc<RefCell<CollectorState>>, generation: u64) {
    let delay = {
        let mut state = shared.borrow_mut();
        if !is_current(&state, generation) || state.reconnect.is_some() {
            return;
        }
        state.backoff.next_delay()
    };
    let weak = Rc::downgrade(shared);
    let timer = glib::timeout_add_local_once(delay, move || {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        {
            let mut state = shared.borrow_mut();
            if !is_current(&state, generation) {
                return;
            }
            state.reconnect.take();
        }
        start_generation(&shared);
    });
    let mut state = shared.borrow_mut();
    if is_current(&state, generation) && state.reconnect.is_none() {
        state.reconnect = Some(timer);
    } else {
        timer.remove();
    }
}

fn is_current(state: &CollectorState, generation: u64) -> bool {
    !state.stopped && state.generation == generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::niri::{NiriPublication, WorkspaceActivationObservers};

    fn observers() -> SharedWorkspaceActivationObservers {
        Rc::new(RefCell::new(WorkspaceActivationObservers::default()))
    }

    fn update(output: Option<&str>, workspace_index: u64) -> ReducerPublication {
        ReducerPublication {
            snapshot: serde_json::json!({ "available": true }),
            workspace_indices: vec![workspace_index],
            focused_activation: output.map(|output| WorkspaceActivation {
                output: output.to_owned(),
                workspace_index,
            }),
        }
    }
    #[cfg(target_os = "linux")]
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::Mutex,
        thread,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    #[cfg(target_os = "linux")]
    static DEFAULT_MAIN_CONTEXT: Mutex<()> = Mutex::new(());

    #[cfg(target_os = "linux")]
    struct TestDirectory(PathBuf);

    #[cfg(target_os = "linux")]
    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "html-desktop-shell-niri-collector-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("temporary test directory must be created");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(target_os = "linux")]
    struct CollectorShutdownGuard(NiriCollector);

    #[cfg(target_os = "linux")]
    impl Drop for CollectorShutdownGuard {
        fn drop(&mut self) {
            self.0.shutdown();
        }
    }

    #[cfg(target_os = "linux")]
    fn pump_main_context_until(
        context: &glib::MainContext,
        deadline: Instant,
        mut condition: impl FnMut() -> bool,
    ) {
        while !condition() {
            while context.pending() {
                context.iteration(false);
            }
            assert!(
                Instant::now() < deadline,
                "timed out while driving the default GLib main context"
            );
            thread::sleep(Duration::from_millis(2));
        }
        while context.pending() {
            context.iteration(false);
        }
    }

    #[cfg(target_os = "linux")]
    fn read_generation(path: &Path) -> Option<u64> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    #[cfg(target_os = "linux")]
    fn shell_quote(path: &Path) -> String {
        format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
    }

    #[test]
    fn reconnect_delay_caps_and_resets() {
        let mut backoff = ReconnectBackoff::new(Duration::from_millis(250), Duration::from_secs(5));
        assert_eq!(backoff.next_delay(), Duration::from_millis(250));
        assert_eq!(backoff.next_delay(), Duration::from_millis(500));
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
        assert_eq!(backoff.next_delay(), Duration::from_secs(2));
        assert_eq!(backoff.next_delay(), Duration::from_secs(4));
        assert_eq!(backoff.next_delay(), Duration::from_secs(5));
        assert_eq!(backoff.next_delay(), Duration::from_secs(5));
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(250));
    }

    #[test]
    fn eof_and_wait_claim_one_reconnect_in_either_order() {
        let mut eof_first = CompletionGate::default();
        assert!(!eof_first.mark_stdout_done());
        assert!(eof_first.mark_wait_done());
        assert!(!eof_first.mark_wait_done());
        assert!(!eof_first.mark_stdout_done());

        let mut wait_first = CompletionGate::default();
        assert!(!wait_first.mark_wait_done());
        assert!(wait_first.mark_stdout_done());
        assert!(!wait_first.mark_stdout_done());
        assert!(!wait_first.mark_wait_done());
    }

    #[test]
    fn activation_gate_suppresses_initial_duplicates_unavailable_and_reconnect_baseline() {
        let first = WorkspaceActivation {
            output: "eDP-1".to_owned(),
            workspace_index: 1,
        };
        let second = WorkspaceActivation {
            output: "DP-1".to_owned(),
            workspace_index: 2,
        };
        let mut gate = ActivationGate::default();

        assert_eq!(gate.observe(Some(first.clone())), None);
        assert_eq!(gate.observe(Some(first.clone())), None);
        assert_eq!(gate.observe(Some(second.clone())), Some(second.clone()));
        assert_eq!(gate.observe(None), None);
        assert_eq!(gate.observe(Some(first.clone())), Some(first.clone()));

        gate.reset();
        assert_eq!(gate.observe(Some(second)), None);
    }

    #[test]
    fn publication_callbacks_run_after_borrows_release_and_can_disconnect() {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let observers = observers();
        let collector = NiriCollector::stopped(Rc::clone(&publication), Rc::clone(&observers));
        let generation = {
            let mut state = collector.state.borrow_mut();
            state.stopped = false;
            state.generation
        };
        let state_weak = Rc::downgrade(&collector.state);
        let publication_for_callback = Rc::clone(&publication);
        let observers_for_callback = Rc::clone(&observers);
        let handler_id = Rc::new(RefCell::new(None));
        let handler_id_for_callback = Rc::clone(&handler_id);
        let activations = Rc::new(RefCell::new(Vec::new()));
        let activations_for_callback = Rc::clone(&activations);
        let id = observers.borrow_mut().connect(move |activation| {
            state_weak
                .upgrade()
                .expect("collector state must remain alive")
                .borrow_mut()
                .retiring = false;
            publication_for_callback
                .borrow_mut()
                .workspace_indices
                .push(99);
            observers_for_callback.borrow_mut().disconnect(
                handler_id_for_callback
                    .borrow()
                    .expect("handler ID must be set"),
            );
            activations_for_callback.borrow_mut().push(activation);
        });
        *handler_id.borrow_mut() = Some(id);

        publish_update(&collector.state, generation, update(Some("eDP-1"), 1));
        assert!(
            activations.borrow().is_empty(),
            "initial state must establish only a baseline"
        );
        publish_update(&collector.state, generation, update(Some("DP-1"), 2));
        assert_eq!(
            *activations.borrow(),
            vec![WorkspaceActivation {
                output: "DP-1".to_owned(),
                workspace_index: 2,
            }]
        );
        publish_update(&collector.state, generation, update(Some("HDMI-A-1"), 3));
        assert_eq!(
            activations.borrow().len(),
            1,
            "disconnected callback must stay disconnected"
        );
        collector.shutdown();
    }

    #[test]
    fn shutdown_and_stale_generation_never_emit_activation() {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let observers = observers();
        let collector = NiriCollector::stopped(Rc::clone(&publication), Rc::clone(&observers));
        let calls = Rc::new(RefCell::new(0));
        let calls_for_callback = Rc::clone(&calls);
        observers
            .borrow_mut()
            .connect(move |_| *calls_for_callback.borrow_mut() += 1);
        let generation = {
            let mut state = collector.state.borrow_mut();
            state.stopped = false;
            state.generation
        };

        publish_update(&collector.state, generation, update(Some("eDP-1"), 1));
        publish_update(
            &collector.state,
            generation.wrapping_sub(1),
            update(Some("DP-1"), 2),
        );
        collector.shutdown();
        publish_update(&collector.state, generation, update(Some("DP-1"), 2));
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn stale_generation_and_shutdown_are_never_current() {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let collector = NiriCollector::stopped(publication, observers());
        let state = collector.state.borrow();
        assert!(!is_current(&state, state.generation));
        assert!(!is_current(&state, state.generation.wrapping_sub(1)));
    }

    #[test]
    fn shutdown_is_idempotent_and_clears_publication() {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let collector = NiriCollector::stopped(Rc::clone(&publication), observers());
        collector.shutdown();
        collector.shutdown();
        assert_eq!(publication.borrow().snapshot["available"], false);
        assert!(publication.borrow().workspace_indices.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gio_lifecycle_reconnects_recovers_and_reaps_child_on_shutdown() {
        let _serial = DEFAULT_MAIN_CONTEXT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let context = glib::MainContext::default();
        let _context_owner = context
            .acquire()
            .expect("test must own the default GLib main context");
        let deadline = Instant::now() + Duration::from_secs(5);
        let directory = TestDirectory::new();
        let executable = directory.join("fake-niri-event-stream");
        let generation_file = directory.join("generation");
        let pid_file = directory.join("live-child-pid");

        let script = [
            "#!/bin/sh".to_owned(),
            format!("STATE={}", shell_quote(&generation_file)),
            format!("PID_FILE={}", shell_quote(&pid_file)),
            "generation=$(cat \"$STATE\" 2>/dev/null || printf 0)".to_owned(),
            "generation=$((generation + 1))".to_owned(),
            "printf '%s\\n' \"$generation\" > \"$STATE\"".to_owned(),
            "case \"$generation\" in".to_owned(),
            "  1)".to_owned(),
            "    printf '%s\\n' '{\"WorkspacesChanged\":{\"workspaces\":\"malformed\"}}'"
                .to_owned(),
            "    ;;".to_owned(),
            "  2)".to_owned(),
            "    printf '%s\\n' '{\"WorkspacesChanged\":{\"workspaces\":[{\"id\":99,\"idx\":9,\"name\":null,\"output\":\"stale-output\",\"is_urgent\":false,\"is_active\":true,\"is_focused\":true,\"active_window_id\":null}]}}'"
                .to_owned(),
            "    ;;".to_owned(),
            "  *)".to_owned(),
            "    printf '%s\\n' \"$$\" > \"$PID_FILE\"".to_owned(),
            "    printf '%s\\n' '{\"WorkspacesChanged\":{\"workspaces\":[{\"id\":41,\"idx\":3,\"name\":\"recovered\",\"output\":\"HDMI-A-1\",\"is_urgent\":false,\"is_active\":true,\"is_focused\":true,\"active_window_id\":7}]}}'"
                .to_owned(),
            "    printf '%s\\n' '{\"WindowsChanged\":{\"windows\":[{\"id\":7,\"title\":\"Recovered Window\",\"app_id\":\"integration.test\",\"is_focused\":true,\"is_urgent\":false,\"layout\":{},\"focus_timestamp\":null}]}}'"
                .to_owned(),
            "    exec /bin/sleep 30".to_owned(),
            "    ;;".to_owned(),
            "esac".to_owned(),
            String::new(),
        ]
        .join("\n");
        fs::write(&executable, script).expect("fake executable must be written");
        let mut permissions = fs::metadata(&executable)
            .expect("fake executable metadata must exist")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).expect("fake executable must be executable");

        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let observers = observers();
        let activations = Rc::new(RefCell::new(Vec::new()));
        let activations_for_handler = Rc::clone(&activations);
        observers
            .borrow_mut()
            .connect(move |activation| activations_for_handler.borrow_mut().push(activation));
        let collector = CollectorShutdownGuard(NiriCollector::start_for_test(
            Rc::clone(&publication),
            observers,
            vec![executable.into_os_string()],
            Duration::from_millis(10),
        ));

        pump_main_context_until(&context, deadline, || {
            read_generation(&generation_file) == Some(2)
                && publication.borrow().snapshot["available"] == false
        });
        assert!(
            publication.borrow().workspace_indices.is_empty(),
            "a partial generation must not publish workspace indices"
        );

        let expected = serde_json::json!({
            "available": true,
            "focusedOutput": { "available": true, "name": "HDMI-A-1" },
            "workspaces": {
                "available": true,
                "items": [{
                    "id": 41,
                    "index": 3,
                    "name": "recovered",
                    "output": "HDMI-A-1",
                    "isActive": true,
                    "isFocused": true
                }]
            },
            "focusedWindows": {
                "available": true,
                "items": [{
                    "output": "HDMI-A-1",
                    "window": {
                        "title": "Recovered Window",
                        "appId": "integration.test"
                    }
                }]
            }
        });
        pump_main_context_until(&context, deadline, || {
            publication.borrow().snapshot == expected
        });
        assert_eq!(publication.borrow().workspace_indices, vec![3]);
        assert_eq!(read_generation(&generation_file), Some(3));
        assert!(
            activations.borrow().is_empty(),
            "a reconnect generation's initial coherent state must only establish its baseline"
        );

        let child_pid: u32 = fs::read_to_string(&pid_file)
            .expect("live generation must record its pid")
            .trim()
            .parse()
            .expect("recorded child pid must be numeric");
        let child_proc = PathBuf::from(format!("/proc/{child_pid}"));
        assert!(
            child_proc.exists(),
            "live fake child must exist before shutdown"
        );

        collector.0.shutdown();
        assert_eq!(publication.borrow().snapshot["available"], false);
        assert!(publication.borrow().workspace_indices.is_empty());
        pump_main_context_until(&context, deadline, || !child_proc.exists());

        let shutdown_generation = read_generation(&generation_file);
        let stability_deadline = Instant::now() + Duration::from_millis(80);
        while Instant::now() < stability_deadline {
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            read_generation(&generation_file),
            shutdown_generation,
            "shutdown must not leave or restart a reconnect timer"
        );
        assert_eq!(shutdown_generation, Some(3));
        assert_eq!(publication.borrow().snapshot["available"], false);
        assert!(publication.borrow().workspace_indices.is_empty());
        assert!(
            !child_proc.exists(),
            "shutdown child must be gone and reaped, not left as a zombie"
        );
    }
}
