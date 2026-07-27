//! Pure domain model for the Youth Timer.
//!
//! This module has no dependency on `youth_sdk`, WASI, or any host type. It
//! is compiled and tested on the host target as ordinary Rust, mirroring the
//! discipline in the `youth-calculator` reference app: the model is the
//! tested core, and `app.rs` is a thin adapter that reads/writes it through
//! typed state and renders it through the SDK's builders.
//!
//! # Gate C-1: no tick source
//!
//! Gate A stood in for the missing host clock with manual `Advance`
//! commands. Gate C-1 deleted them, along with the `remaining_seconds`
//! they decremented, because that value was only ever moved by button
//! presses and so never corresponded to real elapsed time — carrying it
//! forward would have preserved a fiction (see `../FINDINGS.md`,
//! TIMER-F003).
//!
//! This model therefore has **no way to advance time**. Modes,
//! bounded configuration, session counting, and persistence still work;
//! nothing counts down. Gate C-2 restores that by arming a real host
//! schedule through `context.time()` and reacting to an autonomous
//! `ScheduleElapsed` delivery.

/// The Timer's application mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// No active session. The configured duration may be changed.
    Idle,
    /// A session is counting down.
    Running,
    /// A session is frozen at its current remaining duration.
    Paused,
    /// The configured duration has been fully consumed and is awaiting
    /// acknowledgement.
    Elapsed,
}

/// The largest configurable duration: 99 minutes 59 seconds, matching the
/// design's seconds-precision bound and the `MM:SS` display format.
pub const MAX_SECONDS: u64 = 99 * 60 + 59;

/// The canonical, durable application state.
///
/// Every field here is what the design's "Application model" section says
/// the guest should persist: mode, configured duration, and a
/// completed-session count. There is no native timestamp and no host clock
/// reading. Gate C-1 removed `remaining_seconds` and the guest-invented
/// `generation`: the former was only ever decremented by manual `Advance`
/// clicks and so never tracked real time, and the latter is replaced by a
/// host-issued schedule generation in C-2 (TIMER-F003, TIMER-F008). A
/// paused session's remainder becomes host-owned, not guest state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timer {
    mode: Mode,
    configured_seconds: u64,
    completed_sessions: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mode: Mode::Idle,
            configured_seconds: 0,
            completed_sessions: 0,
        }
    }

    /// Reconstructs a timer directly from previously-stored field values,
    /// bypassing the transition methods. Used only by `app.rs`'s `load`,
    /// which calls [`Timer::is_valid`] immediately afterward — this
    /// constructor does not itself guarantee the invariants those methods
    /// maintain, because its whole purpose is to let `load` detect a
    /// corrupted record rather than have one silently coerced into shape.
    #[must_use]
    pub const fn from_parts(mode: Mode, configured_seconds: u64, completed_sessions: u64) -> Self {
        Self {
            mode,
            configured_seconds,
            completed_sessions,
        }
    }

    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub const fn configured_seconds(&self) -> u64 {
        self.configured_seconds
    }

    #[must_use]
    pub const fn completed_sessions(&self) -> u64 {
        self.completed_sessions
    }

    /// Adjusts the configured duration by `delta` seconds (which may be
    /// negative), clamped to `[0, MAX_SECONDS]`. Idle only: a running,
    /// paused, or elapsed session's duration is fixed for that session.
    ///
    /// Returns whether the configured duration actually changed.
    pub fn add_seconds(&mut self, delta: i64) -> bool {
        if !matches!(self.mode, Mode::Idle) {
            return false;
        }
        let signed = i64::try_from(self.configured_seconds).unwrap_or(i64::MAX);
        let proposed = signed.saturating_add(delta).max(0);
        let clamped = u64::try_from(proposed).unwrap_or(u64::MAX).min(MAX_SECONDS);
        if clamped == self.configured_seconds {
            return false;
        }
        self.configured_seconds = clamped;
        true
    }

    /// Starts a new session from the configured duration. Idle only, and
    /// only if a nonzero duration has been configured. Begins a new
    /// generation: a stale reference to a prior session (e.g. a delayed
    /// wakeup on a platform with real scheduling) would no longer name the
    /// active one.
    pub fn start(&mut self) -> bool {
        if !matches!(self.mode, Mode::Idle) || self.configured_seconds == 0 {
            return false;
        }
        self.mode = Mode::Running;
        true
    }

    /// Freezes the remaining duration. Running only.
    pub fn pause(&mut self) -> bool {
        if !matches!(self.mode, Mode::Running) {
            return false;
        }
        self.mode = Mode::Paused;
        true
    }

    /// Continues counting down from the frozen remaining duration. Paused
    /// only. No new generation: this is the same session continuing, not a
    /// new one.
    pub fn resume(&mut self) -> bool {
        if !matches!(self.mode, Mode::Paused) {
            return false;
        }
        self.mode = Mode::Running;
        true
    }

    /// Abandons the active session and returns to Idle at the configured
    /// duration. Running or Paused only.
    pub fn cancel(&mut self) -> bool {
        if !matches!(self.mode, Mode::Running | Mode::Paused) {
            return false;
        }
        self.mode = Mode::Idle;
        true
    }

    /// Returns to Idle at the configured duration from any mode, and
    /// begins a new generation. Unlike `cancel`, this is valid from
    /// `Elapsed` too (it is the "start fresh" operation), and unlike
    /// `acknowledge`, it does not require having reached `Elapsed` first.
    pub fn reset(&mut self) -> bool {
        let changed = !matches!(self.mode, Mode::Idle);
        self.mode = Mode::Idle;
        changed
    }

    /// Acknowledges an elapsed session and returns to Idle at the
    /// configured duration. Elapsed only — this is deliberately not the
    /// same operation as `reset` (see the design's "Elapsed delivery":
    /// the host would redeliver the event until an equivalent
    /// acknowledgement commits).
    pub fn acknowledge(&mut self) -> bool {
        if !matches!(self.mode, Mode::Elapsed) {
            return false;
        }
        self.mode = Mode::Idle;
        true
    }

    /// The `MM:SS` countdown text. Shared by `view` and `handle` in
    /// `app.rs` so the displayed string can never diverge from the model
    /// that produced it (the discipline `youth-calculator` calls
    /// CALC-F009).
    /// Gate C-1 has no tick source, so this reports the configured
    /// duration. C-2 replaces it with a host-owned countdown node reading
    /// the live schedule (TIMER-F004); the guest never formats elapsed
    /// time from its own state again.
    #[must_use]
    pub fn display(&self) -> String {
        format_seconds(self.configured_seconds)
    }

    /// Applies one command, dispatching to the operation methods above.
    /// `app.rs` calls this from `handle`, exactly as the calculator's
    /// `Model::apply` does, so both applications share one shape: load,
    /// clone, apply, compare, save-if-changed.
    pub fn apply(&mut self, command: Command) {
        match command {
            Command::AddSeconds(delta) => {
                self.add_seconds(delta);
            }
            Command::Start => {
                self.start();
            }
            Command::Pause => {
                self.pause();
            }
            Command::Resume => {
                self.resume();
            }
            Command::Cancel => {
                self.cancel();
            }
            Command::Reset => {
                self.reset();
            }
            Command::Acknowledge => {
                self.acknowledge();
            }
            Command::DismissNotice => {}
        }
    }

    /// Checks the invariants a loaded (and therefore potentially
    /// hand-edited or corrupted) state record must satisfy. Mirrors the
    /// calculator's defensive `Model::is_valid` check at load time.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.configured_seconds <= MAX_SECONDS
    }
}

/// One user-issued operation on a [`Timer`]. Gate C-1 removed
/// `Command::Advance`: nothing in this application advances time any more,
/// and C-2 replaces it with an autonomous host `ScheduleElapsed` delivery
/// rather than a command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AddSeconds(i64),
    Start,
    Pause,
    Resume,
    Cancel,
    Reset,
    Acknowledge,
    /// Clears the migration recovery notice. Purely presentational: it
    /// changes no timer state, which is exactly why it exercises the
    /// rule that a no-op command still completes a pending rewrite.
    DismissNotice,
}

#[must_use]
fn format_seconds(total: u64) -> String {
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes:02}:{seconds:02}")
}

/// The stable, storage-facing name for a [`Mode`]. Used by `app.rs` when
/// persisting the `mode` state key.
#[must_use]
pub const fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Idle => "idle",
        Mode::Running => "running",
        Mode::Paused => "paused",
        Mode::Elapsed => "elapsed",
    }
}

/// The inverse of [`mode_name`]. `None` for any string that is not one of
/// the four stored names, so `app.rs` can turn a corrupted `mode` value
/// into an explicit error instead of silently defaulting.
#[must_use]
pub fn parse_mode(value: &str) -> Option<Mode> {
    match value {
        "idle" => Some(Mode::Idle),
        "running" => Some(Mode::Running),
        "paused" => Some(Mode::Paused),
        "elapsed" => Some(Mode::Elapsed),
        _ => None,
    }
}

/// A Gate A (v1) durable record, already parsed into typed values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyRecord {
    pub mode: Mode,
    pub configured_seconds: u64,
    pub completed_sessions: u64,
}

/// The result of classifying a legacy record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migrated {
    pub model: Timer,
    /// A live session was reset, so the user must be told.
    pub recovered: bool,
}

/// Classifies a Gate A record into the current model.
///
/// The policy is deliberately conservative and lives here, in the pure
/// model, so it is host-testable: `app.rs` only performs the state I/O.
///
/// No deadline and no remaining duration is ever invented, because none
/// ever existed — Gate A's `remaining_seconds` was moved only by manual
/// `Advance` presses. A legacy `running` or `paused` session therefore
/// becomes idle at its configured duration and raises a recovery notice,
/// rather than being reinterpreted as an active schedule. `elapsed` keeps
/// its meaning, and `completed_sessions` survives every path because it is
/// real application meaning.
#[must_use]
pub const fn migrate_legacy(record: LegacyRecord) -> Migrated {
    let (mode, recovered) = match record.mode {
        Mode::Running | Mode::Paused => (Mode::Idle, true),
        Mode::Idle => (Mode::Idle, false),
        Mode::Elapsed => (Mode::Elapsed, false),
    };
    Migrated {
        model: Timer::from_parts(mode, record.configured_seconds, record.completed_sessions),
        recovered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_timer_is_idle_and_zeroed() {
        let timer = Timer::new();
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.configured_seconds(), 0);
        assert_eq!(timer.completed_sessions(), 0);
        assert!(timer.is_valid());
    }

    #[test]
    fn add_seconds_clamps_to_the_bounds_and_is_idle_only() {
        let mut timer = Timer::new();
        assert!(
            !timer.add_seconds(-30),
            "already zero, so clamping reports no change"
        );
        assert_eq!(timer.configured_seconds(), 0, "cannot go below zero");

        assert!(timer.add_seconds(90));
        assert_eq!(timer.configured_seconds(), 90);

        assert!(timer.add_seconds(i64::MAX));
        assert_eq!(timer.configured_seconds(), MAX_SECONDS);
        assert!(!timer.add_seconds(10), "already at the maximum");

        timer.start();
        assert!(
            !timer.add_seconds(10),
            "configuration is fixed once started"
        );
    }

    #[test]
    fn start_requires_idle_and_a_nonzero_duration() {
        let mut timer = Timer::new();
        assert!(!timer.start(), "nothing configured");

        timer.add_seconds(60);
        assert!(timer.start());
        assert_eq!(timer.mode(), Mode::Running);
        assert!(!timer.start(), "already running");
    }

    #[test]
    fn pause_and_resume_are_state_gated() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.pause(), "idle cannot pause");

        timer.start();
        assert!(timer.pause());
        assert_eq!(timer.mode(), Mode::Paused);
        assert!(!timer.pause(), "already paused");

        assert!(timer.resume());
        assert_eq!(timer.mode(), Mode::Running);
        assert!(!timer.resume(), "already running");
    }

    #[test]
    fn cancel_returns_to_idle_from_running_or_paused_only() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.cancel(), "idle has nothing to cancel");

        timer.start();
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.configured_seconds(), 60, "configuration survives");

        timer.start();
        timer.pause();
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
    }

    #[test]
    fn reset_returns_to_idle_from_any_mode() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        timer.start();
        assert!(timer.reset());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.configured_seconds(), 60);
        assert!(!timer.reset(), "already idle: nothing changed");
    }

    #[test]
    fn acknowledge_is_elapsed_only() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.acknowledge(), "idle has nothing to acknowledge");
        timer.start();
        assert!(!timer.acknowledge(), "running has nothing to acknowledge");
    }

    #[test]
    fn completed_sessions_are_preserved_across_transitions() {
        // Sessions are application meaning, distinct from any host
        // schedule generation (TIMER-F008). Nothing in C-1 increments
        // them, since nothing elapses; C-2 does so on ScheduleElapsed.
        let timer = Timer::from_parts(Mode::Elapsed, 300, 7);
        assert_eq!(timer.completed_sessions(), 7);
        let mut timer = timer;
        assert!(timer.reset());
        assert_eq!(timer.completed_sessions(), 7, "reset preserves sessions");
    }

    #[test]
    fn display_formats_the_configured_duration() {
        let mut timer = Timer::new();
        assert_eq!(timer.display(), "00:00");
        timer.add_seconds(61);
        assert_eq!(timer.display(), "01:01");
        timer.add_seconds(i64::MAX);
        assert_eq!(timer.display(), "99:59");
    }

    #[test]
    fn is_valid_rejects_an_out_of_range_configuration() {
        assert!(Timer::from_parts(Mode::Idle, MAX_SECONDS, 0).is_valid());
        assert!(!Timer::from_parts(Mode::Idle, MAX_SECONDS + 1, 0).is_valid());
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in [Mode::Idle, Mode::Running, Mode::Paused, Mode::Elapsed] {
            assert_eq!(parse_mode(mode_name(mode)), Some(mode));
        }
        assert_eq!(parse_mode("advancing"), None);
    }

    #[test]
    fn apply_dispatches_every_command() {
        let mut timer = Timer::new();
        timer.apply(Command::AddSeconds(120));
        assert_eq!(timer.configured_seconds(), 120);
        timer.apply(Command::Start);
        assert_eq!(timer.mode(), Mode::Running);
        timer.apply(Command::Pause);
        assert_eq!(timer.mode(), Mode::Paused);
        timer.apply(Command::Resume);
        assert_eq!(timer.mode(), Mode::Running);
        timer.apply(Command::Cancel);
        assert_eq!(timer.mode(), Mode::Idle);
        timer.apply(Command::Reset);
        assert_eq!(timer.mode(), Mode::Idle);
        timer.apply(Command::Acknowledge);
        assert_eq!(timer.mode(), Mode::Idle);
    }

    fn legacy(mode: Mode, configured: u64, sessions: u64) -> LegacyRecord {
        LegacyRecord {
            mode,
            configured_seconds: configured,
            completed_sessions: sessions,
        }
    }

    #[test]
    fn legacy_idle_migrates_unchanged_without_a_notice() {
        let out = migrate_legacy(legacy(Mode::Idle, 300, 4));
        assert_eq!(out.model.mode(), Mode::Idle);
        assert_eq!(out.model.configured_seconds(), 300);
        assert_eq!(out.model.completed_sessions(), 4);
        assert!(!out.recovered);
    }

    #[test]
    fn legacy_running_never_fabricates_a_schedule() {
        // Gate A's running timer was never running against real time, so
        // migration must not present it as an active session.
        let out = migrate_legacy(legacy(Mode::Running, 300, 2));
        assert_eq!(out.model.mode(), Mode::Idle, "no deadline is invented");
        assert_eq!(out.model.configured_seconds(), 300);
        assert_eq!(out.model.completed_sessions(), 2);
        assert!(out.recovered, "the user must be told it was reset");
    }

    #[test]
    fn legacy_paused_is_reset_for_the_same_reason() {
        let out = migrate_legacy(legacy(Mode::Paused, 90, 1));
        assert_eq!(out.model.mode(), Mode::Idle);
        assert_eq!(out.model.configured_seconds(), 90);
        assert!(out.recovered);
    }

    #[test]
    fn legacy_elapsed_preserves_its_meaning_and_sessions() {
        let out = migrate_legacy(legacy(Mode::Elapsed, 120, 9));
        assert_eq!(out.model.mode(), Mode::Elapsed);
        assert_eq!(
            out.model.completed_sessions(),
            9,
            "sessions are real meaning"
        );
        assert!(!out.recovered, "elapsed was not a live session");
    }

    #[test]
    fn completed_sessions_survive_every_migration_path() {
        for mode in [Mode::Idle, Mode::Running, Mode::Paused, Mode::Elapsed] {
            assert_eq!(
                migrate_legacy(legacy(mode, 60, 11))
                    .model
                    .completed_sessions(),
                11,
                "{mode:?} lost its session count"
            );
        }
    }

    #[test]
    fn migration_is_idempotent_on_its_own_output() {
        // Re-classifying an already-migrated record must be a fixed point:
        // idle stays idle and elapsed stays elapsed, with no second notice.
        for mode in [Mode::Idle, Mode::Running, Mode::Paused, Mode::Elapsed] {
            let once = migrate_legacy(legacy(mode, 300, 3));
            let twice = migrate_legacy(legacy(
                once.model.mode(),
                once.model.configured_seconds(),
                once.model.completed_sessions(),
            ));
            assert_eq!(twice.model, once.model, "{mode:?} is not a fixed point");
            assert!(!twice.recovered, "{mode:?} re-raised a recovery notice");
        }
    }
}
