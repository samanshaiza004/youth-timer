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

## What this app is (Gate C-3)

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

`FINDINGS.md` records what remains open: this repository's own test suite
can prove declaration, persistence, identity-across-restart, and that the
`countdown` node is genuinely bound to a live schedule (not a value this
app resolved or formatted itself) — see `tests/basic.youth-test` and
`tests/migration_then_start.youth-test` — but cannot itself wait on real
elapsed time, resolve a live countdown's displayed value, or fire a
virtual-clock wake, so `ScheduleElapsed` delivery and
overdue-reconciliation-across-a-closed-process are proven by review and by
Youth's own workspace tests, not by an end-to-end test from this repo.
Native notification is Gate D, not yet built.

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

## Release evidence (Gate C-3)

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

## Gate C-4 and Gate D: not yet built

Overdue reconciliation across a real closed-process interval, and proving
elapsed delivery and redelivery end to end from this repo, are Gate C-4.
Native notification on elapse is Gate D. See `FINDINGS.md`, TIMER-F002,
TIMER-F007, and TIMER-F006.

