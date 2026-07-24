//! Pure domain model for the Youth Timer.
//!
//! This module has no dependency on `youth_sdk`, WASI, or any host type. It
//! is compiled and tested on the host target as ordinary Rust, mirroring the
//! discipline in the `youth-calculator` reference app: the model is the
//! tested core, and `app.rs` is a thin adapter that reads/writes it through
//! typed state and renders it through the SDK's builders.
//!
//! # Why time is a manual step, not a clock
//!
//! Current Youth exposes no clock, tick, or scheduled wakeup to a guest
//! (see `../FINDINGS.md`, TIMER-F001/F002). `advance` stands in for the
//! host-owned clock the design calls for: it is the one operation that
//! would, on a platform with `youth:time`, be driven by a host schedule
//! instead of a button or a test command. Everything else here — the mode
//! machine, bounded configuration, session counting, and formatting — is
//! genuinely expressible today and is exercised by the tests below and by
//! `tests/basic.youth-test` against the real headless runtime.

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

/// The default step size for the manual "advance" commands.
pub const ADVANCE_SMALL_SECONDS: u64 = 1;
pub const ADVANCE_LARGE_SECONDS: u64 = 10;

/// The canonical, durable application state.
///
/// Every field here is exactly what the design's "Application model"
/// section says the guest should persist: mode, configured duration, an
/// opaque generation, and a completed-session count. There is no field for
/// a native timestamp or a host clock reading, because none exists to
/// persist (TIMER-F003 records the consequence: `remaining_seconds` has to
/// live here today, standing in for what a host schedule would own).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timer {
    mode: Mode,
    configured_seconds: u64,
    remaining_seconds: u64,
    generation: u64,
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
            remaining_seconds: 0,
            generation: 0,
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
    pub const fn from_parts(
        mode: Mode,
        configured_seconds: u64,
        remaining_seconds: u64,
        generation: u64,
        completed_sessions: u64,
    ) -> Self {
        Self {
            mode,
            configured_seconds,
            remaining_seconds,
            generation,
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
    pub const fn remaining_seconds(&self) -> u64 {
        self.remaining_seconds
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
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
        self.remaining_seconds = clamped;
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
        self.remaining_seconds = self.configured_seconds;
        self.generation = self.generation.wrapping_add(1);
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
        self.remaining_seconds = self.configured_seconds;
        true
    }

    /// Returns to Idle at the configured duration from any mode, and
    /// begins a new generation. Unlike `cancel`, this is valid from
    /// `Elapsed` too (it is the "start fresh" operation), and unlike
    /// `acknowledge`, it does not require having reached `Elapsed` first.
    pub fn reset(&mut self) -> bool {
        let changed =
            !matches!(self.mode, Mode::Idle) || self.remaining_seconds != self.configured_seconds;
        self.mode = Mode::Idle;
        self.remaining_seconds = self.configured_seconds;
        self.generation = self.generation.wrapping_add(1);
        changed
    }

    /// Consumes `step` seconds of the running session. This is the manual
    /// stand-in for a host clock (see the module docs and TIMER-F001): on
    /// a platform with `youth:time`, this would be applied by the host
    /// when a schedule elapses, not called directly. Running only; a
    /// no-op elsewhere so that stray advance commands (e.g. a queued
    /// activation arriving after a pause) cannot corrupt a frozen or idle
    /// timer.
    ///
    /// Reaching zero transitions to `Elapsed` and counts the session as
    /// completed.
    pub fn advance(&mut self, step: u64) -> bool {
        if !matches!(self.mode, Mode::Running) {
            return false;
        }
        let next = self.remaining_seconds.saturating_sub(step);
        self.remaining_seconds = next;
        if next == 0 {
            self.mode = Mode::Elapsed;
            self.completed_sessions = self.completed_sessions.saturating_add(1);
        }
        true
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
        self.remaining_seconds = self.configured_seconds;
        true
    }

    /// The `MM:SS` countdown text. Shared by `view` and `handle` in
    /// `app.rs` so the displayed string can never diverge from the model
    /// that produced it (the discipline `youth-calculator` calls
    /// CALC-F009).
    #[must_use]
    pub fn display(&self) -> String {
        format_seconds(self.remaining_seconds)
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
            Command::Advance(step) => {
                self.advance(step);
            }
            Command::Acknowledge => {
                self.acknowledge();
            }
        }
    }

    /// Checks the invariants a loaded (and therefore potentially
    /// hand-edited or corrupted) state record must satisfy. Mirrors the
    /// calculator's defensive `Model::is_valid` check at load time.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        if self.configured_seconds > MAX_SECONDS {
            return false;
        }
        if self.remaining_seconds > self.configured_seconds {
            return false;
        }
        match self.mode {
            // Idle always carries a full, unconsumed duration.
            Mode::Idle => self.remaining_seconds == self.configured_seconds,
            // Elapsed is reached only by counting all the way down.
            Mode::Elapsed => self.remaining_seconds == 0,
            // Running and Paused may hold any remaining duration in
            // between, including immediately after `start` (== configured)
            // or immediately before elapsing (== 0 is not reachable here
            // since `advance` transitions straight to Elapsed at zero).
            Mode::Running | Mode::Paused => true,
        }
    }
}

/// One user- or test-issued operation on a [`Timer`]. `app.rs` maps SDK
/// commands onto these; `Command::Advance` is the one variant a platform
/// with `youth:time` would issue from a host schedule instead of a button
/// (see the module docs and TIMER-F001).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    AddSeconds(i64),
    Start,
    Pause,
    Resume,
    Cancel,
    Reset,
    Advance(u64),
    Acknowledge,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_timer_is_idle_and_zeroed() {
        let timer = Timer::new();
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.configured_seconds(), 0);
        assert_eq!(timer.remaining_seconds(), 0);
        assert_eq!(timer.generation(), 0);
        assert_eq!(timer.completed_sessions(), 0);
    }

    #[test]
    fn add_seconds_clamps_to_zero_and_max() {
        let mut timer = Timer::new();
        assert!(!timer.add_seconds(-5));
        assert_eq!(timer.configured_seconds(), 0);

        assert!(timer.add_seconds(70));
        assert_eq!(timer.configured_seconds(), 70);
        assert_eq!(timer.remaining_seconds(), 70);

        assert!(timer.add_seconds(i64::try_from(MAX_SECONDS).unwrap() * 2));
        assert_eq!(timer.configured_seconds(), MAX_SECONDS);
    }

    #[test]
    fn add_seconds_no_op_reports_no_change() {
        let mut timer = Timer::new();
        assert!(!timer.add_seconds(0));
    }

    #[test]
    fn add_seconds_is_idle_only() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        timer.start();
        assert!(!timer.add_seconds(10));
        assert_eq!(timer.configured_seconds(), 60);
    }

    #[test]
    fn start_requires_idle_and_nonzero_configured_duration() {
        let mut timer = Timer::new();
        assert!(!timer.start(), "zero duration must not start");
        assert_eq!(timer.mode(), Mode::Idle);

        timer.add_seconds(30);
        assert!(timer.start());
        assert_eq!(timer.mode(), Mode::Running);
        assert_eq!(timer.remaining_seconds(), 30);

        assert!(!timer.start(), "already running");
    }

    #[test]
    fn start_advances_generation() {
        let mut timer = Timer::new();
        timer.add_seconds(30);
        let before = timer.generation();
        timer.start();
        assert_eq!(timer.generation(), before + 1);
    }

    #[test]
    fn pause_and_resume_are_state_gated() {
        let mut timer = Timer::new();
        assert!(!timer.pause(), "cannot pause while idle");

        timer.add_seconds(30);
        timer.start();
        assert!(timer.pause());
        assert_eq!(timer.mode(), Mode::Paused);
        assert!(!timer.pause(), "already paused");

        assert!(timer.resume());
        assert_eq!(timer.mode(), Mode::Running);
        assert!(!timer.resume(), "already running");
    }

    #[test]
    fn pause_freezes_remaining_duration() {
        let mut timer = Timer::new();
        timer.add_seconds(30);
        timer.start();
        timer.advance(10);
        timer.pause();
        let frozen = timer.remaining_seconds();
        assert!(!timer.advance(5), "advance is a no-op while paused");
        assert_eq!(timer.remaining_seconds(), frozen);
    }

    #[test]
    fn advance_decrements_remaining_while_running() {
        let mut timer = Timer::new();
        timer.add_seconds(30);
        timer.start();
        assert!(timer.advance(10));
        assert_eq!(timer.remaining_seconds(), 20);
    }

    #[test]
    fn advance_reaching_zero_elapses_and_counts_a_session() {
        let mut timer = Timer::new();
        timer.add_seconds(10);
        timer.start();
        assert!(timer.advance(10));
        assert_eq!(timer.mode(), Mode::Elapsed);
        assert_eq!(timer.remaining_seconds(), 0);
        assert_eq!(timer.completed_sessions(), 1);
    }

    #[test]
    fn advance_saturates_and_does_not_underflow() {
        let mut timer = Timer::new();
        timer.add_seconds(5);
        timer.start();
        assert!(timer.advance(999));
        assert_eq!(timer.remaining_seconds(), 0);
        assert_eq!(timer.mode(), Mode::Elapsed);
        assert_eq!(timer.completed_sessions(), 1);
    }

    #[test]
    fn advance_is_a_no_op_outside_running() {
        let mut timer = Timer::new();
        assert!(!timer.advance(1), "idle");

        timer.add_seconds(10);
        timer.start();
        timer.advance(10);
        assert_eq!(timer.mode(), Mode::Elapsed);
        assert!(!timer.advance(1), "elapsed");
        assert_eq!(timer.remaining_seconds(), 0);
    }

    #[test]
    fn acknowledge_is_elapsed_only_and_restores_configured_duration() {
        let mut timer = Timer::new();
        assert!(!timer.acknowledge(), "cannot acknowledge while idle");

        timer.add_seconds(10);
        timer.start();
        timer.advance(10);
        assert!(timer.acknowledge());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.remaining_seconds(), 10);
        assert!(!timer.acknowledge(), "already acknowledged");
    }

    #[test]
    fn cancel_returns_running_or_paused_to_idle() {
        let mut timer = Timer::new();
        assert!(!timer.cancel(), "cannot cancel while idle");

        timer.add_seconds(30);
        timer.start();
        timer.advance(20);
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.remaining_seconds(), 30);

        timer.start();
        timer.pause();
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
    }

    #[test]
    fn cancel_does_not_change_completed_sessions() {
        let mut timer = Timer::new();
        timer.add_seconds(30);
        timer.start();
        timer.cancel();
        assert_eq!(timer.completed_sessions(), 0);
    }

    #[test]
    fn reset_returns_to_idle_from_any_mode_and_advances_generation() {
        for setup in ["idle", "running", "paused", "elapsed"] {
            let mut timer = Timer::new();
            timer.add_seconds(10);
            match setup {
                "running" => {
                    timer.start();
                }
                "paused" => {
                    timer.start();
                    timer.pause();
                }
                "elapsed" => {
                    timer.start();
                    timer.advance(10);
                }
                _ => {}
            }
            let before = timer.generation();
            timer.reset();
            assert_eq!(timer.mode(), Mode::Idle, "setup: {setup}");
            assert_eq!(timer.remaining_seconds(), timer.configured_seconds());
            assert_eq!(timer.generation(), before + 1, "setup: {setup}");
        }
    }

    #[test]
    fn display_formats_as_zero_padded_minutes_and_seconds() {
        let mut timer = Timer::new();
        assert_eq!(timer.display(), "00:00");

        timer.add_seconds(9);
        assert_eq!(timer.display(), "00:09");

        timer.add_seconds(-9);
        timer.add_seconds(i64::try_from(MAX_SECONDS).unwrap());
        assert_eq!(timer.display(), "99:59");
    }

    #[test]
    fn display_reflects_remaining_not_configured_once_running() {
        let mut timer = Timer::new();
        timer.add_seconds(125); // 02:05
        timer.start();
        timer.advance(65); // -1:05 -> 01:00
        assert_eq!(timer.display(), "01:00");
    }

    #[test]
    fn apply_dispatches_every_command_variant() {
        let mut timer = Timer::new();
        timer.apply(Command::AddSeconds(20));
        assert_eq!(timer.configured_seconds(), 20);
        timer.apply(Command::Start);
        assert_eq!(timer.mode(), Mode::Running);
        timer.apply(Command::Pause);
        assert_eq!(timer.mode(), Mode::Paused);
        timer.apply(Command::Resume);
        assert_eq!(timer.mode(), Mode::Running);
        timer.apply(Command::Advance(20));
        assert_eq!(timer.mode(), Mode::Elapsed);
        timer.apply(Command::Acknowledge);
        assert_eq!(timer.mode(), Mode::Idle);
        timer.apply(Command::Start);
        timer.apply(Command::Cancel);
        assert_eq!(timer.mode(), Mode::Idle);
        let before_generation = timer.generation();
        timer.apply(Command::Reset);
        assert_eq!(timer.generation(), before_generation + 1);
    }

    #[test]
    fn every_reachable_state_is_valid() {
        let mut timer = Timer::new();
        assert!(timer.is_valid(), "fresh");
        timer.add_seconds(15);
        assert!(timer.is_valid(), "configured");
        timer.start();
        assert!(timer.is_valid(), "just started");
        timer.advance(5);
        assert!(timer.is_valid(), "running, partially consumed");
        timer.pause();
        assert!(timer.is_valid(), "paused");
        timer.resume();
        timer.advance(10);
        assert!(timer.is_valid(), "elapsed");
        assert_eq!(timer.mode(), Mode::Elapsed);
        timer.acknowledge();
        assert!(timer.is_valid(), "acknowledged");
    }

    #[test]
    fn is_valid_rejects_out_of_range_and_inconsistent_records() {
        let mut over_max = Timer::new();
        over_max.configured_seconds = MAX_SECONDS + 1;
        assert!(!over_max.is_valid());

        let mut remaining_exceeds_configured = Timer::new();
        remaining_exceeds_configured.configured_seconds = 10;
        remaining_exceeds_configured.remaining_seconds = 11;
        assert!(!remaining_exceeds_configured.is_valid());

        let mut idle_with_stale_remaining = Timer::new();
        idle_with_stale_remaining.configured_seconds = 30;
        idle_with_stale_remaining.remaining_seconds = 10;
        idle_with_stale_remaining.mode = Mode::Idle;
        assert!(!idle_with_stale_remaining.is_valid());

        let mut elapsed_with_nonzero_remaining = Timer::new();
        elapsed_with_nonzero_remaining.configured_seconds = 30;
        elapsed_with_nonzero_remaining.remaining_seconds = 5;
        elapsed_with_nonzero_remaining.mode = Mode::Elapsed;
        assert!(!elapsed_with_nonzero_remaining.is_valid());
    }

    #[test]
    fn mode_name_round_trips_through_parse_mode() {
        for mode in [Mode::Idle, Mode::Running, Mode::Paused, Mode::Elapsed] {
            assert_eq!(parse_mode(mode_name(mode)), Some(mode));
        }
        assert_eq!(parse_mode("not-a-mode"), None);
    }
}
