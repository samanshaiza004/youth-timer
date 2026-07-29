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
//!
//! # Gate C-2: schedule identity
//!
//! `Running` and `Paused` now carry a [`ScheduleHandle`] — the host-issued
//! `(id, generation)` pair, and nothing else. This module has no
//! dependency on `youth_sdk`, so it cannot use the SDK's own `Schedule`
//! type directly; `ScheduleHandle` is a local mirror of exactly its two
//! fields, and `app.rs` converts between them at the boundary. No
//! remaining-time value is ever stored here or anywhere durable
//! (TIMER-F003): the host owns it entirely.
//!
//! Every operation that changes when a schedule would fire — starting,
//! pausing, resuming — must first ask the host and receive back a new
//! handle before this model transitions (TIMER-F008 and the D2 pause/
//! resume correction in `DEVELOPER-PREVIEW-2.md`: the host bumps the
//! generation on every such call, so the guest cannot keep using a
//! pre-call handle). Cancelling and resetting an active session ask the
//! host to retire the handle and pass no replacement.

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

/// A host-issued schedule identity: the `(id, generation)` pair, and
/// nothing else. A local mirror of `youth_sdk::Schedule`'s two accessors,
/// kept here rather than depending on the SDK, so this module stays
/// host-testable. `app.rs` is the only place that converts between the
/// two.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduleHandle {
    pub id: u64,
    pub generation: u64,
}

/// The canonical, durable application state.
///
/// Every field here is what the design's "Application model" section says
/// the guest should persist: mode, configured duration, the active
/// schedule identity when one exists, and a completed-session count.
/// There is no native timestamp and no host clock reading. Gate C-1
/// removed `remaining_seconds` and the guest-invented `generation`: the
/// former was only ever decremented by manual `Advance` clicks and so
/// never tracked real time, and the latter is replaced here by the
/// host-issued schedule generation (TIMER-F003, TIMER-F008). A paused
/// session's remainder becomes host-owned, not guest state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timer {
    mode: Mode,
    configured_seconds: u64,
    completed_sessions: u64,
    active_schedule: Option<ScheduleHandle>,
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
            active_schedule: None,
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
        completed_sessions: u64,
        active_schedule: Option<ScheduleHandle>,
    ) -> Self {
        Self {
            mode,
            configured_seconds,
            completed_sessions,
            active_schedule,
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

    #[must_use]
    pub const fn active_schedule(&self) -> Option<ScheduleHandle> {
        self.active_schedule
    }

    /// Whether [`Timer::start`] would currently succeed. `app.rs` checks
    /// this *before* arming a host schedule, so it never creates one for
    /// a command the model would then reject — the query and the
    /// mutator's own guard share this one definition, so they cannot
    /// drift apart.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(self.mode, Mode::Idle) && self.configured_seconds > 0
    }

    /// Whether [`Timer::pause`] would currently succeed.
    #[must_use]
    pub const fn can_pause(&self) -> bool {
        matches!(self.mode, Mode::Running)
    }

    /// Whether [`Timer::resume`] would currently succeed.
    #[must_use]
    pub const fn can_resume(&self) -> bool {
        matches!(self.mode, Mode::Paused)
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
    /// only if a nonzero duration has been configured, and only with a
    /// schedule handle the host has already issued (`app.rs` calls
    /// `context.time().schedule_after(..)` first; this method never talks
    /// to the host itself). Returns `false`, touching nothing, if the
    /// guard fails — `app.rs` must not have armed a host schedule it then
    /// discards.
    pub fn start(&mut self, schedule: ScheduleHandle) -> bool {
        if !self.can_start() {
            return false;
        }
        self.mode = Mode::Running;
        self.active_schedule = Some(schedule);
        true
    }

    /// Freezes the remaining duration. Running only. `schedule` is the
    /// **new** handle the host returned from pausing — pausing bumps the
    /// generation (it changes when the schedule would next fire), so the
    /// pre-pause handle is already stale and must not be kept.
    pub fn pause(&mut self, schedule: ScheduleHandle) -> bool {
        if !self.can_pause() {
            return false;
        }
        self.mode = Mode::Paused;
        self.active_schedule = Some(schedule);
        true
    }

    /// Continues counting down from the frozen remaining duration. Paused
    /// only. `schedule` is the new handle the host returned from resuming
    /// — resuming also bumps the generation, for the same reason pausing
    /// does.
    pub fn resume(&mut self, schedule: ScheduleHandle) -> bool {
        if !self.can_resume() {
            return false;
        }
        self.mode = Mode::Running;
        self.active_schedule = Some(schedule);
        true
    }

    /// Abandons the active session and returns to Idle. Running or Paused
    /// only. Takes no handle: `app.rs` reads [`Timer::active_schedule`]
    /// beforehand to tell the host which schedule to retire, and this
    /// method only clears the guest's own record of it — the host's
    /// `cancel` call needs no reply, unlike pause and resume.
    pub fn cancel(&mut self) -> bool {
        if !matches!(self.mode, Mode::Running | Mode::Paused) {
            return false;
        }
        self.mode = Mode::Idle;
        self.active_schedule = None;
        true
    }

    /// Returns to Idle from any mode. Unlike `cancel`, this is valid from
    /// `Elapsed` too (it is the "start fresh" operation), and unlike
    /// `acknowledge`, it does not require having reached `Elapsed` first.
    /// As with `cancel`, `app.rs` retires any active host schedule using
    /// the handle it read before calling this.
    pub fn reset(&mut self) -> bool {
        let changed = !matches!(self.mode, Mode::Idle) || self.active_schedule.is_some();
        self.mode = Mode::Idle;
        self.active_schedule = None;
        changed
    }

    /// Acknowledges an elapsed session and returns to Idle. Elapsed only
    /// — this is deliberately not the same operation as `reset` (see the
    /// design's "Elapsed delivery": the host would redeliver the event
    /// until an equivalent acknowledgement commits). No schedule is
    /// involved: [`Timer::on_elapsed`] already cleared it on the way in.
    pub fn acknowledge(&mut self) -> bool {
        if !matches!(self.mode, Mode::Elapsed) {
            return false;
        }
        self.mode = Mode::Idle;
        true
    }

    /// Reacts to an autonomous `ScheduleElapsed` delivery. Running only,
    /// and only if `matching` names the schedule this timer is actually
    /// waiting on — the host already guards against stale generations
    /// before ever invoking the guest (TIMER-F011), but the application
    /// still checks its own record independently rather than treating any
    /// delivery as completion by assumption. A mismatch is `false` with
    /// nothing changed, which `app.rs` treats as a hard error, not a
    /// silent no-op: it should not be reachable if the host's own guard is
    /// working.
    pub fn on_elapsed(&mut self, matching: ScheduleHandle) -> bool {
        if self.mode != Mode::Running || self.active_schedule != Some(matching) {
            return false;
        }
        self.mode = Mode::Elapsed;
        self.active_schedule = None;
        self.completed_sessions = self.completed_sessions.saturating_add(1);
        true
    }

    /// The `MM:SS` literal text for a mode that has no live host schedule
    /// to display instead. Shared by `view` and `handle` in `app.rs` so the
    /// displayed string can never diverge from the model that produced it
    /// (the discipline `youth-calculator` calls CALC-F009).
    ///
    /// Only meaningful for `Idle` (the configured duration, not yet
    /// running) and `Elapsed` (the session is over: `00:00`, not whatever
    /// duration was configured — TIMER-F004). `Running` and `Paused` are
    /// never presented from this method: `app.rs` presents those as a
    /// host-owned `Countdown` bound to the real schedule instead, since a
    /// live countdown is exactly what this pure, clock-free model cannot
    /// compute.
    #[must_use]
    pub fn display(&self) -> String {
        match self.mode {
            Mode::Idle | Mode::Running | Mode::Paused => format_seconds(self.configured_seconds),
            Mode::Elapsed => format_seconds(0),
        }
    }

    /// Checks the invariants a loaded (and therefore potentially
    /// hand-edited or corrupted) state record must satisfy. Mirrors the
    /// calculator's defensive `Model::is_valid` check at load time, and
    /// additionally enforces the schedule-identity invariant Gate C-2
    /// introduced: `Running`/`Paused` always carry a schedule the host
    /// issued, and `Idle`/`Elapsed` never do — a record with either
    /// mismatched is corrupt, not a state this application ever produces.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        if self.configured_seconds > MAX_SECONDS {
            return false;
        }
        match self.mode {
            Mode::Running | Mode::Paused => self.active_schedule.is_some(),
            Mode::Idle | Mode::Elapsed => self.active_schedule.is_none(),
        }
    }
}

/// One user-issued operation on a [`Timer`]. Gate C-1 removed
/// `Command::Advance`. Gate C-2 gives `Start`/`Pause`/`Resume`/`Cancel`/
/// `Reset` host meaning: `app.rs` no longer applies these through a single
/// pure dispatcher (see the removed `Timer::apply`) because each now
/// requires a host round trip interleaved with the model's own guard
/// check, and `Start`/`Pause`/`Resume` need the schedule handle the host
/// returns. Elapsing is no longer a command at all — it arrives as an
/// autonomous `ScheduleElapsed` delivery and is handled by
/// [`Timer::on_elapsed`].
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
        // Migration never carries a schedule identity forward: a legacy
        // guest-invented generation was never a host schedule (TIMER-F008),
        // and idle/elapsed are exactly the modes that require None.
        model: Timer::from_parts(
            mode,
            record.configured_seconds,
            record.completed_sessions,
            None,
        ),
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

        timer.start(handle(1, 1));
        assert!(
            !timer.add_seconds(10),
            "configuration is fixed once started"
        );
    }

    #[test]
    fn start_requires_idle_and_a_nonzero_duration_and_stores_the_handle() {
        let mut timer = Timer::new();
        assert!(!timer.start(handle(1, 1)), "nothing configured");

        timer.add_seconds(60);
        assert!(timer.start(handle(7, 1)));
        assert_eq!(timer.mode(), Mode::Running);
        assert_eq!(timer.active_schedule(), Some(handle(7, 1)));
        assert!(!timer.start(handle(8, 1)), "already running");
        assert_eq!(
            timer.active_schedule(),
            Some(handle(7, 1)),
            "a rejected start must not overwrite the existing handle"
        );
    }

    #[test]
    fn pause_and_resume_are_state_gated_and_replace_the_handle() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.pause(handle(1, 2)), "idle cannot pause");

        timer.start(handle(1, 1));
        assert!(timer.pause(handle(1, 2)));
        assert_eq!(timer.mode(), Mode::Paused);
        assert_eq!(
            timer.active_schedule(),
            Some(handle(1, 2)),
            "pausing bumped the generation; the guest must hold the new one"
        );
        assert!(!timer.pause(handle(1, 3)), "already paused");

        assert!(timer.resume(handle(1, 3)));
        assert_eq!(timer.mode(), Mode::Running);
        assert_eq!(timer.active_schedule(), Some(handle(1, 3)));
        assert!(!timer.resume(handle(1, 4)), "already running");
    }

    #[test]
    fn cancel_returns_to_idle_from_running_or_paused_only_and_clears_the_handle() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.cancel(), "idle has nothing to cancel");

        timer.start(handle(1, 1));
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.active_schedule(), None);
        assert_eq!(timer.configured_seconds(), 60, "configuration survives");

        timer.start(handle(2, 1));
        timer.pause(handle(2, 2));
        assert!(timer.cancel());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.active_schedule(), None);
    }

    #[test]
    fn reset_returns_to_idle_from_any_mode_and_clears_the_handle() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        timer.start(handle(1, 1));
        assert!(timer.reset());
        assert_eq!(timer.mode(), Mode::Idle);
        assert_eq!(timer.active_schedule(), None);
        assert_eq!(timer.configured_seconds(), 60);
        assert!(
            !timer.reset(),
            "already idle with no handle: nothing changed"
        );
    }

    #[test]
    fn reset_reports_a_change_when_only_the_handle_would_clear() {
        // A degenerate case `is_valid` would otherwise never let arise in
        // practice (Idle always implies no handle), included because
        // `reset`'s "changed" bookkeeping is a separate check from mode
        // equality and deserves its own coverage.
        let mut timer = Timer::from_parts(Mode::Idle, 60, 0, Some(handle(9, 1)));
        assert!(timer.reset());
        assert_eq!(timer.active_schedule(), None);
    }

    #[test]
    fn acknowledge_is_elapsed_only() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(!timer.acknowledge(), "idle has nothing to acknowledge");
        timer.start(handle(1, 1));
        assert!(!timer.acknowledge(), "running has nothing to acknowledge");
    }

    #[test]
    fn on_elapsed_requires_running_and_a_matching_handle() {
        let mut timer = Timer::new();
        timer.add_seconds(60);
        assert!(
            !timer.on_elapsed(handle(1, 1)),
            "idle has no active schedule to elapse"
        );

        timer.start(handle(1, 1));
        assert!(
            !timer.on_elapsed(handle(1, 2)),
            "the host guards against this, but the app must reject it too"
        );
        assert_eq!(
            timer.mode(),
            Mode::Running,
            "a rejected delivery must not mutate the timer"
        );

        assert!(timer.on_elapsed(handle(1, 1)));
        assert_eq!(timer.mode(), Mode::Elapsed);
        assert_eq!(timer.active_schedule(), None);
        assert_eq!(timer.completed_sessions(), 1);
    }

    #[test]
    fn on_elapsed_rejects_a_paused_schedule() {
        // Pausing removes the host deadline entirely, so a paused schedule
        // cannot become due; a delivery naming one is impossible under a
        // correct host and must still be refused defensively.
        let mut timer = Timer::new();
        timer.add_seconds(60);
        timer.start(handle(1, 1));
        timer.pause(handle(1, 2));
        assert!(!timer.on_elapsed(handle(1, 2)));
        assert_eq!(timer.mode(), Mode::Paused);
    }

    #[test]
    fn is_valid_requires_a_handle_iff_running_or_paused() {
        assert!(Timer::from_parts(Mode::Idle, 60, 0, None).is_valid());
        assert!(!Timer::from_parts(Mode::Idle, 60, 0, Some(handle(1, 1))).is_valid());
        assert!(Timer::from_parts(Mode::Elapsed, 60, 1, None).is_valid());
        assert!(!Timer::from_parts(Mode::Elapsed, 60, 1, Some(handle(1, 1))).is_valid());
        assert!(Timer::from_parts(Mode::Running, 60, 0, Some(handle(1, 1))).is_valid());
        assert!(!Timer::from_parts(Mode::Running, 60, 0, None).is_valid());
        assert!(Timer::from_parts(Mode::Paused, 60, 0, Some(handle(1, 1))).is_valid());
        assert!(!Timer::from_parts(Mode::Paused, 60, 0, None).is_valid());
    }

    #[test]
    fn completed_sessions_are_preserved_across_transitions() {
        // Sessions are application meaning, distinct from any host
        // schedule generation (TIMER-F008). Only on_elapsed increments
        // them.
        let timer = Timer::from_parts(Mode::Elapsed, 300, 7, None);
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
    fn display_is_zero_once_elapsed_regardless_of_configured_duration() {
        // The session is over; showing the configured duration again would
        // read as "still 5 minutes to go" rather than "done" (TIMER-F004).
        let timer = Timer::from_parts(Mode::Elapsed, 300, 1, None);
        assert_eq!(timer.display(), "00:00");
    }

    #[test]
    fn is_valid_rejects_an_out_of_range_configuration() {
        assert!(Timer::from_parts(Mode::Idle, MAX_SECONDS, 0, None).is_valid());
        assert!(!Timer::from_parts(Mode::Idle, MAX_SECONDS + 1, 0, None).is_valid());
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in [Mode::Idle, Mode::Running, Mode::Paused, Mode::Elapsed] {
            assert_eq!(parse_mode(mode_name(mode)), Some(mode));
        }
        assert_eq!(parse_mode("advancing"), None);
    }

    #[test]
    fn a_full_session_life_cycle_holds_the_invariant_throughout() {
        let mut timer = Timer::new();
        timer.add_seconds(300);
        assert!(timer.is_valid());
        timer.start(handle(1, 1));
        assert!(timer.is_valid());
        timer.pause(handle(1, 2));
        assert!(timer.is_valid());
        timer.resume(handle(1, 3));
        assert!(timer.is_valid());
        assert!(timer.on_elapsed(handle(1, 3)));
        assert!(timer.is_valid());
        assert!(timer.acknowledge());
        assert!(timer.is_valid());
    }

    fn handle(id: u64, generation: u64) -> ScheduleHandle {
        ScheduleHandle { id, generation }
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
        assert_eq!(out.model.active_schedule(), None);
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
        assert_eq!(
            out.model.active_schedule(),
            None,
            "a legacy guest-invented generation is never reused as a host schedule identity"
        );
        assert!(out.recovered, "the user must be told it was reset");
    }

    #[test]
    fn legacy_paused_is_reset_for_the_same_reason() {
        let out = migrate_legacy(legacy(Mode::Paused, 90, 1));
        assert_eq!(out.model.mode(), Mode::Idle);
        assert_eq!(out.model.configured_seconds(), 90);
        assert_eq!(out.model.active_schedule(), None);
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
