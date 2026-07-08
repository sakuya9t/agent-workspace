# Classifier measurement (shadow classification)

Status: **designed, not implemented** (backlog row MEAS). Dev-only feature,
**disabled by default**; it exists to find heuristic gaps, not to ship a
smarter classifier.

## Problem

The daemon turns terminal output into user-facing state with deterministic
heuristics — today the attention pipeline (`AttentionState`): substring
patterns over the output tail (`plugins/attention.rs::default_attention`),
Claude's screen matcher (`claude_attention`), the bell, the 4 s idle timer,
plus the echo/sticky rules in `session_manager::on_output`. These heuristics
are fast and cheap, but their misses are invisible: a free-form "which
approach do you prefer?" question with no menu chrome reads as idle, an
aider-style `(Y)es/(N)o [Yes]:` confirm has no matching pattern, and we only
learn about such gaps when a user notices a stuck badge.

We measured (2026-07-06, CPU-only inference, see "LLM budget" below) that a
local 1–2 B model classifies exactly these regex-hard cases correctly while
also making its own mistakes on trivial ones. That makes it wrong as a
*replacement*, but ideal as a **disagreement detector**: run it in shadow
over the same inputs, and every heuristic↔LLM disagreement is a candidate
hidden edge case — captured with the full input snapshot, both labels, and
the LLM's reasoning, ready to triage into a regression-test fixture or a new
pattern.

Attention is only the first instance. The same blind spot exists for every
classification heuristic the project has or will grow (exit-outcome
summaries, menu-chrome detection, escape-sequence risk, …), so the module
is built as a general shadow-classification harness with a per-task
registry — not an attention tool.

## Goals / non-goals

Goals

- **Task-agnostic core.** Any classification decision in the project — a
  pure function from a serializable input to one of a small label set — can
  be registered and shadow-evaluated. Attention is the first registered
  task, not a privileged one: the core (gate, channel, sampling, LLM
  protocol, storage, API) never mentions attention. Adopting a new task
  touches exactly two places: its own `TaskSpec` and its call site
  (see "Adopting a new classification task").
- On disagreement, persist a **replayable** snapshot: the exact classifier
  input, both results, both reasons, model + prompt version.
- Zero effect on live behavior: the LLM result never touches
  `AttentionState`, the DB session rows, or notifications.
- Negligible hot-path cost when enabled; literally one branch when disabled.

Non-goals

- Not a production feature; no client UI (curl/jq + a JSONL export is the
  MVP interface). No cloud APIs in the default path — the model runs on the
  dev's own machine, so terminal snapshots never leave the host.
- No LLM-driven attention state (that decision, if ever, is *informed by*
  this data).
- No redaction of snapshots (dev-only; see Privacy).

## Architecture

```
 monitor loop (per session)                Measurer (one tokio task)
 ┌──────────────────────────┐   bounded    ┌───────────────────────────┐
 │ on_output / on_idle      │   mpsc       │ policy gate:              │
 │  heuristic runs as today │──try_send──▶ │  dedup (input hash LRU)   │
 │  + observe(Observation)  │  (drop+count │  rate caps (rpm, probes)  │
 │    behind cheap gate     │   when full) │ pass 1: label (constrained)│
 └──────────────────────────┘              │ pass 2: reasoning (only on │
                                           │         disagreement)      │
                                           │ store → measure_samples    │
                                           └───────────────────────────┘
```

New module `crates/daemon/src/measure/`:

- `mod.rs` — `Measurer` task, `MeasureHandle` (the cheap call-site API),
  sampling policy, counters.
- `llm.rs` — `MeasureLlm` trait + the one MVP backend: an OpenAI-compatible
  HTTP endpoint (Ollama or `llama-server`) via `ureq` (already a workspace
  dep) inside `spawn_blocking`, concurrency 1.
- `prompts.rs` — per-task versioned prompt templates, label sets, and the
  heuristic→LLM label projection.
- `store.rs` — `measure_samples` schema + queries on the existing SQLite
  handle.

`SessionManager` holds an `Option<MeasureHandle>`; `None` (the default)
means the call sites cost one `is_some()` check.

### The Observation and the task registry

The core is generic over "a classifier ran". Everything task-specific —
what the input looks like, how to phrase the LLM question, how to map the
heuristic's output into the LLM's label space, how to replay — lives in one
`TaskSpec`; the Measurer, store, and API only ever see the generic shapes:

```rust
pub struct Observation {
    pub task: &'static str,          // registry key ("attention", "exit_outcome", …)
    pub variant: &'static str,       // which heuristic implementation ran
    pub scope: String,               // entity the decision is about (session id,
                                     // line hash, workspace id, …) — the unit of
                                     // probe fairness, NOT assumed to be a session
    pub salience: Salience,          // Always | Steady — the only thing sampling
                                     // needs to know about a task's triggers
    pub trigger: &'static str,       // free analytics tag ("transition", "idle", …)
    pub input: serde_json::Value,    // task-owned replayable snapshot (opaque here)
    pub heur_raw: String,            // pure classifier output (the thing under test)
    pub heur_final: Option<String>,  // after any task-side post-rules (context only)
    pub heur_reason: Option<String>,
    pub meta: serde_json::Value,     // free-form context (plugin name, exit code, …)
}

pub struct TaskSpec {
    pub task: &'static str,
    pub labels: &'static [&'static str],
    pub prompt_version: &'static str,
    /// Build the pass-1/pass-2 prompts from the opaque input.
    pub render: fn(&serde_json::Value) -> anyhow::Result<Prompt>,
    /// Heuristic output → the LLM label space (defines "agreement").
    pub project: fn(&str) -> &'static str,
    /// Re-run the heuristic on a stored input (offline replay + round-trip test).
    pub replay: Option<fn(&serde_json::Value) -> anyhow::Result<String>>,
    /// Named report categories, computed at read time in the summary endpoint
    /// (e.g. attention's `idle_but_waiting`: trigger=="idle" && llm=="WAITING").
    pub reports: &'static [(&'static str, fn(&SampleRow) -> bool)],
}

pub static REGISTRY: &[TaskSpec] = &[ATTENTION /*, EXIT_OUTCOME, …*/];
```

The input is `serde_json::Value` **at the boundary only** — each task keeps
its own typed snapshot struct next to its heuristic (attention's is
`AttentionInput { text, text_is_screen, bell, prior_state, silence_ms }`)
and serializes it at the call site. The core never learns those fields; a
task with a completely different input shape (a single line, an exit code +
final screen, a file path) fits without touching core types. `observe()`
with an unregistered task is a counted drop (`unknown_task`), never a panic
— which also keeps daemon upgrades safe against stale remote submitters
(below).

`scope` generalizes "session": it is whatever entity makes probe
rate-limiting fair for that task — attention uses the session id, a
line-level task would use a line hash, a workspace-level task the workspace
id.

**Why the comparison space is binary for attention.** Idle is a *temporal*
property (4 s of silence); a static snapshot cannot show it, and in testing
the LLM correctly refused to be sure. So the LLM is never asked "idle?" —
it is asked "is the agent waiting on the human?". This makes the projection
of `Idle` → `NOT_WAITING` deliberately falsifiable: **an idle-trigger sample
where the LLM answers WAITING is the single highest-value disagreement
category** — it is precisely the "agent asked a free-form question, regexes
missed it, session went quietly idle" gap. The report calls this category
out separately (`idle_but_waiting`).

We compare against `heur_raw` (the pure classifier), not `heur_final`,
because the sticky/echo rules are timing logic layered on top — they are not
the heuristic under test and the LLM has no visibility into them.
`heur_final` is stored for context.

### First registered task: attention (exact seams)

This is the worked example of the adoption recipe — nothing below touches
the core.

1. `on_output` (`session_manager.rs`, after the `(raw, reason)` match and the
   sticky/echo resolution): build the Observation from what is already in
   scope — `tail` or `screen` (whichever was classified), `bell`, `raw`,
   `attention`, plugin name. `salience = Always`, `trigger = "transition"`
   when `raw` differs from the previous raw sample for this session; else
   `salience = Steady`, `trigger = "probe"`.
2. `on_idle` (Activity→Idle transition): snapshot `handle.screen_text()` —
   bounded to the visible grid and current, which is what a human would look
   at to judge "is it actually waiting?". `on_idle` gains the `handle`
   parameter (it is in scope in the monitor loop). `salience = Always`,
   `trigger = "idle"`.

No new timers: steady-state coverage ("heuristic says Activity forever, but
the screen has been a question for a minute" — TUIs that redraw constantly
never fire the idle timer) falls out of the `Steady` path: `on_output` fires
constantly for such sessions and the Measurer's per-scope probe interval
admits one sample every `ASM_MEASURE_PROBE_SECS`.

### Adopting a new classification task

The recipe, in full — the point of the design is that this list has exactly
three steps and none of them are in `measure/mod.rs`, `store.rs`, or the
API:

1. Define the task's typed input snapshot next to its heuristic, with
   `Serialize`/`Deserialize` (that derives replayability).
2. Add a `TaskSpec` to the registry in `prompts.rs`: label set, prompt
   renderer, projection, optional replay fn, optional report categories.
3. Call `observe()` at the decision site (choosing `scope`, `salience`,
   `trigger`).

A shared **conformance test** runs over `REGISTRY` and asserts, for every
task: labels are non-empty and unique; `project()` maps every declared
heuristic output into the label set; `render()` succeeds on the task's
sample inputs; and, when `replay` is present, store→load→replay reproduces
`heur_raw` on those samples. Adding a task without passing conformance
fails CI — adoption stays cheap *and* disciplined.

Concrete candidates already in the project's scope (to show the shapes the
core must not preclude — none of these need core changes):

- **`exit_outcome`** — at session exit the summary classifies success vs.
  failure from the exit code (`on_exit`). Shadow: judge from the final
  screen + exit code; catches "exit 0 but the agent actually aborted" and
  the reverse. Scope = session id, one `Always` sample per exit. This is
  the planned MEAS-3 task.
- **`menu_chrome`** — `is_menu_chrome` / `is_selected_option` are
  line-level binary classifiers inside the attention heuristic. Shadow at
  the line granularity (input: the line + 3 lines of context; scope = hash
  of the line class) to find chrome shapes the character-class rules miss.
- **future** — escape-sequence risk classification (security-followups #8),
  transcript summary categories (the post-MVP Memory feature), any client-
  side heuristic via remote submission (below).

**Remote observations** (MEAS-3): `POST /api/measure/observe` accepts an
Observation JSON under the same bearer auth as the rest of the API, so
components that are not the daemon — the web client's own heuristics,
`asmux`, dev scripts — can submit to the same pipeline. The task must be
registered in the daemon's `REGISTRY` (unknown task → 422 + counted;
`input_json` capped at 16 KB). Prompting, sampling, storage, and triage are
identical from that point — the Measurer does not care where an Observation
came from.

### Hot-path discipline

`observe()` must be safe to call on every output chunk (or whatever a
task's hottest call rate is):

- Stage 1 (inline, before building the Observation): `enabled` check, a
  call-site-local `last_offered_ms` (a plain `i64` owned by the caller's own
  loop — no shared state, works identically for any task), and a global
  token check. Fail → return, **no clone of the input (e.g. the 4 KB tail)
  happens**.
- Stage 2: clone the snapshot, `try_send` on a bounded (`64`) mpsc channel.
  Full channel → drop and bump `dropped_queue_full`. Never block, never
  await, never take a lock beyond the channel.

Per-scope fairness is enforced authoritatively in the Measurer; stage 1 is
only a cheap pre-filter, so a call site that cannot keep a local hint (rare)
may skip it and rely on the Measurer's gate.

### Sampling policy (in the Measurer, off-thread)

Volume control is the design's load-bearing wall — CPU inference costs
~0.3–1.2 s per call (below), so we evaluate a trickle, chosen for signal.
The policy is task-agnostic; it needs only the two generic fields:

- **`Salience::Always`** samples (a state change, a task-defined key event —
  attention uses "transition" and "idle") are admitted, subject only to the
  global rpm cap and dedup.
- **`Salience::Steady`** samples are admitted at most once per
  `ASM_MEASURE_PROBE_SECS` per **scope** (session, line-class, … — whatever
  the task declared as its scope).
- **Dedup**: SHA-256 over (task, variant, canonicalized `input_json`); an
  LRU of recent hashes (~4096 entries) skips already-evaluated inputs — for
  attention, TUI redraw frames hash identically after trailing-whitespace
  normalization, which the task's snapshot builder applies before
  serializing.
- **Global cap**: `ASM_MEASURE_RPM` evaluations/minute (token bucket) across
  all tasks; beyond it samples are counted as `dropped_rate_cap`, not
  queued. `Always` samples are admitted from the bucket before `Steady`
  ones when both are waiting in the channel.
- Every drop path increments a counter surfaced in the summary endpoint —
  silent sampling would make agreement rates uninterpretable.

### Two-pass LLM protocol

Grounded in the 2026-07-06 benchmarks (Ryzen 7945HX, CPU-only, Q4 models;
scale ~2–4× slower for a mid laptop, ~10× for small ARM):

| call | model / mode | latency (warm) | accuracy on our hard set |
| --- | --- | --- | --- |
| pass 1 | qwen3:1.7b, no thinking, few-shot, label-constrained | 0.3–1.2 s | 4/5 (llama3.2:1b was 0-for: unusable) |
| pass 2 | qwen3:1.7b, thinking on | 3.5–17.5 s | 5/5 |

- **Pass 1** (every admitted sample): temperature 0, output constrained to
  the task's label set (GBNF grammar via `llama-server`, or
  `format`/logit-bias on Ollama; parser also tolerates leading whitespace and
  trailing chatter as a fallback). One label token; cost ≈ prefill only.
- **Pass 2** (disagreements only): re-ask with thinking/reasoning enabled and
  "explain in ≤2 sentences". Stores `llm_reasoning` and a second label
  `llm_label2`. A pass-2 flip back to agreement is recorded, not discarded —
  it usually marks the sample as genuinely ambiguous, which is itself triage
  signal. Only disagreements pay the multi-second cost, which at expected
  disagreement rates (few/hour) is noise.
- Prompt templates carry `prompt_version`; agreement rates are only
  comparable within one version, and the summary endpoint groups by it.
- Endpoint failures: per-call timeout (pass 1: 20 s, pass 2: 180 s);
  after 5 consecutive transport failures the Measurer logs one warning and
  suspends itself for 10 min (`llm_errors` counter keeps counting) — a dead
  Ollama must not produce a log torrent or a retry storm.

### Storage

One table in the existing daemon SQLite (`db_path()`), managed by
`measure/store.rs`:

```sql
CREATE TABLE IF NOT EXISTS measure_samples (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  at_ms          INTEGER NOT NULL,
  task           TEXT    NOT NULL,          -- registry key ('attention', …)
  variant        TEXT    NOT NULL,          -- heuristic implementation ('default' | 'claude' | …)
  scope          TEXT    NOT NULL,          -- task-defined entity (session id, line hash, …)
  trigger        TEXT    NOT NULL,          -- task's free tag ('transition' | 'idle' | 'probe' | …)
  meta_json      TEXT,                      -- task context (plugin, exit code, …)
  input_hash     TEXT    NOT NULL,
  input_json     TEXT,                      -- NULL for most agreements (below)
  heur_raw       TEXT    NOT NULL,
  heur_final     TEXT,                      -- NULL when the task has no post-rules
  heur_reason    TEXT,
  llm_label      TEXT,                      -- NULL = LLM error/timeout
  llm_label2     TEXT,                      -- pass-2 label (disagreements)
  llm_reasoning  TEXT,
  agree          INTEGER,                   -- project(heur_raw) == llm_label
  model          TEXT    NOT NULL,
  prompt_version TEXT    NOT NULL,
  llm_ms         INTEGER,
  triage         TEXT    NOT NULL DEFAULT 'new'
                 -- 'new' | 'heuristic_gap' | 'llm_wrong' | 'ambiguous'
);
CREATE INDEX IF NOT EXISTS idx_measure_agree  ON measure_samples(agree, triage);
CREATE INDEX IF NOT EXISTS idx_measure_at     ON measure_samples(at_ms);
```

- **Disagreements keep everything** (`input_json` always present — that is
  the product).
- **Agreements** keep the row (labels, hash, timings — needed for agreement
  rates) but `input_json` only for a 1-in-10 sample, to spot-audit LLM
  quality without growing the DB.
- Retention: prune rows older than `ASM_MEASURE_KEEP_DAYS` (default 14) on
  Measurer start and daily; triaged disagreements (`triage != 'new'`) are
  kept until exported or purged explicitly.

`input_json` is the serialized `MeasureInput` — sufficient to re-run
`default_attention` / `claude_attention` byte-identically offline. A
round-trip test asserts replayability (store → load → re-classify →
`heur_raw` matches).

### Privacy

Snapshots are raw terminal text and can contain secrets (the `password:`
pattern exists precisely because passwords get prompted). Mitigations, MVP:
data never leaves the host (local model, local DB), feature is dev-only and
default-off, retention is bounded, and `DELETE /api/measure/samples` purges
everything. Redaction is explicitly out of scope; add a row to
`security-followups.md` when this lands so the gap is tracked.

### Config (env vars, matching `config.rs` conventions)

| var | default | meaning |
| --- | --- | --- |
| `ASM_MEASURE` | unset (**off**) | `1` enables the module |
| `ASM_MEASURE_URL` | `http://127.0.0.1:11434` | OpenAI-compatible endpoint (Ollama, llama-server) |
| `ASM_MEASURE_MODEL` | `qwen3:1.7b` | pass-1/2 model name |
| `ASM_MEASURE_RPM` | `6` | global evaluations/minute cap |
| `ASM_MEASURE_PROBE_SECS` | `45` | per-session steady-state probe interval (`0` = transitions/idle only) |
| `ASM_MEASURE_KEEP_DAYS` | `14` | retention for untriaged rows |

Startup behavior when enabled: probe the endpoint once (model present?);
on failure log a single clear warning and run with the Measurer suspended
(counters still expose the suspension) rather than failing daemon startup —
the daemon's job is sessions, not measurement.

### Reading the results (the payoff loop)

HTTP (existing axum router, same auth as the rest of the API):

- `GET /api/measure/summary` — counters (evals, agree/disagree by
  task/variant/trigger/prompt_version, each task's named report categories
  from its `TaskSpec.reports` (attention: `idle_but_waiting`), drops,
  llm_errors, p50/p95 `llm_ms`).
- `GET /api/measure/disagreements?triage=new&limit=50` — full rows.
- `POST /api/measure/samples/{id}/triage` — body `{"triage":"heuristic_gap"}`.
- `GET /api/measure/export.jsonl?triage=heuristic_gap` — one sample per line.
- `DELETE /api/measure/samples` — purge.

The loop that justifies the module:

1. Run enabled for a few days of normal agent use.
2. Triage `new` disagreements:
   - **`heuristic_gap`** → export → the snapshot becomes a regression
     fixture next to the task's heuristic plus a pattern/matcher fix (for
     attention: a `repro_*` test in `plugins/attention.rs`, whose test
     module is already shaped exactly for this).
   - **`llm_wrong`** → labeled example banked for a future distilled
     classifier (a fine-tuned ~100 M encoder is the long-term cheap path;
     this table *is* its training set accumulating).
   - **`ambiguous`** → kept as documentation of genuinely undecidable
     screens.
3. Agreement-rate trend per prompt_version tells us when the shadow model
   itself is too noisy to be a useful judge.

### LLM budget (why these defaults)

From the 2026-07-06 feasibility benchmarks: qwen3:1.7b Q4 is ~1.9 GB
resident while loaded (Ollama auto-unloads after idle), pass-1 calls are
0.3–1.2 s warm on 8 desktop threads and ~2–3× that at 2 threads. At the
default 6 rpm cap the worst-case duty cycle is ≈10% of one core on a dev
workstation — and in practice transitions + dedup keep it far below the cap.
This is a dev-machine budget by design; the module is not meant to run on
small VPS/SBC hosts, which is another reason it is default-off rather than
auto-enabled.

### Testing

- `MeasureLlm` is a trait → policy tests use a scripted fake (agree,
  disagree, error, timeout) and assert sampling/dedup/cap/suspension
  behavior deterministically — against a synthetic test task, not
  attention, so core tests stay decoupled from any real heuristic.
- The registry **conformance test** (see "Adopting a new classification
  task") covers every registered task: label/projection totality, prompt
  rendering, and store→load→replay round-trips on the task's sample inputs.
- Label-parser tolerance tests (leading whitespace, trailing chatter).
- One `#[ignore]`d integration test that talks to a live local Ollama when
  `ASM_MEASURE_URL` is reachable, for manual runs.

### Milestones

- **MEAS-1**: task-agnostic core (registry, gate, Measurer, store,
  summary/disagreements endpoints, purge, conformance test) + attention as
  the only registered task. Pass 1 only (`agree` + snapshots;
  `llm_reasoning` empty). Immediately useful: disagreement rows already
  carry the input. The generic shapes are **not** deferred — MEAS-1's core
  must already compile against the synthetic test task, so generality is
  proven before a second real task exists.
- **MEAS-2**: pass-2 reasoning, triage endpoint, JSONL export, retention
  prune, per-task report categories (`idle_but_waiting`).
- **MEAS-3**: second real task (`exit_outcome`) + `POST /api/measure/observe`
  for remote submitters — exercises the adoption recipe end to end and
  opens the pipeline to non-daemon components.
