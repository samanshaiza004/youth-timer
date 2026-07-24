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

## Open findings

| ID | Summary | Next decision |
| --- | --- | --- |
| TIMER-F001 | No guest-visible clock; a countdown cannot advance on its own | Requires `youth:time` design and implementation evidence beyond this app |
| TIMER-F002 | No scheduled wakeup or elapsed-event delivery | Same; the design's "arm a deadline" step has no protocol home today |
| TIMER-F003 | Remaining duration must be durable guest state, duplicating what a host schedule would own | Resolves automatically once TIMER-F001/F002 are addressed |
| TIMER-F004 | Countdown presentation cannot update without invoking the guest | Requires host-owned temporal presentation (the design's countdown node) |
| TIMER-F006 | No native notification effect on elapse | Deferred until a scheduling capability exists to notify about |
| TIMER-F007 | No durable schedule or overdue recovery across process exit | Same; `restart` proves app-state persistence, not real-time reconciliation |
| TIMER-F008 | Generations are expressible as state but stale-delivery is untestable without real scheduling | Becomes testable once a host schedule assigns delivery identity |

## Findings index

| ID | Summary | Status | Primary implication |
| --- | --- | --- | --- |
| TIMER-F001 | No guest-visible clock | Open | A countdown can only advance on user or test action |
| TIMER-F002 | No scheduled wakeup / elapsed delivery | Open | "Start" cannot arm a deadline; only mode changes |
| TIMER-F003 | Guest must persist remaining duration itself | Open | A second source of truth the host would otherwise own |
| TIMER-F004 | Countdown cannot self-update | Open | Every visible tick costs a full guest turn |
| TIMER-F005 | Incremental `handle` updates can silently diverge from a fresh `view` | Addressed in this app; platform risk remains open (tracked by youth's CALC-F009) | Every mode-dependent button `enabled` state must be explicitly reissued on every turn |
| TIMER-F006 | No native notification effect | Open | Elapse cannot alert the user outside the window |
| TIMER-F007 | No durable schedule / overdue recovery | Open | Time passed while Youth was closed cannot be reconciled |
| TIMER-F008 | Generations expressible but stale-delivery untestable | Open (boundary confirmation) | Needs host schedule identity to mean anything |
| TIMER-F009 | Mode machine, bounded configuration, sessions, commands, and shortcuts are fully expressible today | Addressed (positive boundary confirmation) | The non-temporal 90% of the design needs no new protocol |
| TIMER-F010 | The host does not enforce a button's `enabled` flag on activation; it is presentation only | Addressed (boundary confirmation) | The guest must self-guard every command regardless of what the UI shows |

## TIMER-F001 — No guest-visible clock

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Domain-model and command design, before any code was written
- **Platform:** Platform-independent source and WIT inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
- **Evidence:** `wit/youth/youth-app.wit` and `wit/youth/deps/youth-state/store.wit`
  declare no time, clock, or duration type. The `application` world's only
  import is `youth:state/store@0.0.1`. The host's guest-import allowlist
  (`docs/GUEST-PROFILE.md` in the Youth workspace) permits
  `wasi:clocks/monotonic-clock` only as an inert artifact of linking Rust
  `std`; there is no SDK API that reads it, and `wasi:clocks/wall-clock` is
  not permitted at all.
- **Developer impact:** A Timer author cannot ask "how much real time has
  passed" from inside `view` or `handle`. The design's central primitive —
  a bounded countdown — has no time source.
- **What could not be expressed:** Any duration measurement, elapsed-time
  query, or "now" reading.
- **What felt repetitive:** Nothing at this stage; the gap is total, not
  incremental.
- **What leaked WIT details:** None; the absence is visible from the WIT
  file itself without touching generated bindings.
- **What required host policy:** Deciding whether a clock should ever be
  guest-visible at all versus staying host-owned (the design's own
  "capability ownership" table already answers this: clock access stays
  host-owned).
- **Unavoidable protocol addition:** A `youth:time` interface, or an
  equivalent host-owned scheduling capability. This app cannot supply
  evidence for its exact shape (see TIMER-F002), only for its necessity.
- **What remains SDK/application behavior:** None of the clock itself; the
  domain logic that consumes a tick (`Timer::advance`) is application-owned
  and generalizes cleanly regardless of where the tick comes from.
- **Impact:** Functional headless behavior is available for every operation
  except "wait." The published app cannot count down unattended, which is
  disclosed in `README.md` rather than hidden.
- **Resolution:** None yet. `Command::Advance` stands in for the missing
  primitive; see `src/model.rs`'s module docs for how the substitution is
  scoped.

## TIMER-F002 — No scheduled wakeup or elapsed-event delivery

- **Status:** Open
- **Observed:** 2026-07-23
- **Application:** Youth Timer
- **Workflow stage:** Domain-model design
- **Platform:** Platform-independent WIT and runtime inspection
- **Local path:** `/Users/keina/dev/youth-timer`
- **Commit:** Initial app proof
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
- **Resolution:** `src/app.rs`'s `handle` now recomputes `ButtonEnabled` and
  calls `set_enabled` for all eleven command buttons on every state-changing
  turn, from the same function `view` uses. `tests/basic.youth-test` exercises
  the shortcut path (`key enter`, not `activate start`) specifically because
  `activate <name>` bypasses `enabled_buttons` filtering entirely and would
  not have caught this bug — see TIMER-F010, a related finding this same
  investigation produced. The platform-level question CALC-F009 raises
  (should there be a convergence check, SDK-level tree diffing, or declared
  reactive dependencies instead of explicit patches) remains open; this
  finding adds a second application's evidence toward answering it.

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
- **Resolution:** None yet; explicitly deferred behind TIMER-F002 by the
  design itself.

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
- **What remains SDK/application behavior:** The increment rule itself
  (`start`/`reset` bump it, `resume` does not) is domain logic and stays
  application-owned regardless of what carries the identity.
- **Impact:** None measurable today.
- **Resolution:** None yet; becomes testable once TIMER-F002 exists.

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
- **Resolution:** N/A; confirming evidence, exercised by
  `tests/basic.youth-test`'s paused-advance case.
