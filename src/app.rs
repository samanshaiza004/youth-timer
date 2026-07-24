//! The SDK-facing half of Youth Timer.
//!
//! This module owns exactly the translation between the SDK's semantic
//! tree/event/state surface and the pure model in `crate::model`. It
//! contains no timer logic of its own — every decision about what a
//! command does lives in `Model::apply`, so `view` and `handle` can never
//! disagree about what state means (mirrors the `youth-calculator`
//! discipline recorded as CALC-F009).

use youth_sdk::prelude::*;
use youth_sdk::{StateReader, StateWriter};

use crate::model::{
    ADVANCE_LARGE_SECONDS, ADVANCE_SMALL_SECONDS, Command, MAX_SECONDS, Mode, Timer as Model,
    mode_name, parse_mode,
};

pub(crate) struct Timer;

impl Application for Timer {
    fn view(context: &ViewContext) -> Result<Tree> {
        let model = load(context.state())?;
        let enabled = ButtonEnabled::compute(&model);

        Ok(Tree::root(Column::new([
            Text::new(node!("countdown"), model.display()).align(TextAlign::Center),
            Text::new(node!("mode"), mode_line(&model)),
            Text::new(node!("sessions"), sessions_line(&model)),
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
            // These two buttons are the manual stand-in for the host
            // clock a `youth:time` platform would own: this is the one
            // operation a scheduling host would drive itself instead of
            // routing through a button. See `crate::model`'s module docs
            // and FINDINGS.md TIMER-F001/F002.
            Row::new([
                Button::command(command!("advance-1s"), "Advance 1s")
                    .shortcut(Shortcut::Character('a'))
                    .enabled(enabled.advance_1s),
                Button::command(command!("advance-10s"), "Advance 10s")
                    .shortcut(Shortcut::Character('k'))
                    .enabled(enabled.advance_10s),
            ]),
            Row::new([Button::command(command!("ack"), "Acknowledge")
                .shortcut(Shortcut::Character('o'))
                .enabled(enabled.ack)]),
        ])))
    }

    fn handle(context: &mut EventContext, events: &Events) -> Result<Update> {
        let Some(command) = command_from(events) else {
            return Ok(Update::unchanged());
        };
        let mut model = load(context.state())?;
        let before = model;
        model.apply(command);
        if model == before {
            return Ok(Update::unchanged());
        }
        save(context.state(), &model)?;

        // `handle` patches the host's retained tree incrementally --
        // unlike `view`, it is never re-run wholesale (only `resync`
        // re-invokes `view`). Any button whose `enabled` state depends on
        // the model must therefore have that state explicitly reissued
        // here on every change, or it keeps whatever value `view` (or the
        // previous `handle`) last set. This is TIMER-F005 in
        // ../FINDINGS.md: there is no declarative recomputation across a
        // turn, and the calculator's set-text-only pattern does not by
        // itself surface this because its buttons are always enabled.
        let enabled = ButtonEnabled::compute(&model);
        Ok(Update::new()
            .set_text(node!("countdown"), model.display())
            .set_text(node!("mode"), mode_line(&model))
            .set_text(node!("sessions"), sessions_line(&model))
            .set_enabled(node!("add-1m"), enabled.add_1m)
            .set_enabled(node!("add-10s"), enabled.add_10s)
            .set_enabled(node!("sub-10s"), enabled.sub_10s)
            .set_enabled(node!("start"), enabled.start)
            .set_enabled(node!("pause"), enabled.pause)
            .set_enabled(node!("resume"), enabled.resume)
            .set_enabled(node!("cancel"), enabled.cancel)
            .set_enabled(node!("reset"), enabled.reset)
            .set_enabled(node!("advance-1s"), enabled.advance_1s)
            .set_enabled(node!("advance-10s"), enabled.advance_10s)
            .set_enabled(node!("ack"), enabled.ack))
    }
}

/// Every command button's `enabled` state, computed once from the model
/// and shared by `view` (to build the tree) and `handle` (to patch it),
/// so the two can never disagree about which controls a given mode
/// permits.
struct ButtonEnabled {
    add_1m: bool,
    add_10s: bool,
    sub_10s: bool,
    start: bool,
    pause: bool,
    resume: bool,
    cancel: bool,
    reset: bool,
    advance_1s: bool,
    advance_10s: bool,
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
            start: configuring && has_duration,
            pause: running,
            resume: paused,
            cancel: running || paused,
            reset: !configuring || has_duration,
            advance_1s: running,
            advance_10s: running,
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
/// button declared in `view`; kept in the same order as the tree so the
/// two stay easy to cross-check by eye, matching the calculator's
/// `command()` table.
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
        (
            command!("advance-1s"),
            Command::Advance(ADVANCE_SMALL_SECONDS),
        ),
        (
            command!("advance-10s"),
            Command::Advance(ADVANCE_LARGE_SECONDS),
        ),
        (command!("ack"), Command::Acknowledge),
    ];
    commands
        .into_iter()
        .find_map(|(key, command)| events.commanded(key).then_some(command))
}

/// Reads durable state into a validated [`Model`]. A never-mounted app has
/// no `mode` key yet, and returns a fresh timer; any other missing or
/// out-of-range value is a hard error rather than a silent default, so a
/// corrupted state file cannot be misread as an ordinary fresh start.
fn load(state: impl ReadState) -> Result<Model> {
    let Some(mode_value) = state.read_text("mode")? else {
        return Ok(Model::default());
    };
    let mode = parse_mode(&mode_value)
        .ok_or_else(|| Error::invalid_state().with_message("unknown timer mode"))?;
    let configured_seconds = required_seconds(state, "configured-seconds")?;
    let remaining_seconds = required_seconds(state, "remaining-seconds")?;
    let generation = required_u64(state, "generation")?;
    let completed_sessions = required_u64(state, "completed-sessions")?;

    let model = Model::from_parts(
        mode,
        configured_seconds,
        remaining_seconds,
        generation,
        completed_sessions,
    );
    if !model.is_valid() {
        return Err(Error::invalid_state().with_message("timer state is invalid"));
    }
    Ok(model)
}

fn save(state: StateWriter, model: &Model) -> Result<()> {
    state.set_text("mode", mode_name(model.mode()))?;
    state.set_integer("configured-seconds", to_i64(model.configured_seconds())?)?;
    state.set_integer("remaining-seconds", to_i64(model.remaining_seconds())?)?;
    state.set_integer("generation", to_i64(model.generation())?)?;
    state.set_integer("completed-sessions", to_i64(model.completed_sessions())?)?;
    Ok(())
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
}

impl ReadState for StateReader {
    fn read_text(self, key: &str) -> Result<Option<String>> {
        self.text(key)
    }

    fn read_integer(self, key: &str) -> Result<Option<i64>> {
        self.integer(key)
    }
}

impl ReadState for StateWriter {
    fn read_text(self, key: &str) -> Result<Option<String>> {
        self.text(key)
    }

    fn read_integer(self, key: &str) -> Result<Option<i64>> {
        self.integer(key)
    }
}
