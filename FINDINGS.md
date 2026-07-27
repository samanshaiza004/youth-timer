# Timer Findings

This is the canonical evidence record for the Timer entry in the Youth
Utility Suite. Findings describe observed application friction; they do not
automatically authorize platform features.

This app is deliberately a **Gate A** proof: it implements the Timer domain
model and command surface with today's Youth capabilities and records what
could not be built. It does not count down on its own. Every tick is an
explicit command (`Advance 1s` / `Advance 10s`, or the equivalent test
activation) standing in for a host clock that does not exist yet. See
`README.md` for what this means for anyone trying the app.

## The main result

TIMER-F009 means the next round of protocol work can stay narrow:

```text
Needed:
  - durable scheduling
  - host-owned temporal presentation
  - elapsed delivery
  - overdue reconciliation
  - notifications

Not needed:
  - a redesigned application lifecycle
  - a general guest clock
  - raw timer ticks
  - a new command system
  - a different persistence API
```

## Gate B has landed in the platform; this app has not migrated

Youth implemented `youth:time@0.0.1` and durable scheduling in Gate B
(Developer Preview 2). That changes the *platform* answer to several
findings below, but **this application still runs the Gate A design** —
every tick is still an explicit `Advance` command. Findings therefore
keep their Gate A status except where the finding was purely a question
about host architecture:

- **TIMER-F011 is closed.** It asked who owns a schedule when no guest
  instance is mounted, which was answerable without the app changing.
- **TIMER-F001, F002, F003, F007, F008** now have platform answers
  (declarative scheduling, host-initiated delivery, host-owned schedule
  state, overdue reconciliation, host-issued generations), but remain
  open *as application evidence* until the Timer adopts `context.time()`
  and deletes its manual-advance path (TIMER-F003's deletion list).
- **TIMER-F004 and F006** are untouched: host-owned countdown
  presentation and notification are Gate C and Gate D.

## Open findings

| ID | Summary | Next decision |
| --- | --- | --- |
| TIMER-F001 | No host-owned temporal capability; a countdown cannot be declared, delivered, or presented (corrected 2026-07-25: guests *can* read monotonic time via `Instant::now()`) | Requires declarative scheduling + host-owned presentation; also decide whether `wasi:clocks/monotonic-clock` stays permitted, since it breaks test hermeticity |
| TIMER-F002 | No host-initiated application turn | Same; the design's "arm a deadline" step has no protocol home today |
| TIMER-F003 | Remaining duration must be durable guest state, duplicating what a host schedule would own | Resolves automatically once TIMER-F001/F002 are addressed; deletion path recorded below |
| TIMER-F004 | Countdown presentation cannot update without invoking the guest | Requires host-owned temporal presentation (the design's countdown node) |
| TIMER-F005 | Incremental `handle` updates can silently diverge from a fresh `view` | Materially strengthens CALC-F009; justifies building a `--verify-view-convergence` test mode (design recorded below), not yet SDK diffing/reactive dependencies |
| TIMER-F006 | No native notification effect on elapse | Decision recorded: attach a bounded notification descriptor to the schedule, not a general effects API |
| TIMER-F007 | No durable schedule or overdue recovery across process exit | Same; `restart` proves app-state persistence, not real-time reconciliation |
| TIMER-F008 | Generations are expressible as state but stale-delivery is untestable without real scheduling | Schedule generation must be host-issued identity, distinct from any guest session counter |

## Findings index

| ID | Summary | Status | Primary implication |
| --- | --- | --- | --- |
| TIMER-F001 | No host-owned temporal capability (corrected: measurement *is* possible; delivery and presentation are not) | Open | The gap is being woken and presenting, not reading a clock; `monotonic-clock` permission is a hermeticity hole |
| TIMER-F002 | No host-initiated application turn | Open | "Start" cannot arm a deadline; only mode changes |
| TIMER-F003 | Guest must persist remaining duration itself | Open | A second source of truth the host would otherwise own; deletion path recorded |
| TIMER-F004 | Countdown cannot self-update | Open | Every visible tick costs a full guest turn |
| TIMER-F005 | Incremental `handle` updates can silently diverge from a fresh `view` | Addressed in this app; platform risk remains open (tracked by youth's CALC-F009) | Turns theoretical CALC-F009 risk into a reproduced bug; justifies a convergence-checker test mode |
| TIMER-F006 | No native notification effect | Open (design decision recorded) | Elapse cannot alert the user outside the window; recommend schedule-attached notification descriptor |
| TIMER-F007 | No durable schedule / overdue recovery | Open | Time passed while Youth was closed cannot be reconciled |
| TIMER-F008 | Generations expressible but stale-delivery untestable | Open (boundary confirmation) | Schedule generation must be host-issued, not conflated with an app session counter |
| TIMER-F009 | Mode machine, bounded configuration, sessions, commands, and shortcuts are fully expressible today | Addressed (positive boundary confirmation) | The non-temporal design surface needs no new protocol — see "The main result" above |
| TIMER-F010 | The host does not enforce a button's `enabled` flag on activation; it is presentation only | Addressed (boundary confirmation) | The guest must self-guard every command; test DSL naming (`activate` vs. a future `click`) recorded |
| TIMER-F013 | The `.youth-test` DSL cannot seed pre-existing durable state, so migration paths cannot be integration-tested | Open | Migration correctness rests on unit tests plus review rather than end-to-end evidence |
| TIMER-F012 | An application cannot migrate its own durable state at load time; `view` is read-only | Open | Migration must defer to the first event turn, so legacy interpretation can never be retired on the app's own schedule |
| TIMER-F011 | Schedule ownership across app unloading is undefined | Addressed (Gate B) | The host owns schedule existence, due detection, and pending delivery; the guest owns only its transactional reaction |

## Suggested Gate B priority

Ordered so schedule correctness is proven independently of desktop
presentation, and so nothing is built on an assumption TIMER-F011 has not
yet settled:

```text
1. Define durable schedule storage independently of sleeping.
2. Define host-issued schedule identity and generation (TIMER-F008).
3. Make schedule creation, pause, resume, and cancellation transaction-bound.
4. Build the virtual clock and scheduler state machine.
5. Prove stale-wake rejection.
6. Prove restart and overdue reconciliation headlessly.
7. Add host-initiated elapsed-event delivery (TIMER-F002), settling
   TIMER-F011 (schedule ownership without a live instance) before this step.
8. Remove the Timer's manual `Advance` path (TIMER-F003's deletion list).
9. Add countdown presentation afterward (TIMER-F004).
10. Add notifications last (TIMER-F006's recorded (B) design).
```

## TIMER-F001 — No host-owned temporal capability

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Domain-model and command design, before any code was written
- **Platform:** Platform-independent source and WIT inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `wit/youth/youth-app.wit` and `wit/youth/deps/youth-state/store.wit`
  declare no time, clock, or duration type. The `application` world's only
  import is `youth:state/store@0.0.1`. There is no SDK API that reads a
  clock, and `wasi:clocks/wall-clock` is not permitted by the host's
  guest-import allowlist.
- **CORRECTION (2026-07-25):** This finding originally claimed the guest
  had *no time source at all*, and described
  `wasi:clocks/monotonic-clock` as "inert." **That was wrong**, and the
  error is recorded rather than silently edited away. A guest can in fact
  read real monotonic time today by bypassing the SDK and calling
  `std::time::Instant::now()` directly. Verified chain, in the Youth
  workspace at commit `48dce6d`:
  1. `crates/youth-runtime/src/profile.rs:28` lists
     `wasi:clocks/monotonic-clock` in `PERMITTED_GUEST_IMPORTS`.
  2. This app's own built component imports it — visible in
     `wasm-tools component wit dist/dev.saman.timer.wasm`.
  3. `configure_wasi` calls `wasmtime_wasi::p2::add_to_linker_sync`, which
     links `clocks::monotonic_clock` (`wasmtime-wasi-46.0.1`,
     `src/p2/mod.rs:345`).
  4. `WasiCtxBuilder::new().build()` yields `WasiClocksCtx::default()`,
     which installs the **real host monotonic clock**
     (`wasmtime-wasi-46.0.1`, `src/clocks.rs:57-63`) — clocks are not
     opt-in the way stdio/env/fs are.

  Confirmed from source, not yet from a running experiment; a test
  component that reads `Instant::now()` across two turns should be added
  to the Youth workspace to make this empirical and regression-tested.
- **What this does and does not change:** The design's thesis is
  *strengthened*, not weakened. What a guest can do is *measure* elapsed
  time between turns it is already running in. What it still cannot do —
  and what `youth:time` must actually provide — is be **woken** when a
  duration passes, present a countdown without a turn, or have a
  deadline survive process exit. The gap was never measurement; it is
  delivery and presentation. TIMER-F002 was already the more precisely
  stated half of this finding, and this correction makes F001 narrower
  and F002 comparatively more important.
- **Hermeticity consequence:** because the clock is real, a guest could
  observe wall-time progression that a virtual test clock does not
  control. Any Gate B work that promises deterministic, time-hermetic
  headless tests must decide whether `wasi:clocks/monotonic-clock` stays
  permitted. This is now a Gate B decision, not a documentation detail.
- **Developer impact:** A Timer author cannot express a bounded countdown
  *declaratively*, cannot be called when it elapses, and cannot present it
  without a guest turn. They can, if they reach past the SDK, measure how
  much time passed between two turns the user already caused.
- **This finding is not a request for a guest clock.** The design's own
  "capability ownership" table already places clock access, sleeping, and
  presentation refresh on the host, and this app agrees with that: nothing
  here argues a guest should ever call `now()`. What is missing is
  *declarative scheduling* ("elapse after this bounded duration") and
  *host-owned temporal presentation* (a countdown that redraws without
  invoking the guest) — not a way to measure time from inside a guest turn.
  `youth:time` should not add `now()` merely to close this finding; doing
  so would resolve the letter of the gap while missing its point. See
  TIMER-F002 for the related but distinct gap this finding does *not*
  cover: F001 is about representing time-dependent state and presentation
  at all; F002 is about the host having any way to *initiate* a guest turn
  because a deadline passed, as opposed to because a user acted.
- **What could not be expressed:** A bounded countdown, a duration, or any
  time-dependent presentation — declaratively, i.e. without the guest
  itself standing in for the missing capability by tracking elapsed time
  as ordinary state (see TIMER-F003).
- **What felt repetitive:** Nothing at this stage; the gap is total, not
  incremental.
- **What leaked WIT details:** None; the absence is visible from the WIT
  file itself without touching generated bindings.
- **What required host policy:** Deciding whether a clock should ever be
  guest-visible at all versus staying host-owned (already answered: it
  stays host-owned), and deciding the shape of the declarative scheduling
  primitive and host-owned countdown presentation that should replace it.
- **Unavoidable protocol addition:** A declarative scheduling interface
  (`youth:time` or equivalent) and host-owned temporal presentation. This
  app can supply evidence for their necessity, not for their exact shape
  (see TIMER-F002 for the delivery half of that shape).
- **What remains SDK/application behavior:** None of the scheduling or
  presentation primitive itself; the domain logic that consumes an elapsed
  notification (`Timer::advance`'s eventual replacement) is
  application-owned and generalizes cleanly regardless of where the
  notification comes from.
- **Impact:** Functional headless behavior is available for every operation
  except "wait." The published app cannot count down unattended, which is
  disclosed in `README.md` rather than hidden.
- **Resolution:** None yet. `Command::Advance` stands in for the missing
  primitive; see `src/model.rs`'s module docs for how the substitution is
  scoped, and TIMER-F003 for the exact deletion path once this lands.

## TIMER-F002 — No host-initiated application turn

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Domain-model design
- **Platform:** Platform-independent WIT and runtime inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Distinct from TIMER-F001:** F001 is that a guest has no way to
  *represent* time-dependent state or presentation at all. F002 is
  narrower and specifically about *delivery*: even granting a guest a
  perfect way to declare "elapse after this duration," nothing in the
  runtime can invoke that guest's `handle` because a deadline passed
  rather than because a user acted. A design that solved F001 alone
  (e.g. a purely declarative duration with no delivery mechanism) would
  still leave F002 unresolved.
- **Evidence:** The `lifecycle` interface exposes only `mount`, `handle`, and
  `resync`; `event-kind` has exactly one variant, `activate(node-id)`. There
  is no way for the host to invoke a guest because a deadline passed rather
  than because a user acted, and no `patch-batch` or `event` field carries a
  delay, deadline, or schedule identity.
- **Developer impact:** `start()` can only change the guest's own recorded
  mode. It cannot, in any sense the platform enforces, arm anything. If the
  process exits between `start` and the configured duration elapsing, no
  event is ever coming — not late, not at all — until something else (a
  user action, or `restart` in tests) causes the guest to run again.
- **What could not be expressed:** The design's `schedule-after(duration) ->
  schedule` capability, or any semantically equivalent "call me back."
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** Everything about delivery — fuel/deadline
  budgets for a host-initiated turn, redelivery semantics, and generation
  handling — is necessarily host policy once it exists (see the design's
  "Transaction model" and "Generations and stale wakeups" sections).
- **Unavoidable protocol addition:** A schedule-elapsed event kind and the
  host machinery to produce it. This is the design's Gate B/C scope.
- **What remains SDK/application behavior:** Deciding what an elapsed event
  *means* (finish a session, increment a counter) stays application-owned;
  only the delivery mechanism is a platform gap.
- **Impact:** The Timer's central promise — "notify me later" — cannot be
  built at all today, headless or windowed.
- **Resolution:** None yet.

## TIMER-F003 — Remaining duration must be durable guest state

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** `src/model.rs` and `src/app.rs` design
- **Platform:** Platform-independent source inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** The design's "Application model" section explicitly lists
  what the guest should persist, and it does **not** include a continuously
  decreasing remaining-time counter — that is supposed to be reconstructed
  from a host-owned schedule. `Timer` in `src/model.rs` has to store
  `remaining_seconds` anyway (see the field's doc comment), because nothing
  else can compute it: there is no schedule to ask.
- **Developer impact:** The guest keeps a second, guest-owned copy of exactly
  the number the design assigns to the host. Once `youth:time` exists, this
  field becomes redundant and should be deleted, not extended.
- **What could not be expressed:** "Remaining time" as a derived,
  host-reconstructed value.
- **What felt repetitive:** Every mutation that changes `mode` also has to
  decide what happens to `remaining_seconds` (freeze it, reset it, or
  decrement it) — six of `Timer`'s eight operations touch this field.
- **What leaked WIT details:** None.
- **What required host policy:** N/A for this app; the eventual fix is
  entirely a host capability.
- **Unavoidable protocol addition:** None beyond TIMER-F001/F002 — this
  finding is a direct consequence of those, not an independent gap.
- **What remains SDK/application behavior:** The `MM:SS` formatting and the
  decision to elapse at exactly zero remain application-owned regardless of
  where the countdown value comes from.
- **Impact:** No correctness risk today (the field is validated on every
  load via `Timer::is_valid`), but it is dead weight against the design's
  intended architecture.
- **Deletion path:** This is scaffolding, not an alternative production
  path to keep once `youth:time` exists. When it lands:

  ```text
  Remove:
    - guest-owned remaining_seconds (Timer struct field and its accessor)
    - the "Advance 1s" / "Advance 10s" commands and buttons
    - Timer::advance
    - the "advance-*" entries in app.rs's command table
    - persistence of a continuously changing remaining-time value

  Replace with:
    - a host schedule identity held as opaque guest state
    - host schedule generation (see TIMER-F008 — host-issued, not
      guest-invented)
    - a countdown presentation reference (the design's
      Countdown::new(node!("remaining"), schedule)) instead of a
      guest-formatted MM:SS string
    - handling for a schedule-elapsed event instead of a manual command
  ```

  Tests should advance the design's virtual host clock at that point
  (`advance time <duration>` in the design's own test-language sketch),
  not continue calling `Timer::advance` — the manual command is a Gate A
  substitute for the missing capability, not a feature to keep alongside
  it. `.youth-test`'s `Command::Key`/`Command::Activate` distinction
  (TIMER-F010) suggests the future virtual-clock control belongs as its
  own new `.youth-test` command family, analogous to how `key` was added
  for keyboard policy rather than overloading `activate`.
- **Resolution:** None yet; tracked for removal once TIMER-F001/F002 land.

## TIMER-F004 — Countdown presentation cannot self-update

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** `src/app.rs` design
- **Platform:** Platform-independent runtime and desktop-crate inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `youth-desktop`'s event loop runs with `ControlFlow::Wait`
  and redraws only on an OS input event or a guest-turn result; nothing
  requests a redraw on a timer. The host renders exactly what the guest's
  tree last said. There is no mechanism for on-screen text to change
  without a full guest `handle` turn (which, per TIMER-F001/F002, can only
  be triggered by a user action, never by time passing).
- **Developer impact:** A visually "live" countdown does not exist; every
  digit change costs a real transactional turn and a boundary round trip
  (durable-state read+write, tree validation, patch application).
- **What could not be expressed:** The design's `Countdown::new(...)`
  presentation node that the host redraws "without invoking the guest,
  opening a transaction, producing patches, or increasing boundary
  traffic."
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** Deciding that presentation refresh must
  stay host-owned and off the guest's boundary (again, matched by the
  design's own capability-ownership table).
- **Unavoidable protocol addition:** A host-owned countdown/presentation
  concept tied to a schedule, which cannot exist before TIMER-F001/F002.
- **What remains SDK/application behavior:** None; this is purely a host
  rendering capability.
- **Impact:** At Gate A there is no runtime cost to measure (no per-tick
  boundary traffic exists to instrument — see the Metrics section below),
  which is itself evidence of how much of the design is unbuildable today,
  not evidence of good performance.
- **Resolution:** None yet.

## TIMER-F005 — Incremental `handle` updates can silently diverge from a fresh `view`

- **Status:** Addressed in this app; the underlying platform risk remains open (already tracked as CALC-F009 in the Youth workspace)
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** First `youth test` run against `tests/basic.youth-test`
- **Platform:** macOS, headless real runtime
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `handle` returns an `Update`, and the SDK adapter
  (`youth-sdk`'s `component.rs`) applies it as an incremental patch to the
  *existing* retained tree (`tree.apply(&update)`); it never re-invokes
  `view`. Only `resync` re-runs `view` from scratch. The first version of
  `src/app.rs`'s `handle` returned only `set_text` for the three display
  nodes and never reissued any button's `enabled` state. After
  `activate add-10s` made the "Start" button eligible, `key enter` (its
  declared `Shortcut::Enter`) did nothing: the host's shortcut resolver
  (`youth-interaction`) only searches buttons the *retained tree* currently
  marks enabled (`crates/youth-interaction/src/lib.rs`'s `enabled_buttons`),
  and the retained tree still held the button's mount-time `enabled: false`
  because nothing had ever told it otherwise. The test failure was exact
  and reproducible: `expect text mode "Mode: running"` observed
  `"Mode: idle"` because the shortcut silently resolved to no target.
- **Developer impact:** A handler that is individually correct (right
  model, right text) can still leave the tree in a state the UI cannot act
  on, with no error from the SDK, the host, or a test that only checks
  `activate` (bypassing shortcut resolution) rather than `key`. This is not
  hypothetical: it happened on the first real test run of this app.
- **What could not be expressed:** A declarative "recompute everything
  `view` would have set" operation. `Update` has no such primitive; every
  affected node's every affected field must be named.
- **What felt repetitive:** Both `view` and `handle` need the same
  mode-derived `enabled` decision for eleven buttons. This app factors that
  into one `ButtonEnabled::compute` shared by both (see `src/app.rs`), which
  removes the duplication CALC-F002 already flagged but does not remove the
  need to remember to call it in `handle`.
- **What leaked WIT details:** None; the failure is entirely inside the
  SDK's semantic `Update` API and the host's tree-patch semantics, never
  raw WIT.
- **What required host policy:** Whether `handle` patches incrementally or
  the host re-derives the tree from a fresh `view` after every turn is a
  runtime architecture decision, not something this app can change.
- **Unavoidable protocol addition:** None demonstrated here — this is the
  same gap CALC-F009 already named ("more dynamic apps could omit an
  affected node and produce a live tree that differs from... output"). This
  app's `enabled` bug is concrete evidence for that prediction, not a new
  finding at the protocol level. The Timer differs from the calculator
  precisely because most of its buttons are *not* always enabled, which is
  exactly the "more dynamic application" CALC-F009 was waiting for.
- **What remains SDK/application behavior:** Computing which fields changed
  and constructing the corresponding `Update` is, and should stay,
  application-owned; the risk is forgetting a field, not the API shape.
- **Impact:** Silent behavioral divergence between `view` and `handle`
  output with no compiler or runtime error — the exact failure mode is a
  UI control that looks or (via keyboard) is enabled/disabled incorrectly
  after some turns, discoverable only by testing every control's reachable
  states, not by testing that state transitions are individually correct.
- **Materially strengthens CALC-F009:** The calculator demonstrated
  *theoretical* duplication — a handler has to remember that a display
  node changed, but its buttons never change `enabled`, so nothing there
  could actually diverge. This app produced an *actual* behavioral
  failure: a fresh `view` correctly computed `start`'s eligibility, the
  retained tree stayed stale because `handle` omitted `set_enabled`, and
  keyboard shortcut resolution silently operated on the stale tree and
  failed. CALC-F009 predicted this exact failure mode before any app had
  produced it; TIMER-F005 is that evidence.
- **Proposed tooling (not yet built in Youth):** This is now enough
  evidence to justify a debug/test convergence checker, though not yet
  enough to justify replacing explicit patches with SDK tree diffing or
  declared reactive dependencies. A future test mode could, after each
  accepted turn:

  ```text
  1. Apply the accepted Update to the previous normalized tree.
  2. Commit the application state.
  3. Invoke read-only view/resync from committed state.
  4. Normalize the reconstructed tree.
  5. Compare guest-owned semantics.
  6. Report every divergent node and property.
  ```

  With a diagnostic in this shape:

  ```text
  view convergence failed after command `add-10s`

  node `start`:
    patched tree:       enabled = false
    reconstructed view: enabled = true

  node `reset`:
    patched tree:       enabled = false
    reconstructed view: enabled = true
  ```

  This should stay out of ordinary production turns — it duplicates a
  `view` call per turn purely for verification — and could initially live
  behind an opt-in test flag, e.g. `youth test --verify-view-convergence`,
  in the Youth workspace's `crates/youth-test`. This app does not
  implement that flag (it is host tooling, out of an app repo's scope);
  it records the concrete design here because TIMER-F005 is the evidence
  that would justify building it.
- **Resolution:** `src/app.rs`'s `handle` now recomputes `ButtonEnabled` and
  calls `set_enabled` for all eleven command buttons on every state-changing
  turn, from the same function `view` uses. `tests/basic.youth-test` exercises
  the shortcut path (`key enter`, not `activate start`) specifically because
  `activate <name>` bypasses `enabled_buttons` filtering entirely and would
  not have caught this bug — see TIMER-F010, a related finding this same
  investigation produced. The platform-level question CALC-F009 raises
  (should there be a convergence check, SDK-level tree diffing, or declared
  reactive dependencies instead of explicit patches) remains open; Timer and
  Calculator together justify building the convergence checker described
  above eventually, but do not yet justify reactive dependencies or SDK
  tree diffing.

## TIMER-F006 — No native notification effect

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Domain-model design
- **Platform:** Platform-independent source inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** No notification interface, effect, or host command exists
  anywhere in the Youth workspace (`crates/youth-runtime`,
  `crates/youth-desktop`, and the WIT contracts contain no such concept).
- **Developer impact:** Reaching Elapsed while the window is unfocused or
  minimized is indistinguishable, from outside the app, from nothing having
  happened.
- **What could not be expressed:** Any "alert the user outside the window"
  effect.
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** Notification is inherently host policy
  (native OS integration); no guest-side workaround exists or should exist.
- **Unavoidable protocol addition:** A best-effort notification host effect,
  per the design's "Notification semantics" section — explicitly separate
  from and secondary to durable elapsed-event delivery (TIMER-F002).
- **What remains SDK/application behavior:** Deciding *when* to notify
  (already answered by the design: on elapse) stays application-owned.
- **Impact:** No functional loss for the headless model; a real loss for
  the app's usefulness as a background timer.
- **Design decision to make before Gate C:** There are two shapes this
  capability could take, and they are not equivalent:

  ```text
  A. Guest requests notification after elapsed delivery
     deadline expires -> host queues elapsed event -> guest handles it
     -> guest requests notification effect

     + notification content can depend on current application state
     + no notification data needs to be attached to the schedule
     - notification requires a successful guest turn
     - a trapped or unloaded guest may delay or prevent it
     - less reliable while the app is backgrounded/unloaded

  B. Notification intent is attached to the schedule
     guest schedules deadline with a bounded notification descriptor
     -> host persists both transactionally -> deadline expires
     -> host displays a best-effort notification directly
     -> host independently queues the durable elapsed event

     + does not depend on waking the guest successfully
     + matches runtime-resident background behavior
     + notification and elapsed delivery stay operationally separate
     - schedule API becomes slightly larger
     - notification content must be declared in advance
     - localization / dynamic content need future consideration
  ```

  This app's recommendation is **(B)**, a small bounded notification
  descriptor attached to the schedule at creation:

  ```rust
  context.time().schedule_after(
      duration,
      ScheduleOptions::new()
          .notification(Notification::new(
              "Timer complete",
              "Your timer has elapsed.",
          )),
  )?;
  ```

  This does not make notification delivery durable or exactly-once — it
  is still best-effort, per the design's own "Notification semantics"
  section — it only lets the host attempt it directly when the schedule
  becomes due, without depending on a guest turn succeeding first, which
  matters specifically because TIMER-F011 raises the same "no live guest
  instance" scenario this decision needs to survive.
- **Resolution:** None yet; explicitly deferred behind TIMER-F002 by the
  design itself. Recording the (B)-shaped decision now so Gate B/C
  scoping does not default to (A) by omission; this finding should not by
  itself be read as requiring a general, freeform notification effects
  API — a bounded, schedule-attached descriptor is the narrower and
  recommended shape.

## TIMER-F007 — No durable schedule or overdue recovery across process exit

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** `tests/basic.youth-test` design (the `restart` command)
- **Platform:** macOS, headless real runtime
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `restart` in `.youth-test` (and the design's own "restart
  recovery" requirement) stops and respawns the app against the same
  durable-state file, then re-mounts. This app's test proves `mode` and
  `completed_sessions` survive a restart correctly — but that is proof of
  ordinary durable-state persistence, not of the design's "overdue
  recovery": nothing reconciles *how much real time passed while the
  process was not running*, because nothing durable ever recorded a
  deadline (TIMER-F002) or has access to wall-clock time to compare against
  (TIMER-F001, `wasi:clocks/wall-clock` is not in the guest import
  allowlist).
- **Developer impact:** A "restart-safe" Timer test can pass completely
  while giving no evidence at all about the behavior the design actually
  cares about (a session that elapsed while Youth was closed).
- **What could not be expressed:** Any test or code path involving elapsed
  wall-clock time across a process boundary.
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** Overdue reconciliation ordering
  (deadline-then-creation-sequence, per the design) is inherently host
  policy once a durable schedule exists.
- **Unavoidable protocol addition:** Durable schedule storage plus a
  `recovered-overdue` event reason, matching the design's "Youth process
  exited" section. Cannot exist before TIMER-F001/F002.
- **What remains SDK/application behavior:** Interpreting a
  `recovered-overdue` event once delivered (same as an ordinary elapsed
  event) stays application-owned.
- **Impact:** This is the clearest illustration in the whole findings set of
  the difference between "durable state" (which Youth has today, and this
  app uses correctly) and "durable schedules" (which Youth does not have).
- **Resolution:** None yet.

## TIMER-F008 — Generations are expressible but stale-delivery is untestable

- **Status:** Open (boundary confirmation)
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** `src/model.rs` design
- **Platform:** Platform-independent source inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `Timer::generation` in `src/model.rs` correctly increments on
  `start` and `reset`, matching the design's rule that starting or resetting
  begins a new generation a stale delivery should not be able to affect.
  But nothing in this app can *produce* a stale delivery to test against,
  because nothing delivers anything asynchronously (TIMER-F002) — the field
  exists and is unit-tested (`start_advances_generation`,
  `reset_returns_to_idle_from_any_mode_and_advances_generation`), but only
  as inert bookkeeping.
- **Developer impact:** None today; this is forward-compatibility scaffolding
  with no present cost, recorded so it is not mistaken for validated
  behavior.
- **What could not be expressed:** A stale-wakeup scenario ("generation 1 is
  cancelled, generation 2 starts, a generation 1 event arrives late").
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** N/A for this app.
- **Unavoidable protocol addition:** None beyond what TIMER-F002 already
  requires; a schedule's generation would need to be the identity a host
  schedule carries, not just a guest-state counter.
- **Generation ownership must move to the host, not merely gain a delivery
  path:** `Timer::generation` today is *application session bookkeeping*,
  and that is a different thing from *schedule delivery identity*, even
  though this app currently conflates them in one field. Once
  `youth:time` exists, the design's stale-wakeup rejection
  (`schedule-after` returning an opaque identity + generation the host
  itself assigns and checks) is not something a guest can offer to
  provide — a guest-invented counter cannot be trusted to reject a wake
  the guest itself did not generate. Keep these separate:

  ```text
  session number:
      application meaning (e.g. "this is the Nth countdown the user has
      run"). Guest-owned, guest-incremented, purely domain logic.

  schedule generation:
      host delivery identity, assigned by context.time().schedule_after(...)
      and checked by the host before ever invoking the guest with a wake.
      The guest may durably store and read it back, but must not invent it.
  ```

  A calculator-like domain model (or this Timer) may keep its own session
  number for display/counting purposes (this app's `completed_sessions`
  is exactly that), but a future `youth:time` schedule identity is a
  distinct field the guest receives from the host, not something the
  guest computes from `start`/`reset` calls the way `generation` is
  computed today.
- **What remains SDK/application behavior:** The increment rule itself
  (`start`/`reset` bump it, `resume` does not) is domain logic and stays
  application-owned regardless of what carries the identity; only the
  *delivery-rejection* generation must be host-issued.
- **Impact:** None measurable today.
- **Resolution:** None yet; becomes testable once TIMER-F002 exists. When
  it does, expect `Timer::generation` to either become dead code (deleted
  alongside the rest of TIMER-F003's deletion list) or to be explicitly
  renamed and re-scoped as a session counter distinct from the host's
  schedule generation — not silently repurposed as if it already were
  one.

## TIMER-F009 — The non-temporal design surface is fully expressible today

- **Status:** Addressed (positive boundary confirmation)
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Full app proof (model, view, handle, persistence, tests)
- **Platform:** macOS, headless real runtime and `wasm32-wasip2` build
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** Every part of the design that is not about time itself
  built without friction: the four-mode state machine, bounded
  duration configuration (clamped `[0, 99:59]`), session counting,
  `MM:SS` display formatting, eleven distinct commands each with a
  declared keyboard shortcut, durable persistence with load-time
  validation (`Timer::is_valid`), and restart-safe state. 22 unit tests
  and one full `.youth-test` scenario cover this without touching any
  raw WIT concept.
- **Developer impact:** Positive — a Timer author's actual domain work
  (not the clock) is straightforward with the current SDK, following the
  same `model.rs`/`app.rs` split, `Command`/`apply` dispatch, and
  `ReadState`-trait `load`/`save` pattern the calculator established. This
  suite's second app confirms those are reusable conventions, not
  calculator-specific accidents.
- **What could not be expressed:** Nothing, for this scope.
- **What felt repetitive:** The `mode_line`/`sessions_line`/`display`
  string-building trio is called from both `view` and `handle`, same as
  the calculator's single `display()` call — an established, accepted
  cost (CALC-F009's "felt repetitive" section already names this pattern).
- **What leaked WIT details:** None.
- **What required host policy:** Shortcut resolution and focus order stay
  host-owned, as they should; the app declares intent only.
- **Unavoidable protocol addition:** None for this scope.
- **What remains SDK/application behavior:** All of it — this finding's
  entire point is that the boundary is drawn correctly for everything
  except time.
- **Impact:** `raw_wit_concepts = 0` (see the Metrics section).
- **Resolution:** N/A; this is confirming evidence, not a defect.

## TIMER-F010 — A button's `enabled` flag is presentation only, not access control

- **Status:** Addressed (boundary confirmation)
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** `tests/basic.youth-test` design, while investigating TIMER-F005
- **Platform:** macOS, headless real runtime
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `crates/youth-runtime/src/host.rs`'s `activate` path does not
  check a node's `enabled` flag before invoking the guest; `enabled` is
  read only by `youth-interaction`'s `enabled_buttons`, which gates
  keyboard-shortcut and Tab-focus resolution but not a direct
  `activate(node)` call. `tests/basic.youth-test` demonstrates this
  directly: `activate advance-1s` succeeds and reaches the guest's
  `handle` while the "Advance 1s" button is presented disabled (the
  Timer is Paused). The guest is not protected by the host here — it must
  reject the command itself.
- **Developer impact:** An author who assumes "disabled" means "cannot be
  triggered" will ship a guest that silently trusts host-side presentation
  as if it were host-side authorization. This app's `Timer::advance` (and
  every other mode-gated operation) already no-ops correctly outside its
  valid mode, so the exposure has no effect here — but this was verified
  by inspection and by the test above, not assumed.
- **What could not be expressed:** N/A; this is a confirmed behavior of the
  existing platform, not a gap.
- **What felt repetitive:** Every one of `Timer`'s eight operations
  independently re-checks its own precondition. There is no single
  reusable "reject if not in an allowed mode" primitive; each method
  states its own `matches!` guard.
- **What leaked WIT details:** None.
- **What required host policy:** None; this finding is that host policy
  correctly and deliberately stops at presentation, leaving command
  validity entirely to the guest — consistent with Youth's untrusted-guest
  model (a guest cannot be *trusted* to keep its own state correct either,
  which is exactly why `Timer::is_valid` exists on load).
- **Unavoidable protocol addition:** None; recorded so a future app does
  not assume otherwise.
- **What remains SDK/application behavior:** All command-precondition
  checking remains, and should remain, application-owned.
- **Impact:** No defect; a documented boundary a future Utility Suite app
  should not re-discover the hard way.
- **Test DSL naming risk:** `.youth-test`'s `activate <name>` can mislead
  an author into believing it simulates user interaction; in fact it is
  direct semantic injection that bypasses every host interaction policy
  `key` goes through (enabled-button filtering, focus, shortcut
  resolution). That difference is exactly what makes it useful for
  proving TIMER-F010 — but the same property means a passing `activate`
  test proves nothing about whether a real user could reach that command
  through the UI at all. The three-way distinction worth documenting
  going forward, once it earns its complexity:

  ```text
  invoke advance-1s
      Directly inject semantic activation.
      May target a disabled control.
      Tests guest command validation. (today's `activate`, precisely named)

  click advance-1s
      Pass through host interaction policy.
      Requires the node to be enabled and hittable.

  key "1"
      Pass through focus and shortcut policy. (today's `key`, unchanged)
  ```

  This app does not need a `click` command yet — nothing here required
  proving "hittable," only "enabled." Renaming or adding to the DSL is a
  Youth-workspace `crates/youth-test` change, not something this app can
  do alone. Recorded now, in the same investigation that produced the
  gap, specifically so a future Utility Suite app does not rediscover
  TIMER-F010 by assuming `activate` means what `click` would mean.
- **Resolution:** N/A; confirming evidence, exercised by
  `tests/basic.youth-test`'s paused-advance case, which is now commented
  to state explicitly that it relies on `activate` bypassing interaction
  policy (see the test file for the exact wording).

## TIMER-F011 — Schedule ownership across app unloading is undefined

- **Status:** Addressed by Gate B (Youth `97d7b3b`)
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Design-document review against `crates/youth-runtime`
- **Platform:** Platform-independent source inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** The design's "Background and shutdown behavior" section
  requires deadlines to remain active while a window is "minimized,
  hidden, unfocused, or temporarily unloaded," and separately describes
  "overdue recovery" for when the whole Youth *process* exits. Those are
  two different lifecycles, and only the second one currently has any
  answer. `crates/youth-runtime/src/worker.rs`'s `YouthAppHandle::spawn`
  (and `spawn_ephemeral`) is the only way an app instance comes to exist:
  a dedicated worker thread owning one live `Store` and one live guest
  instance, for the lifetime of that handle. Nothing in the runtime
  represents a durable schedule as an entity independent of a spawned
  worker — there is no host-side scheduling service that could remain
  armed while an app instance is "temporarily unloaded" but the Youth
  process keeps running.
- **Developer impact:** The design's "app instance temporarily unloaded,
  but Youth stays running, and the deadline should still fire" scenario
  cannot be built on the current architecture without first answering who
  owns the schedule during that gap — and the honest current answer is
  "nobody," because nothing exists yet that is not either a live instance
  or a fully-restarted process.
- **What could not be expressed:** A durable schedule that outlives one
  particular spawned `YouthAppHandle` while the host process keeps
  running.
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** This is entirely a host-lifecycle
  question: whether a durable schedule should be indexed independently of
  any live instance (by app ID, schedule ID, generation, protocol
  version, and delivery status), with the scheduler able to, when due,
  queue the durable elapsed delivery, attempt a notification, and load or
  wake the application per host policy — rather than assuming every
  host-initiated event must target an already-running actor the way
  `YouthAppHandle` does today.
- **Unavoidable protocol addition:** None beyond TIMER-F001/F002 at the
  guest-facing layer; this finding is specifically about host-internal
  architecture (a schedule service decoupled from the worker-per-instance
  model), which a guest never observes directly except through whether
  "temporarily unloaded" deadlines actually fire.
- **What remains SDK/application behavior:** None; this is entirely a
  host runtime architecture question.
- **Impact:** If unaddressed, "temporarily unloaded" degrades silently
  into "process exited" — i.e., the design's minimized/unfocused case
  would only work by accident of the instance happening to still be
  spawned, not because anything guarantees it.
- **Resolution:** **Addressed by Gate B** in the Youth workspace
  (`95c983e`, `6d29a50`, `a924c92`, `97d7b3b`). Ownership is now
  explicit:

  > Schedule creation, persistence, due detection, wake validation,
  > pending delivery, and delivery acknowledgement are host-owned. The
  > guest owns only its transactional semantic reaction.

  Concretely: schedules live in the application's own state database and
  are readable with no runtime, no engine, and no guest instantiated
  (`StateStore::open`), so a schedule exists independently of any spawned
  `YouthAppHandle`. Due detection and pending-delivery creation require
  no live guest at all — guest availability determines only when a
  pending delivery may be *consumed*. A fired wake carries no authority:
  it enters the worker's single FIFO mailbox and is revalidated against
  durable state before any guest turn is constructed, because a pause,
  resume, or cancel may have committed after it was armed.
  Acknowledgement is a write inside the same SQLite transaction that
  commits the guest's state and installs its tree, so at-least-once is a
  storage guarantee rather than a convention.

  The intermediate state this finding named — "instance gone, state
  alive" — now has a representation. What remains genuinely open is
  narrower and is *policy*, not architecture: whether Youth should
  auto-mount an unloaded application to consume a pending delivery, or
  hold it until the app is next opened. DP2 deliberately defers that
  (decision D4g); today the delivery is retained.

## TIMER-F012 — An application cannot migrate its own durable state at load time

- **Status:** Open
- **Observed:** 2026-07-27
- **Application:** Youth Timer
- **Workflow stage:** Gate C-1 application-state migration design
- **Platform:** macOS, SDK revision `8696bd97`
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Gate C-1
- **Evidence:** `Application::view` receives a `ViewContext`, whose
  `state()` returns a **read-only** `StateReader`. `delete` and every
  `set_*` exist only on `StateWriter`, reachable solely through
  `EventContext` in `handle` (`youth-sdk` `lib.rs`: `StateReader`'s impl
  carries only `boolean`/`integer`/`text`/`bytes`; `delete` is at line
  595 inside `StateWriter`). `view` is used for **both** mount and
  resync, and resync is a read-only phase at the runtime boundary, so the
  SDK cannot widen it without breaking that invariant.
- **Developer impact:** An application that needs to rewrite its durable
  representation on upgrade — add a schema version, drop obsolete keys,
  reclassify a legacy record — **cannot do so when its state is first
  read**. Migration must be deferred to the first event turn. The
  consequence is that an application opened but never interacted with
  never completes its durable migration, so `load` must be able to
  *interpret* every legacy shape correctly and indefinitely, not merely
  survive one upgrade.
- **What could not be expressed:** A migration that runs once, at load,
  and durably normalizes state before any UI is presented.
- **What felt repetitive:** Every read path must carry legacy
  interpretation for as long as legacy records may exist, because there
  is no point at which the application can guarantee it has rewritten
  them.
- **What leaked WIT details:** None.
- **What required host policy:** Whether `mount` should be able to write
  durable state through the SDK. The runtime already treats `Mount` as a
  writable phase (`GuestCallPhase::writable()` is true for `Mount` and
  `Handle`), so the restriction is an **SDK** boundary rather than a
  protocol one — `view` is shared by mount and resync and is therefore
  typed to the weaker of the two.
- **Unavoidable protocol addition:** None. This is an SDK API-shape
  question. Options include a distinct mount-time entry point with
  writable state, or an explicit `migrate` hook invoked once per mount.
  This app does not need one to be correct — deferring to the first
  handle works — but the workaround should be a recorded decision rather
  than folklore.
- **What remains SDK/application behavior:** Classification and policy
  are application-owned regardless. Only *when* an application may write
  is at issue.
- **Impact:** Correctness is preserved (this app normalizes in memory on
  every read), but the durable rewrite is not guaranteed to happen at any
  particular time, and legacy interpretation code cannot ever be deleted
  on a schedule the application controls.
- **Resolution:** Worked around in Gate C-1: `load` interprets legacy and
  v2 records identically and reports whether a durable rewrite is
  pending; `save`, which only runs in `handle`, writes the v2 shape and
  deletes the legacy keys. Migration therefore completes on the first
  command after upgrade, and is idempotent because `load` always
  normalizes.

## TIMER-F013 — The test DSL cannot seed pre-existing durable state

- **Status:** Open
- **Observed:** 2026-07-27
- **Application:** Youth Timer
- **Workflow stage:** Gate C-1 migration testing
- **Platform:** macOS, headless `youth test`
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Gate C-1
- **Evidence:** `.youth-test` offers `mount`, `restart`, `activate`,
  `key`, `expect text`, and `expect focus`. Every one of them operates on
  an application that started from an **empty** state file — the runner
  creates a fresh temporary `state.sqlite3` per test file. There is no
  command that writes a durable record before `mount`.
- **Developer impact:** An application cannot integration-test any code
  path that only runs against state written by an *earlier version of
  itself*. Migration is exactly that path. This app can prove its
  migration policy is correct as a pure function (`migrate_legacy` in
  `src/model.rs`, six unit tests), and can prove a fresh v2 record works
  end to end, but **cannot** prove through the real runtime that a
  genuine legacy record is read, rewritten, and has its legacy keys
  deleted.
- **What could not be expressed:** "Given this durable state, mount and
  assert." Youth's own workspace tests do exactly this for the
  `youth:state` v1→v2 migration by constructing a legacy database
  directly — an application repository has no equivalent.
- **What felt repetitive:** N/A.
- **What leaked WIT details:** None.
- **What required host policy:** Whether the test DSL should be able to
  seed state, and if so in what representation. A `state set <key>
  <value>` command would leak the storage model into the test language,
  which the DSL has so far deliberately avoided. A better shape may be
  seeding from a fixture file, or a `given state <file>` prelude.
- **Unavoidable protocol addition:** None; this is test tooling in
  `crates/youth-test`.
- **What remains SDK/application behavior:** Migration policy is
  application-owned and is unit-testable if the application keeps it in a
  pure module, which is the workaround adopted here.
- **Impact:** Migration correctness rests on unit tests plus review, not
  on end-to-end evidence. For an operation whose entire purpose is to
  handle records this version of the code never wrote, that is a real
  coverage gap.
- **Resolution:** None yet. Worked around by extracting `migrate_legacy`
  into the pure model so the policy is host-testable, and by keeping
  state I/O in `app.rs` thin enough to review.
