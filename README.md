# Youth Timer

The second Youth Utility Suite application, maintained as a standalone
repository. It targets protocol `0.0.5` and the exact SDK revision in
`Youth.lock`.

```text
youth check
youth test
youth dev
youth build --release
```

## What this app is (Gate C-4)

`Start`, `Pause`, `Resume`, `Cancel`, and `Reset` each perform a real host
schedule operation through `context.time()` — no clock, tick, or elapsed
delivery is guest-owned or guest-invented. The host issues schedule
identity (`id` + `generation`); this app stores exactly what the host
returns and never fabricates or mutates a generation itself. An autonomous
`ScheduleElapsed` delivery — the host waking this application because a
schedule became due, not a button press — is handled by `handle_elapsed`,
which requires an exact match against the stored schedule handle and
returns an application error on any mismatch rather than silently
no-opping.

The `countdown` node is host-owned temporal presentation, not text this
app formats and rewrites itself: while Running or Paused it is a real
`Countdown` bound to the active schedule, resolved and repainted by the
host with no guest turn per visible tick (proven generically at the
platform level; see `FINDINGS.md`, TIMER-F004). Pausing and resuming both
return a new schedule generation, and the same turn that stores it also
retargets the countdown to it — the display never points at a schedule
reference the host has already superseded. Idle and Elapsed present a
literal string instead (the configured duration, or `"00:00"` once
elapsed), since neither has a live schedule to show.

Recovery is observed, not just reviewed: `tests/overdue_recovery.youth-test`
starts a real schedule, uses the platform's `sleep` command for a genuine
wall-clock wait past its deadline, and `restart`s the process — the
reopened app shows `Mode: elapsed` and an incremented session count
directly, because mounting a reopened process now drains any backlog of
already-overdue deliveries before returning its first snapshot. This app's
`Start` handler has attached a notification descriptor
(`Notification::new("Youth Timer", "Your timer has elapsed.")`) since Gate
C-2; the platform now dispatches it as a real, best-effort OS notification
attempt whenever a schedule elapses, independent of whether the elapsed
delivery itself succeeds — this repo cannot itself observe a dispatched OS
notification (`.youth-test` has no hook for that), so that half of the
evidence is platform-level, not app-repo-level; see `FINDINGS.md`,
TIMER-F002/F006/F007.

`FINDINGS.md` records what remains open: this repository's own test suite
still cannot resolve a live countdown's exact displayed value mid-count, or
inject a mid-turn failure to prove rollback (`test-support` fault
injection lives in the Youth workspace, not here). Native notification
*content* review beyond "a descriptor exists and dispatch is attempted" —
actions, richer payloads, platform-specific behavior — is Gate D.

## The model

`src/model.rs` is a pure, host-agnostic domain model: mode, configured
duration, completed-session count, and an `Option<ScheduleHandle>` mirroring
the host's real schedule identity when one is active — never a guest-derived
remaining-time value. `view` and `handle` share one `present`/`present_update`
pair, itself driven by one `ButtonEnabled::compute` and one
`countdown_content` — so the countdown's content (literal or live), its text,
and every control's enabled state can never disagree between the two.
`app.rs`'s `apply_command` is the only place a command touches both the host
and the model, and it always performs the host schedule operation first,
the model transition second, and persistence last, inside one turn: if the
host call fails, the model never changes and nothing is written. It also
returns the exact `Schedule` value the host just handed back, since pausing
and resuming both bump the generation — `present_update` retargets the
countdown to that value, never to the pre-call reference.

Four modes: Idle (configuring), Running, Paused, Elapsed. Configuring is
possible only while Idle, bounded to `[00:00, 99:59]`. Starting requires a
nonzero configured duration and creates a schedule. Running and Paused
require an active schedule handle; Idle and Elapsed require its absence —
enforced by `Timer::is_valid`. Acknowledging an elapsed session is a
separate step from resetting one, matching the design's distinction between
the two.

## Controls

| Command | Shortcut | Available |
| --- | --- | --- |
| `+1:00` / `+0:10` / `-0:10` | `1` / `2` / `3` | Idle |
| Start | Enter | Idle, with a configured duration |
| Pause | `p` | Running |
| Resume | `u` | Paused |
| Cancel | Escape | Running or Paused |
| Reset | Backspace | Whenever there is something to reset |
| Acknowledge | `o` | Elapsed |

Every command is bound through `Button::command`/`Events::commanded`; Tab
and arrow-key focus, and shortcut resolution, use host-owned interaction
policy exactly as `docs/book/src/ui.md` in the Youth workspace describes.
The host does not enforce a button's `enabled` state on a direct semantic
activation (see `FINDINGS.md`, TIMER-F010) — every command independently
rejects itself when the mode does not allow it, so the app does not rely on
the UI's presentation as access control.

## Release evidence (Gate C-4)

These are the metrics this stage can actually produce; see `FINDINGS.md` for
which of the design's metrics (turn latency, boundary bytes, state-commit
latency, memory, idle CPU) still have nothing autonomous exercised from this
repo's own tests, and are therefore not published here rather than
published as misleading zeros.

| Metric | Value |
| --- | --- |
| Debug component size | 5,878,529 bytes |
| Release component size | 123,497 bytes |
| Release component SHA-256 | `94745724963608f8d24657f668a8e2b14eff9076f9ac6b1dc705641a64517d9b` |
| Raw WIT concepts exposed to the app | 0 |
| Native accessibility projection | 0% (unchanged from the platform baseline) |
| Platform, build | macOS/arm64, protocol `0.0.5`, warm dependency cache |

The release build above ran with an already-populated Cargo registry cache
(this repository had already been checked and tested), so it is not a
fresh-install timing and is not published as one.

## The vendored WIT

The vendored WIT directory is an inspectable contract snapshot, not a
binding source. Rust bindings and export plumbing come only from the exact
`youth-sdk` revision pinned in `Youth.lock`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

## Migrating from an earlier install

An application-state schema version (`model-schema-version`) was added at
Gate C-1. A durable record written by an earlier version of this app —
Gate A's manual-advance model, with its guest-owned `remaining_seconds` and
`generation` — is detected on load, never trusted as real progress, and
rewritten on the first event this version successfully handles (not only
the first one that changes the model — dismissing a recovery notice alone
completes a pending rewrite). A stored `running` or `paused` session
becomes idle at its configured duration with a recovery notice, because no
durable deadline ever existed for it and inventing one would be dishonest.
Completed-session counts survive. Once migrated, the legacy keys are
deleted and never reused.

## What's still open

Rollback-injection testing (proving a failure between schedule creation
and commit leaves no partial state) needs the Youth workspace's
`test-support` fault-injection feature exercised against a Timer-shaped
fixture there — this repo alone cannot express it. Richer notification
behavior (actions, platform-specific payload variants) is Gate D. See
`FINDINGS.md`, TIMER-F013.

