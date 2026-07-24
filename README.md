# Youth Timer

The second Youth Utility Suite application, maintained as a standalone
repository. It targets protocol `0.0.3` and the exact SDK revision in
`Youth.lock`.

```text
youth check
youth test
youth dev
youth build --release
```

## What this app is (Gate A)

This is a **Gate A** proof: it exercises the Timer domain model and command
surface with today's Youth capabilities, and nothing more. Read that
plainly — **it does not count down on its own.** Youth exposes no clock, no
tick, and no scheduled wakeup to a guest application. Every advance of the
countdown, including the two "Advance 1s" / "Advance 10s" buttons in the
window, is an explicit command standing in for a host clock that does not
exist yet.

That is the point of building it. `FINDINGS.md` records exactly what a real
timer would need — a `youth:time` capability owning clocks, sleeping,
durable scheduling, host-rendered countdown presentation, and notification —
and what, separately, turned out to need no new capability at all (the mode
machine, bounded configuration, session counting, and the full command and
shortcut surface all built cleanly on the existing SDK).

## The model

Durable state is the canonical timer model — mode, configured duration,
remaining duration, generation, and completed-session count — never a
derived display string. `view` and `handle` share one `display()` formatter
and one `ButtonEnabled` computation, so the countdown text and every
control's enabled state can never disagree between the two. `Timer::apply`
is the single place a command changes the model; `view` and `handle` never
duplicate that logic.

Four modes: Idle (configuring), Running, Paused, Elapsed. Configuring is
possible only while Idle, bounded to `[00:00, 99:59]`. Starting requires a
nonzero configured duration and begins a new generation. Acknowledging an
elapsed session is a separate step from resetting one, matching the
design's distinction between the two.

## Controls

| Command | Shortcut | Available |
| --- | --- | --- |
| `+1:00` / `+0:10` / `-0:10` | `1` / `2` / `3` | Idle |
| Start | Enter | Idle, with a configured duration |
| Pause | `p` | Running |
| Resume | `u` | Paused |
| Cancel | Escape | Running or Paused |
| Reset | Backspace | Whenever there is something to reset |
| Advance 1s / Advance 10s | `a` / `k` | Running only — the manual clock stand-in |
| Acknowledge | `o` | Elapsed |

Every command is bound through `Button::command`/`Events::commanded`; Tab
and arrow-key focus, and shortcut resolution, use host-owned interaction
policy exactly as `docs/book/src/ui.md` in the Youth workspace describes.
The host does not enforce a button's `enabled` state on a direct semantic
activation (see `FINDINGS.md`, TIMER-F010) — every command independently
rejects itself when the mode does not allow it, so the app does not rely on
the UI's presentation as access control.

## Release evidence (Gate A)

These are the metrics this stage can actually produce; see `FINDINGS.md` for
which of the design's metrics (turn latency, boundary bytes, state-commit
latency, memory, idle CPU) have nothing autonomous to measure yet, and are
therefore not published here rather than published as misleading zeros.

| Metric | Value |
| --- | --- |
| Debug component size | 5,555,544 bytes |
| Release component size | 107,696 bytes |
| Release component SHA-256 | `41cca4f7fd2b1bf205cea0291d091a5147594472492ed680f55640b1a9f1526f` |
| Raw WIT concepts exposed to the app | 0 |
| Native accessibility projection | 0% (unchanged from the platform baseline) |
| Platform, build | macOS/arm64, `youth-cli 0.0.2`, protocol `0.0.3`, warm dependency cache |

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
