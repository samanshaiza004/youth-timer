//! The SDK-facing half of Youth Timer.
//!
//! This module owns exactly the translation between the SDK's semantic
//! tree/event/state surface and the pure model in `crate::model`. It
//! contains no timer *domain* logic of its own — every decision about
//! what a command means for the model lives in `crate::model`, so `view`
//! and `handle` can never disagree about what state means (mirrors the
//! `youth-calculator` discipline recorded as CALC-F009).
//!
//! # Gate C-2: host schedule operation first, model transition second
//!
//! Every command that touches a schedule follows one order, inside one
//! turn: call the host, wait for it to succeed, *then* transition the
//! model, *then* persist. The model must never transition before the host
//! call succeeds — an early transition followed by a failed host call
//! would leave staged application state describing a schedule mutation
//! that never actually happened, and Youth's own turn machinery would
//! then have to choose between committing a lie or rolling back a host
//! call it cannot undo. Doing the host call first means a `?` on its
//! `Result` is the whole safeguard: if it fails, `handle` returns before
//! the model or storage are touched, and the runtime's existing
//! transactional turn leaves everything exactly as it was.

use std::time::Duration;

use youth_sdk::prelude::*;
use youth_sdk::{Schedule, StateReader, StateWriter};

use crate::model::{
    Command, LegacyRecord, MAX_SECONDS, Mode, ScheduleHandle, Timer as Model, migrate_legacy,
    mode_name, parse_mode,
};

pub(crate) struct Timer;

impl Application for Timer {
    fn view(context: &ViewContext) -> Result<Tree> {
        let loaded = load(context.state())?;
        Ok(present(&loaded.model, loaded.recovered))
    }

    fn handle(context: &mut EventContext, events: &Events) -> Result<Update> {
        if let Some(&(schedule_id, generation, _reason)) =
            events.elapsed().collect::<Vec<_>>().first()
        {
            return handle_elapsed(
                context,
                ScheduleHandle {
                    id: schedule_id,
                    generation,
                },
            );
        }

        let Some(command) = command_from(events) else {
            return Ok(Update::unchanged());
        };
        let loaded = load(context.state())?;
        let mut model = loaded.model;
        let before = model;
        let dismissing = matches!(command, Command::DismissNotice);

        apply_command(context, &mut model, command, loaded.wire_schedule)?;

        let changed = model != before;
        let recovered = loaded.recovered && !dismissing;

        // Any successfully handled event completes a pending durable
        // rewrite, not only one that changes the domain model: a valid
        // no-op command must not leave migration outstanding. These state
        // writes still commit transactionally even when the semantic tree
        // does not change.
        if changed || loaded.rewrite_pending || loaded.recovered != recovered {
            save(context.state(), &model, recovered)?;
            delete_legacy(context.state())?;
        }
        if !changed && loaded.recovered == recovered {
            return Ok(Update::unchanged());
        }
        Ok(present_update(&model, recovered))
    }
}

/// Performs the host call a command needs (if any), then transitions the
/// model — strictly in that order, per this module's own documentation
/// above. Guard checks are read from the model's own `can_*` queries
/// rather than duplicated ad hoc, so app.rs and the model can never
/// disagree about when a command is valid.
fn apply_command(
    context: &mut EventContext,
    model: &mut Model,
    command: Command,
    current_schedule: Option<Schedule>,
) -> Result<()> {
    match command {
        Command::AddSeconds(delta) => {
            model.add_seconds(delta);
        }
        Command::Start => {
            if model.can_start() {
                let duration = Duration::from_secs(model.configured_seconds());
                let schedule = context.time().schedule_after(
                    duration,
                    ScheduleOptions::new()
                        .notification(Notification::new("Youth Timer", "Your timer has elapsed.")),
                )?;
                model.start(to_handle(schedule));
                context.state().set_schedule(SCHEDULE_KEY, schedule)?;
            }
        }
        Command::Pause => {
            if model.can_pause()
                && let Some(current) = current_schedule
            {
                let updated = context.time().pause(current)?;
                model.pause(to_handle(updated));
                context.state().set_schedule(SCHEDULE_KEY, updated)?;
            }
        }
        Command::Resume => {
            if model.can_resume()
                && let Some(current) = current_schedule
            {
                let updated = context.time().resume(current)?;
                model.resume(to_handle(updated));
                context.state().set_schedule(SCHEDULE_KEY, updated)?;
            }
        }
        Command::Cancel => {
            if matches!(model.mode(), Mode::Running | Mode::Paused) {
                if let Some(current) = current_schedule {
                    context.time().cancel(current)?;
                }
                model.cancel();
                context.state().delete(SCHEDULE_KEY)?;
            }
        }
        Command::Reset => {
            if let Some(current) = current_schedule {
                context.time().cancel(current)?;
            }
            if model.reset() {
                context.state().delete(SCHEDULE_KEY)?;
            }
        }
        Command::Acknowledge => {
            model.acknowledge();
        }
        Command::DismissNotice => {}
    }
    Ok(())
}

/// Reacts to an autonomous `ScheduleElapsed` delivery. This is not a
/// button command — it arrives as an event the host originates on its
/// own, so there is no `command_from` entry for it.
///
/// A mismatched handle returns an application error, never
/// `Update::unchanged()`: silently accepting a delivery this application
/// does not recognize would let the host's delivery acknowledgement
/// commit even though the application never actually processed it. The
/// host already guards against stale generations before ever invoking
/// the guest (TIMER-F011), so reaching this branch at all should be
/// unreachable under a correct host — the check exists anyway, as
/// defense in depth rather than trust.
fn handle_elapsed(context: &mut EventContext, delivered: ScheduleHandle) -> Result<Update> {
    let loaded = load(context.state())?;
    let mut model = loaded.model;
    if !model.on_elapsed(delivered) {
        return Err(Error::invalid_state()
            .with_message("received ScheduleElapsed for a schedule this timer is not waiting on"));
    }
    save(context.state(), &model, loaded.recovered)?;
    delete_legacy(context.state())?;
    context.state().delete(SCHEDULE_KEY)?;
    Ok(present_update(&model, loaded.recovered))
}

fn to_handle(schedule: Schedule) -> ScheduleHandle {
    ScheduleHandle {
        id: schedule.id(),
        generation: schedule.generation(),
    }
}

/// Builds the full semantic tree from a model. Shared by `view` (as the
/// initial/resync tree) and, via [`present_update`], by every
/// state-changing branch of `handle` — one presentation function is the
/// only way `view` and `handle` can never disagree about what a mode
/// implies, matching this app's own `ButtonEnabled::compute` discipline
/// (TIMER-F005) and the calculator's CALC-F009.
fn present(model: &Model, recovered: bool) -> Tree {
    let enabled = ButtonEnabled::compute(model);
    Tree::root(Column::new([
        Text::new(node!("countdown"), model.display()).align(TextAlign::Center),
        Text::new(node!("notice"), notice_line(recovered)),
        Text::new(node!("mode"), mode_line(model)),
        Text::new(node!("sessions"), sessions_line(model)),
        Row::new([
            Button::command(command!("add-1m"), "+1:00")
                .shortcut(Shortcut::Character('1'))
                .enabled(enabled.add_1m),
            Button::command(command!("add-10s"), "+0:10")
                .shortcut(Shortcut::Character('2'))
                .enabled(enabled.add_10s),
            Button::command(command!("sub-10s"), "-0:10")
                .shortcut(Shortcut::Character('3'))
                .enabled(enabled.sub_10s),
        ]),
        Row::new([
            Button::command(command!("start"), "Start")
                .shortcut(Shortcut::Enter)
                .enabled(enabled.start),
            Button::command(command!("pause"), "Pause")
                .shortcut(Shortcut::Character('p'))
                .enabled(enabled.pause),
            Button::command(command!("resume"), "Resume")
                .shortcut(Shortcut::Character('u'))
                .enabled(enabled.resume),
            Button::command(command!("cancel"), "Cancel")
                .shortcut(Shortcut::Escape)
                .enabled(enabled.cancel),
            Button::command(command!("reset"), "Reset")
                .shortcut(Shortcut::Backspace)
                .enabled(enabled.reset),
        ]),
        Row::new([
            Button::command(command!("ack"), "Acknowledge")
                .shortcut(Shortcut::Character('o'))
                .enabled(enabled.ack),
            Button::command(command!("dismiss"), "Dismiss notice").enabled(recovered),
        ]),
    ]))
}

/// Patches the retained tree to match [`present`]'s output exactly.
/// `handle` never re-runs `view` wholesale (only `resync` does), so every
/// node `present` can set must be reissued here on every state-changing
/// turn — including every button's `enabled` state — or a node keeps
/// whatever value the previous turn last set (TIMER-F005).
fn present_update(model: &Model, recovered: bool) -> Update {
    let enabled = ButtonEnabled::compute(model);
    Update::new()
        .set_text(node!("countdown"), model.display())
        .set_text(node!("mode"), mode_line(model))
        .set_text(node!("sessions"), sessions_line(model))
        .set_text(node!("notice"), notice_line(recovered))
        .set_enabled(node!("add-1m"), enabled.add_1m)
        .set_enabled(node!("add-10s"), enabled.add_10s)
        .set_enabled(node!("sub-10s"), enabled.sub_10s)
        .set_enabled(node!("start"), enabled.start)
        .set_enabled(node!("pause"), enabled.pause)
        .set_enabled(node!("resume"), enabled.resume)
        .set_enabled(node!("cancel"), enabled.cancel)
        .set_enabled(node!("reset"), enabled.reset)
        .set_enabled(node!("ack"), enabled.ack)
        .set_enabled(node!("dismiss"), recovered)
}

/// Every command button's `enabled` state, computed once from the model
/// and shared by [`present`] (to build the tree) and [`present_update`]
/// (to patch it), so the two can never disagree about which controls a
/// given mode permits.
struct ButtonEnabled {
    add_1m: bool,
    add_10s: bool,
    sub_10s: bool,
    start: bool,
    pause: bool,
    resume: bool,
    cancel: bool,
    reset: bool,
    ack: bool,
}

impl ButtonEnabled {
    fn compute(model: &Model) -> Self {
        let configuring = matches!(model.mode(), Mode::Idle);
        let running = matches!(model.mode(), Mode::Running);
        let paused = matches!(model.mode(), Mode::Paused);
        let elapsed = matches!(model.mode(), Mode::Elapsed);
        let has_duration = model.configured_seconds() > 0;
        Self {
            add_1m: configuring,
            add_10s: configuring,
            sub_10s: configuring,
            start: model.can_start(),
            pause: running,
            resume: paused,
            cancel: running || paused,
            reset: !configuring || has_duration,
            ack: elapsed,
        }
    }
}

fn mode_line(model: &Model) -> String {
    format!("Mode: {}", mode_name(model.mode()))
}

fn sessions_line(model: &Model) -> String {
    format!("Sessions: {}", model.completed_sessions())
}

/// Maps every declared command key to its model [`Command`]. One entry per
/// button declared in [`present`]; kept in the same order as the tree so
/// the two stay easy to cross-check by eye, matching the calculator's
/// `command()` table. `ScheduleElapsed` is deliberately absent — it is
/// not a button command, and is handled separately in
/// [`Timer::handle`] before this table is even consulted.
fn command_from(events: &Events) -> Option<Command> {
    let commands = [
        (command!("add-1m"), Command::AddSeconds(60)),
        (command!("add-10s"), Command::AddSeconds(10)),
        (command!("sub-10s"), Command::AddSeconds(-10)),
        (command!("start"), Command::Start),
        (command!("pause"), Command::Pause),
        (command!("resume"), Command::Resume),
        (command!("cancel"), Command::Cancel),
        (command!("reset"), Command::Reset),
        (command!("ack"), Command::Acknowledge),
        (command!("dismiss"), Command::DismissNotice),
    ];
    commands
        .into_iter()
        .find_map(|(key, command)| events.commanded(key).then_some(command))
}

/// The application's own durable schema version, independent of Youth's
/// `youth:state` schema. A record with no version key is legacy v1, which
/// Gate A wrote.
const SCHEMA_VERSION: i64 = 2;

/// Keys Gate A persisted that v2 does not. `remaining-seconds` was only
/// ever moved by manual `Advance` clicks and so never tracked real time;
/// `generation` was guest-invented and is never reused as a host schedule
/// identity (TIMER-F008) — the real identity lives at [`SCHEDULE_KEY`]
/// instead, typed as an SDK `Schedule` rather than a plain integer. Both
/// legacy keys are deleted on migration rather than translated.
const LEGACY_KEYS: [&str; 2] = ["remaining-seconds", "generation"];

/// The durable key holding the active host schedule, when one exists.
/// Stored via [`StateWriter::set_schedule`]/[`StateReader::schedule`]
/// rather than as a plain integer pair, so the host-issued value round
/// -trips as a checked `Schedule` rather than two independent numbers an
/// application could accidentally desynchronize.
const SCHEDULE_KEY: &str = "active-schedule";

/// A durable record normalized into the current model, plus what the
/// caller still owes storage.
pub(crate) struct Loaded {
    pub(crate) model: Model,
    /// A legacy record was read and has not yet been rewritten as v2.
    /// `view` cannot discharge this — its state handle is read-only — so
    /// the next handled event does (TIMER-F012).
    pub(crate) rewrite_pending: bool,
    /// A legacy `running` or `paused` session was reset by migration and
    /// the user has not dismissed the notice.
    pub(crate) recovered: bool,
    /// The real SDK `Schedule` this model's `active_schedule` handle
    /// mirrors, when one exists. `Model` cannot hold this directly — it
    /// has no `youth_sdk` dependency — so `handle` threads it separately
    /// wherever a host call needs a genuine `Schedule` value rather than
    /// the model's plain `(id, generation)` mirror. Unused by `view`.
    pub(crate) wire_schedule: Option<Schedule>,
}

/// Reads durable state into a validated [`Model`], accepting both the v2
/// shape and Gate A's legacy shape and normalizing them into one model.
///
/// A never-mounted app has no `mode` key and returns a fresh timer. Any
/// other missing or out-of-range value is a hard error rather than a
/// silent default, so a corrupted or partially-written record cannot be
/// misread as an ordinary fresh start.
///
/// Migration policy for legacy records is deliberately conservative: no
/// deadline and no remaining duration is ever invented, because none ever
/// existed. A legacy `running` or `paused` session becomes idle at its
/// configured duration and raises a recovery notice; `elapsed` keeps its
/// meaning; `completed-sessions` survives every path.
fn load(state: impl ReadState) -> Result<Loaded> {
    let Some(mode_value) = state.read_text("mode")? else {
        return Ok(Loaded {
            model: Model::default(),
            rewrite_pending: false,
            recovered: false,
            wire_schedule: None,
        });
    };
    let stored_mode = parse_mode(&mode_value)
        .ok_or_else(|| Error::invalid_state().with_message("unknown timer mode"))?;
    let configured_seconds = required_seconds(state, "configured-seconds")?;
    let completed_sessions = required_u64(state, "completed-sessions")?;
    let version = state.read_integer("model-schema-version")?;
    let wire_schedule = state.read_schedule(SCHEDULE_KEY)?;

    match version {
        Some(SCHEMA_VERSION) => {
            // A v2 record must not carry legacy keys; a mixture means
            // something wrote an incoherent record, so fail closed rather
            // than guess which half is authoritative.
            for key in LEGACY_KEYS {
                if state.read_integer(key)?.is_some() {
                    return Err(
                        Error::invalid_state().with_message("v2 state still carries a legacy key")
                    );
                }
            }
            let model = Model::from_parts(
                stored_mode,
                configured_seconds,
                completed_sessions,
                wire_schedule.map(to_handle),
            );
            require_valid(model)?;
            Ok(Loaded {
                model,
                rewrite_pending: false,
                recovered: state.read_boolean("recovery-notice")?.unwrap_or(false),
                wire_schedule,
            })
        }
        Some(_) => Err(Error::invalid_state().with_message("unsupported timer schema version")),
        None => {
            // Legacy v1. Both legacy keys must be present; a partial
            // record fails closed. A legacy record never has a schedule
            // key at all -- Gate A never created one -- so wire_schedule
            // being present here would itself be a corrupted mixture.
            for key in LEGACY_KEYS {
                if state.read_integer(key)?.is_none() {
                    return Err(Error::invalid_state()
                        .with_message("legacy state is missing a required key"));
                }
            }
            if wire_schedule.is_some() {
                return Err(Error::invalid_state()
                    .with_message("legacy state unexpectedly carries a host schedule"));
            }
            // The classification policy itself lives in `crate::model`
            // so it is host-testable; this function only does state I/O.
            let migrated = migrate_legacy(LegacyRecord {
                mode: stored_mode,
                configured_seconds,
                completed_sessions,
            });
            require_valid(migrated.model)?;
            Ok(Loaded {
                model: migrated.model,
                rewrite_pending: true,
                recovered: migrated.recovered,
                wire_schedule: None,
            })
        }
    }
}

fn require_valid(model: Model) -> Result<()> {
    if model.is_valid() {
        return Ok(());
    }
    Err(Error::invalid_state().with_message("timer state is invalid"))
}

/// Writes the complete v2 representation, excluding the schedule identity
/// (callers that armed, paused, resumed, or retired a schedule write
/// [`SCHEDULE_KEY`] themselves, at the point they have the real
/// `Schedule` value in hand). Only reachable from `handle`, because
/// `view`'s state handle is read-only (TIMER-F012).
fn save(state: StateWriter, model: &Model, recovered: bool) -> Result<()> {
    state.set_text("mode", mode_name(model.mode()))?;
    state.set_integer("configured-seconds", to_i64(model.configured_seconds())?)?;
    state.set_integer("completed-sessions", to_i64(model.completed_sessions())?)?;
    state.set_boolean("recovery-notice", recovered)?;
    state.set_integer("model-schema-version", SCHEMA_VERSION)?;
    Ok(())
}

/// Removes every key the legacy shape carried, so a migrated record can
/// never be misread as legacy again. Deleting an absent key is a no-op,
/// which is what makes repeated migration harmless.
fn delete_legacy(state: StateWriter) -> Result<()> {
    for key in LEGACY_KEYS {
        state.delete(key)?;
    }
    Ok(())
}

fn notice_line(recovered: bool) -> String {
    if recovered {
        String::from("Recovered from an earlier version: the running timer was reset.")
    } else {
        String::new()
    }
}

fn required_seconds(state: impl ReadState, key: &str) -> Result<u64> {
    let value = required_u64(state, key)?;
    if value > MAX_SECONDS {
        return Err(Error::invalid_state().with_message(format!("{key} exceeds the maximum")));
    }
    Ok(value)
}

fn required_u64(state: impl ReadState, key: &str) -> Result<u64> {
    let value = state
        .read_integer(key)?
        .ok_or_else(|| Error::invalid_state().with_message(format!("missing state key {key}")))?;
    u64::try_from(value)
        .map_err(|_| Error::invalid_state().with_message(format!("negative value for {key}")))
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::invalid_state())
}

/// A read-only view over durable state, implemented for both
/// [`StateReader`] (used by `view`) and [`StateWriter`] (used by
/// `handle`, which also needs to read before it writes). Mirrors the
/// calculator's `ReadState` trait so `load` can be called from either
/// context.
trait ReadState: Copy {
    fn read_text(self, key: &str) -> Result<Option<String>>;
    fn read_integer(self, key: &str) -> Result<Option<i64>>;
    fn read_boolean(self, key: &str) -> Result<Option<bool>>;
    fn read_schedule(self, key: &str) -> Result<Option<Schedule>>;
}

impl ReadState for StateReader {
    fn read_text(self, key: &str) -> Result<Option<String>> {
        self.text(key)
    }

    fn read_integer(self, key: &str) -> Result<Option<i64>> {
        self.integer(key)
    }

    fn read_boolean(self, key: &str) -> Result<Option<bool>> {
        self.boolean(key)
    }

    fn read_schedule(self, key: &str) -> Result<Option<Schedule>> {
        self.schedule(key)
    }
}

impl ReadState for StateWriter {
    fn read_text(self, key: &str) -> Result<Option<String>> {
        self.text(key)
    }

    fn read_integer(self, key: &str) -> Result<Option<i64>> {
        self.integer(key)
    }

    fn read_boolean(self, key: &str) -> Result<Option<bool>> {
        self.boolean(key)
    }

    fn read_schedule(self, key: &str) -> Result<Option<Schedule>> {
        self.schedule(key)
    }
}
