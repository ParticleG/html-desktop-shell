use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::OsStr,
    rc::{Rc, Weak},
    time::Duration,
};

use gtk4::gio::{self, prelude::*};
use serde_json::{Value, json};

use super::Provider;
use crate::messages::NativeMethodError;

const DEFAULT_SINK: &str = "@DEFAULT_AUDIO_SINK@";
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const STOPPED_REASON: &str = "Audio controller is stopped";

// Requests and periodic observations share one serial subprocess lane. In particular,
// a collector started before a write can never satisfy that write's observation.
#[derive(Clone)]
pub struct AudioController {
    state: Rc<RefCell<State>>,
}

struct State {
    stopped: bool,
    snapshot: Value,
    poll: Option<glib::SourceId>,
    next_id: u64,
    active: Option<Active>,
    pending: VecDeque<Request>,
}

struct Active {
    id: u64,
    stage: Stage,
    child: gio::Subprocess,
    cancellable: gio::Cancellable,
    timeout: Option<glib::SourceId>,
    output: Option<Result<glib::GString, Failure>>,
    wait_done: bool,
    wait_ok: bool,
    timed_out: bool,
}

// Storage is necessary because queued requests survive multiple asynchronous stages.
struct Request {
    change: Change,
    callback: Box<dyn FnOnce(Result<Value, NativeMethodError>)>,
}

#[derive(Clone, Copy)]
enum Change {
    Volume(u32),
    Muted(bool),
}

enum Stage {
    Poll,
    Write(Request),
    Observe(Request),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Observation {
    volume: f64,
    muted: bool,
}

impl Observation {
    fn snapshot(self) -> Value {
        json!({ "available": true, "volume": self.volume, "muted": self.muted })
    }
}

impl Change {
    fn matches(self, observed: Observation) -> bool {
        match self {
            // wpctl displays a rounded volume. Require the requested integer percent
            // to be its unambiguous nearest integer, without clamping amplification.
            Self::Volume(volume) => (observed.volume - f64::from(volume)).abs() < 0.5,
            Self::Muted(muted) => observed.muted == muted,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Unavailable,
    Command,
    InvalidOutput,
    Timeout,
}

impl Failure {
    fn reason(self) -> &'static str {
        match self {
            Self::Unavailable => "WirePlumber audio control is unavailable",
            Self::Command => "Default audio output is unavailable",
            Self::InvalidOutput => "WirePlumber returned invalid audio state",
            Self::Timeout => "WirePlumber audio control timed out",
        }
    }

    fn error(self) -> NativeMethodError {
        NativeMethodError::new("audio_unavailable", self.reason())
    }
}

impl AudioController {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(State {
            stopped: false,
            snapshot: unavailable("Audio state has not been observed"),
            poll: None,
            next_id: 0,
            active: None,
            pending: VecDeque::new(),
        }));
        let weak = Rc::downgrade(&state);
        let poll = glib::timeout_add_local(POLL_INTERVAL, move || {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if state.borrow().stopped {
                return glib::ControlFlow::Break;
            }
            pump(&state, true);
            glib::ControlFlow::Continue
        });
        state.borrow_mut().poll = Some(poll);
        pump(&state, true);
        Self { state }
    }

    pub fn snapshot(&self) -> Value {
        self.state.borrow().snapshot.clone()
    }

    pub fn set_volume(
        &self,
        volume: u32,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        if volume > 100 {
            callback(Err(NativeMethodError::new(
                "invalid_params",
                "Audio volume must be an integer from 0 to 100",
            )));
            return;
        }
        self.enqueue(Change::Volume(volume), callback);
    }

    pub fn set_muted(
        &self,
        muted: bool,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        self.enqueue(Change::Muted(muted), callback);
    }

    fn enqueue(
        &self,
        change: Change,
        callback: impl FnOnce(Result<Value, NativeMethodError>) + 'static,
    ) {
        let error = {
            let state = self.state.borrow();
            if state.stopped {
                Some(stopped_error())
            } else if state.snapshot["available"] != true {
                Some(NativeMethodError::new(
                    "audio_unavailable",
                    "Default audio output is unavailable",
                ))
            } else {
                None
            }
        };
        if let Some(error) = error {
            callback(Err(error));
            return;
        }
        self.state.borrow_mut().pending.push_back(Request {
            change,
            callback: Box::new(callback),
        });
        pump(&self.state, false);
    }

    pub fn shutdown(&self) {
        let (poll, active, pending) = {
            let mut state = self.state.borrow_mut();
            if state.stopped {
                return;
            }
            state.stopped = true;
            state.snapshot = unavailable(STOPPED_REASON);
            (
                state.poll.take(),
                state.active.take(),
                std::mem::take(&mut state.pending),
            )
        };
        if let Some(poll) = poll {
            poll.remove();
        }
        if let Some(mut active) = active {
            cancel_active(&mut active);
            if let Some(request) = active.stage.into_request() {
                (request.callback)(Err(stopped_error()));
            }
        }
        for request in pending {
            (request.callback)(Err(stopped_error()));
        }
    }
}

impl Provider for AudioController {
    fn name(&self) -> &'static str {
        "audio"
    }

    fn snapshot(&self) -> Value {
        AudioController::snapshot(self)
    }

    fn diagnostic_snapshot(&self) -> Value {
        let state = self.state.borrow();
        if state.snapshot["available"] == true {
            json!({ "available": true })
        } else {
            state.snapshot.clone()
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(poll) = self.poll.take() {
            poll.remove();
        }
        if let Some(active) = self.active.as_mut() {
            cancel_active(active);
        }
    }
}

impl Stage {
    fn into_request(self) -> Option<Request> {
        match self {
            Self::Poll => None,
            Self::Write(request) | Self::Observe(request) => Some(request),
        }
    }
}

fn unavailable(reason: &str) -> Value {
    json!({ "available": false, "reason": reason })
}

fn stopped_error() -> NativeMethodError {
    NativeMethodError::new("audio_stopped", STOPPED_REASON)
}

fn cancel_active(active: &mut Active) {
    if let Some(timeout) = active.timeout.take() {
        timeout.remove();
    }
    active.cancellable.cancel();
    if !active.wait_done {
        active.child.force_exit();
    }
    // The separate, uncancellable wait_async remains registered and reaps the child,
    // even after shutdown or destruction makes the controller's weak reference stale.
}

fn pump(state: &Rc<RefCell<State>>, poll: bool) {
    loop {
        let (stage, rejected) = {
            let mut state = state.borrow_mut();
            if state.stopped || state.active.is_some() {
                return;
            }
            if let Some(request) = state.pending.pop_front() {
                if state.snapshot["available"] == true {
                    (Some(Stage::Write(request)), None)
                } else {
                    (None, Some(request))
                }
            } else {
                (poll.then_some(Stage::Poll), None)
            }
        };
        if let Some(request) = rejected {
            (request.callback)(Err(NativeMethodError::new(
                "audio_unavailable",
                "Default audio output is unavailable",
            )));
            // Earlier requests may have invalidated availability while this request
            // was queued. Re-check both lifecycle and availability after callbacks.
            continue;
        }
        if let Some(stage) = stage {
            start_command(state, stage);
        }
        return;
    }
}

fn start_command(state: &Rc<RefCell<State>>, stage: Stage) {
    let id = {
        let mut state = state.borrow_mut();
        if state.stopped {
            drop(state);
            if let Some(request) = stage.into_request() {
                (request.callback)(Err(stopped_error()));
            }
            return;
        }
        state.next_id = state.next_id.wrapping_add(1);
        state.next_id
    };
    let query_args = [
        OsStr::new("wpctl"),
        OsStr::new("get-volume"),
        OsStr::new(DEFAULT_SINK),
    ];
    let volume_arg;
    let write_args;
    let args: &[&OsStr] = match &stage {
        Stage::Poll | Stage::Observe(_) => &query_args,
        Stage::Write(request) => {
            let (command, value) = match request.change {
                Change::Volume(volume) => {
                    volume_arg = format!("{volume}%");
                    ("set-volume", volume_arg.as_str())
                }
                Change::Muted(muted) => ("set-mute", if muted { "1" } else { "0" }),
            };
            write_args = [
                OsStr::new("wpctl"),
                OsStr::new(command),
                OsStr::new(DEFAULT_SINK),
                OsStr::new(value),
            ];
            &write_args
        }
    };
    let child = match gio::Subprocess::newv(
        args,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    ) {
        Ok(child) => child,
        Err(_) => {
            complete_stage(state, stage, Err(Failure::Unavailable));
            return;
        }
    };
    let cancellable = gio::Cancellable::new();
    state.borrow_mut().active = Some(Active {
        id,
        stage,
        child: child.clone(),
        cancellable: cancellable.clone(),
        timeout: None,
        output: None,
        wait_done: false,
        wait_ok: false,
        timed_out: false,
    });

    let weak = Rc::downgrade(state);
    child.wait_async(gio::Cancellable::NONE, move |result| {
        let Some(state) = current(&weak, id) else {
            return;
        };
        {
            let mut state = state.borrow_mut();
            let active = state.active.as_mut().expect("current child");
            active.wait_done = true;
            active.wait_ok = result.is_ok() && active.child.is_successful();
        }
        finish_if_complete(&state, id);
    });
    let weak = Rc::downgrade(state);
    child.communicate_utf8_async(None, Some(&cancellable), move |result| {
        let Some(state) = current(&weak, id) else {
            return;
        };
        let output = result
            .map(|(stdout, _)| stdout.unwrap_or_default())
            .map_err(|_| Failure::Command);
        state
            .borrow_mut()
            .active
            .as_mut()
            .expect("current child")
            .output = Some(output);
        finish_if_complete(&state, id);
    });
    let weak = Rc::downgrade(state);
    let timeout = glib::timeout_add_local_once(COMMAND_TIMEOUT, move || {
        let Some(state) = current(&weak, id) else {
            return;
        };
        let (cancellable, child, wait_done) = {
            let mut state = state.borrow_mut();
            state.snapshot = unavailable(Failure::Timeout.reason());
            let active = state.active.as_mut().expect("current child");
            active.timeout.take();
            active.timed_out = true;
            (
                active.cancellable.clone(),
                active.child.clone(),
                active.wait_done,
            )
        };
        cancellable.cancel();
        if !wait_done {
            child.force_exit();
        }
    });
    state
        .borrow_mut()
        .active
        .as_mut()
        .expect("new child")
        .timeout = Some(timeout);
}

fn current(weak: &Weak<RefCell<State>>, id: u64) -> Option<Rc<RefCell<State>>> {
    let state = weak.upgrade()?;
    let is_current = {
        let state = state.borrow();
        !state.stopped && state.active.as_ref().is_some_and(|active| active.id == id)
    };
    is_current.then_some(state)
}

fn finish_if_complete(state: &Rc<RefCell<State>>, id: u64) {
    let active = {
        let mut state = state.borrow_mut();
        if state.stopped
            || !state.active.as_ref().is_some_and(|active| {
                active.id == id && active.wait_done && active.output.is_some()
            })
        {
            return;
        }
        state.active.take().expect("completed child")
    };
    let Active {
        stage,
        timeout,
        output,
        wait_ok,
        timed_out,
        ..
    } = active;
    if let Some(timeout) = timeout {
        timeout.remove();
    }
    let result = if timed_out {
        Err(Failure::Timeout)
    } else if !wait_ok {
        Err(Failure::Command)
    } else {
        output.expect("completed output")
    };
    complete_stage(state, stage, result);
}

fn complete_stage(
    state: &Rc<RefCell<State>>,
    stage: Stage,
    result: Result<glib::GString, Failure>,
) {
    match stage {
        Stage::Write(request) => match result {
            Ok(_) => {
                // The write's exit status alone is not a successful API response.
                start_command(state, Stage::Observe(request));
                return;
            }
            Err(error) => {
                state.borrow_mut().snapshot = unavailable(error.reason());
                (request.callback)(Err(error.error()));
            }
        },
        Stage::Poll => {
            let result = result.and_then(|output| parse_volume(&output));
            state.borrow_mut().snapshot = match result {
                Ok(observation) => observation.snapshot(),
                Err(error) => unavailable(error.reason()),
            };
        }
        Stage::Observe(request) => {
            let result = result.and_then(|output| parse_volume(&output));
            let response = match result {
                Ok(observation) => {
                    let snapshot = observation.snapshot();
                    state.borrow_mut().snapshot = snapshot.clone();
                    if request.change.matches(observation) {
                        Ok(snapshot)
                    } else {
                        Err(NativeMethodError::new(
                            "audio_action_failed",
                            "Audio output did not match the requested state",
                        ))
                    }
                }
                Err(error) => {
                    state.borrow_mut().snapshot = unavailable(error.reason());
                    Err(error.error())
                }
            };
            (request.callback)(response);
        }
    }
    // A callback can re-enter, queue another request, or stop the controller.
    pump(state, false);
}

fn parse_volume(output: &str) -> Result<Observation, Failure> {
    let line = output.trim();
    if line.contains(['\r', '\n']) {
        return Err(Failure::InvalidOutput);
    }
    let mut tokens = line.split_ascii_whitespace();
    if tokens.next() != Some("Volume:") {
        return Err(Failure::InvalidOutput);
    }
    let volume = tokens.next().ok_or(Failure::InvalidOutput)?;
    let muted = match tokens.next() {
        None => false,
        Some("[MUTED]") => true,
        Some(_) => return Err(Failure::InvalidOutput),
    };
    if tokens.next().is_some() || volume.len() > 128 {
        return Err(Failure::InvalidOutput);
    }
    let (whole, fraction) = volume.split_once('.').unwrap_or((volume, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || (volume.contains('.') && fraction.is_empty())
    {
        return Err(Failure::InvalidOutput);
    }
    // Shift the decimal point before parsing, so 0.29 is reported as 29 rather
    // than a floating-point multiplication artifact such as 28.999999999999996.
    let mut percent = String::with_capacity(volume.len() + 3);
    percent.push_str(whole);
    percent.push_str(&fraction[..fraction.len().min(2)]);
    for _ in fraction.len()..2 {
        percent.push('0');
    }
    if fraction.len() > 2 {
        percent.push('.');
        percent.push_str(&fraction[2..]);
    }
    let volume = percent.parse::<f64>().map_err(|_| Failure::InvalidOutput)?;
    if !volume.is_finite() || volume < 0.0 {
        return Err(Failure::InvalidOutput);
    }
    Ok(Observation { volume, muted })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_real_wpctl_states_without_clamping_amplification() {
        assert_eq!(
            parse_volume("Volume: 0.64\n"),
            Ok(Observation {
                volume: 64.0,
                muted: false,
            })
        );
        assert_eq!(
            parse_volume("Volume: 1.50 [MUTED]\n"),
            Ok(Observation {
                volume: 150.0,
                muted: true,
            })
        );
        assert_eq!(parse_volume("Volume: 0 [MUTED]").unwrap().volume, 0.0);
        assert_eq!(parse_volume("Volume: 1").unwrap().volume, 100.0);
        assert_eq!(parse_volume("Volume: 0.29").unwrap().volume, 29.0);
        assert_eq!(parse_volume("Volume: 0.635").unwrap().volume, 63.5);
        assert_eq!(parse_volume("Volume: 000.07").unwrap().volume, 7.0);
    }

    #[test]
    fn rejects_invalid_or_ambiguous_observations() {
        for output in [
            "",
            "Error: no default sink",
            "Volume:",
            "Volume: NaN",
            "Volume: inf",
            "Volume: -0.1",
            "Volume: -0",
            "Volume: +0.5",
            "Volume: 1e2",
            "Volume: .5",
            "Volume: 1.",
            "Volume: 1.2.3",
            "Volume: 64%",
            "Volume: 0.64 [UNMUTED]",
            "Volume: 0.64 [MUTED] trailing",
            "Volume: 0.64\nVolume: 0.12",
            "Volume:\n0.64",
            "Volume: ０.６４",
        ] {
            assert_eq!(
                parse_volume(output),
                Err(Failure::InvalidOutput),
                "{output:?}"
            );
        }
        assert_eq!(
            parse_volume(&format!("Volume: {}", "9".repeat(129))),
            Err(Failure::InvalidOutput)
        );
    }

    #[test]
    fn writes_require_observed_requested_state_not_old_or_unrelated_values() {
        let observed = Observation {
            volume: 29.0,
            muted: false,
        };
        assert!(Change::Volume(29).matches(observed));
        assert!(!Change::Volume(30).matches(observed));
        assert!(Change::Muted(false).matches(observed));
        assert!(!Change::Muted(true).matches(observed));
        assert!(!Change::Volume(100).matches(Observation {
            volume: 150.0,
            muted: false,
        }));
        for volume in [0, 100] {
            assert!(Change::Volume(volume).matches(Observation {
                volume: f64::from(volume),
                muted: true,
            }));
        }
        assert!(!Change::Volume(30).matches(Observation {
            volume: 29.5,
            muted: false,
        }));
    }

    fn dormant(snapshot: Value) -> AudioController {
        AudioController {
            state: Rc::new(RefCell::new(State {
                stopped: false,
                snapshot,
                poll: None,
                next_id: 0,
                active: None,
                pending: VecDeque::new(),
            })),
        }
    }

    #[test]
    fn invalid_unavailable_and_stopped_writes_never_dispatch() {
        let controller = dormant(unavailable("No default audio output"));
        controller.set_volume(101, |result| {
            assert_eq!(result.unwrap_err().code, "invalid_params");
        });
        controller.set_volume(0, |result| {
            assert_eq!(result.unwrap_err().code, "audio_unavailable");
        });
        controller.set_muted(true, |result| {
            assert_eq!(result.unwrap_err().code, "audio_unavailable");
        });
        controller.shutdown();
        controller.shutdown();
        controller.set_volume(100, |result| {
            assert_eq!(result.unwrap_err().code, "audio_stopped");
        });
        controller.set_muted(false, |result| {
            assert_eq!(result.unwrap_err().code, "audio_stopped");
        });
        assert_eq!(controller.snapshot(), unavailable(STOPPED_REASON));
        assert!(controller.state.borrow().active.is_none());
    }

    #[test]
    fn failed_collection_invalidates_cache_and_real_collection_recovers() {
        let controller = dormant(
            Observation {
                volume: 64.0,
                muted: false,
            }
            .snapshot(),
        );
        complete_stage(&controller.state, Stage::Poll, Err(Failure::Command));
        assert_eq!(controller.snapshot()["available"], false);
        assert!(controller.snapshot().get("volume").is_none());
        complete_stage(&controller.state, Stage::Poll, Ok("Volume: invalid".into()));
        assert_eq!(controller.snapshot()["available"], false);
        complete_stage(
            &controller.state,
            Stage::Poll,
            Ok("Volume: 1.12 [MUTED]".into()),
        );
        assert_eq!(
            controller.snapshot(),
            json!({ "available": true, "volume": 112.0, "muted": true })
        );
    }

    #[test]
    fn post_write_observation_failure_is_not_success_and_callbacks_can_reenter() {
        let controller = dormant(
            Observation {
                volume: 64.0,
                muted: false,
            }
            .snapshot(),
        );
        let callback_controller = controller.clone();
        complete_stage(
            &controller.state,
            Stage::Observe(Request {
                change: Change::Volume(64),
                callback: Box::new(move |result| {
                    assert_eq!(result.unwrap_err().code, "audio_unavailable");
                    assert_eq!(callback_controller.snapshot()["available"], false);
                    callback_controller.shutdown();
                }),
            }),
            Ok("Volume: NaN".into()),
        );
        assert_eq!(controller.snapshot(), unavailable(STOPPED_REASON));
    }

    #[test]
    fn availability_loss_rejects_already_queued_writes() {
        let controller = dormant(
            Observation {
                volume: 64.0,
                muted: false,
            }
            .snapshot(),
        );
        let calls = Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        let callback_controller = controller.clone();
        controller.state.borrow_mut().pending.push_back(Request {
            change: Change::Volume(30),
            callback: Box::new(move |result| {
                assert_eq!(result.unwrap_err().code, "audio_unavailable");
                assert_eq!(callback_controller.snapshot()["available"], false);
                callback_calls.set(callback_calls.get() + 1);
                callback_controller.shutdown();
            }),
        });
        complete_stage(&controller.state, Stage::Poll, Err(Failure::Command));
        assert_eq!(calls.get(), 1);
        assert_eq!(controller.snapshot(), unavailable(STOPPED_REASON));
    }

    #[test]
    fn shutdown_cancels_queued_requests_once_without_borrowing_during_callbacks() {
        let controller = dormant(
            Observation {
                volume: 64.0,
                muted: false,
            }
            .snapshot(),
        );
        let calls = Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        let callback_controller = controller.clone();
        controller.state.borrow_mut().pending.push_back(Request {
            change: Change::Muted(true),
            callback: Box::new(move |result| {
                assert_eq!(result.unwrap_err().code, "audio_stopped");
                assert_eq!(callback_controller.snapshot()["available"], false);
                callback_calls.set(callback_calls.get() + 1);
                callback_controller.shutdown();
            }),
        });
        controller.shutdown();
        controller.shutdown();
        assert_eq!(calls.get(), 1);
    }
}
