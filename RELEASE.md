# Aeris v0.2.0 — Release Notes

The first publishable cut of Aeris: a single-binary DSL for ops,
governance, pipelines, and AI agents. Determinism is the integration
test; capabilities are values; sagas have explicit compensations;
LLM calls are taped and replayable.

This document maps each milestone in `docs/plan.md` to the artefacts
that prove it landed. Where a milestone produced a *golden trace*,
the path is given so the reader can `aeris replay` it bit-identically.

> **v0.3 addendum.** See the `v0.3 highlights` section below for the
> three-mode enforcement (`enforce = "off" | "loose" | "strict"`),
> the script-friendly surface (`loop`, `??`, `strings.*`, value
> methods, natural JSON, `ai.chat(dir)` + `chat.ask` + `kb_size`),
> the inline-error sugar (`catch` / `error()` / `defer`), the
> time-control sugar (`every` / `retry` / `timeout`), and
> `model X@v2 extends X@v1`. The manifest is now called `aeris.toml`
> (the in-memory type is `Manifest`); `lockset.toml` from prior
> drafts is renamed.

---

## Highlights

- **Two capability modes** (post-v0.2.0 / M15, superseded by M15B in
  v0.3): `aeris.toml [caps] required = false` — *prototype mode*,
  suppresses `NoCapInScope` so functions without a `cap` parameter
  may freely call capability operations (the lockset's runtime
  allow-list still applies). `required = true` — *strict mode*,
  re-enables every static check; recommended once a project becomes
  mission-critical. The narrow-caps linter helps the conversion.
- **Single static binary** (`aeris`), built locally with
  `cargo build --release` against the developer's host target. The
  release profile in `Cargo.toml` already enables `strip`, `lto`,
  and `codegen-units = 1`. Multi-target cross-compilation is left to
  the contributor; see § 9 of `docs/plan.md` for the rationale.
- **Trace-first determinism**: `aeris replay <trace> <source>` is
  bit-identical for the deterministic subset (M9.T4, M9.T5).
- **Capabilities are values**: `cap[*]` rejected outside `main`'s
  synthesised cap; allow-lists intersect the lockset ceiling
  (M2.T5 / M2.T6).
- **Sagas have explicit compensations**: a write-effectful `do` with
  `undo noop` is a static error; rollback runs in reverse order with
  bounded retry → `PartialFailure` (M2.T8 / M6).
- **Models are versioned**: every reference carries `@vN`; bare model
  use is rejected (M2.T10).
- **Effect surface is enforced**: `aeris check` prints the surface
  diff as the first hunk when `.aeris/surface.lock` is stale
  (M2.T12 / M7.T5).
- **Human-grade diagnostics**: every error references its `language.md`
  section, quotes the source span with a `^^^^` underline, and adds a
  one-line "did you mean …?" hint (M13.T3 / T4 / T5). `aeris check
  --explain <code>` is manpage-style for codes 64–71 (M13.T6).

---

## Milestone-by-milestone artefacts

| Milestone | Output | Acceptance artefact |
|---|---|---|
| **M0** Bootstrap | workspace, `aeris version` | `Cargo.toml` |
| **M1** Lexer & parser | full `language.md` surface | 100+ round-trip fixtures (`syntax::fmt::tests::FIXTURES`) |
| **M2** Static analysis | `aeris check` exit codes 64 / 65 / 66 / 67 / 68 / 70 / 71 | 200 module-level idempotency fixtures + 30 negative-fixture diagnostic snapshots |
| **M3** Pure interpreter | `aeris run <pure_file>` | tree-walk evaluator over `runtime::eval` |
| **M4** Tracing + L1 | JSONL trace; `io`, `fs`, `env`, `clock` (N2), `random` (N2) | `aeris-tests/golden/m4/*.jsonl` (`io_println`, `fs_write_read`, `env_read`, `clock_random`) |
| **M5** http + shell + contracts | N4 allow-list runtime; `requires:` / `ensures:` | `runtime::http`; trace propagation tests |
| **M6** Sagas + idempotency | forward / rollback / `PartialFailure` | `aeris-tests/golden/m6/saga_success.jsonl`, `saga_rollback.jsonl`, `saga_partial_failure.jsonl` |
| **M7** Lockset + surface | blake3-shaped pinning, `main` cap, `surface.lock` | `lockset::lockset`, `lockset::surface` |
| **M8** Models + policies | `@vN` validation at trust boundaries; deny / require / limit / audit | `runtime::eval::apply_policies` |
| **M9** L2 `ai` + tape + replay | replay bit-identical | `runtime::replay::TapeHandle` + 8 replay fixtures |
| **M10** Agents + agent_net | typed dataflow with `until:` | parser / runner |
| **M11** L2 native handlers | audit / kube / docker / mongodb / minio / rabbitmq | `runtime::eval::lookup_builtin` per backend |
| **M12** Tests + properties + fmt + V1 narrow-caps | `aeris test` / `assert` / `property` / `aeris fmt` / `--narrow-caps` | 200 fmt fixtures, 10 property fixtures, 5 fixture-mode fixtures |
| **M13** Trace diff + `aeris doc` + diagnostics | `aeris trace diff`, `aeris doc`, human-grade errors | `runtime::trace_diff`, `syntax::doc`, `check::render` |
| **M14** Performance + packaging + release | static binary, benches, `aeris init` template | `tests/bench_*.rs`, `examples/` (CI-driven multi-target packaging deferred — § 9) |

---

## v0.3 highlights — script-friendly without losing the audit story

v0.3 closes the v0.1 → v0.2 ergonomic gap that surfaced during
dogfooding. The cap discipline survives intact for production use,
but a project can opt into a relaxed surface where audit is not a
concern.

### M15B — three-mode enforcement

`[caps]` now accepts `enforce = "off" | "loose" | "strict"`. Legacy
`required = true | false` stays as an alias.

| mode | static cap check | runtime allow-list | `cap[*]` in `main` | intended use |
|---|---|---|---|---|
| `strict` | full v0.2 (E65/E66/E67/E68/E70/E71) | enforced | no | production / regulated workloads |
| `loose` | E65 relaxed for fn without `cap` | enforced | no | prototype mode (M15) |
| `off`   | E65 / E66 / E67 / E71 relaxed | bypassed | yes | single-author scripts |

`aeris init` defaults to `enforce = "off"`. The migration ladder is
single-step in either direction (`off` → `loose` → `strict`).

### M24 — script-friendly surface

Land the v0.1 ergonomics without touching the cap story:

- **`loop { }`** — keyword, desugars to `while true { }`.
- **`??`** — null-coalesce on `Result`/`Option`/`Unit`. Right-
  associative. `a ?? b ?? c` ≡ `a ?? (b ?? c)`.
- **`strings.*`** — `trim`, `lower`, `upper`, `contains`,
  `starts_with`, `ends_with`, `split`, `join`, `replace`,
  `parse_int`. All pure helpers, no `cap`.
- **Method-call sugar** on `list<T>` (`.len`, `.empty`, `.first`,
  `.last`, `.slice`, `.contains`, `.join`), `string` (full
  `strings.*` set), `map<K,V>` (`.len`, `.get`).
- **Global `len(x)`** — works on list, set, tuple, map, string,
  bytes.
- **`json.encode`, `json.stringify`, `json.parse`, `json.pretty`**
  — natural (untagged) JSON for user code. The self-tagging
  encoder stays the *trace* serialiser only.
- **`date.today() -> date`, `date.timestamp() -> int`**.
- **`io.println` natural display** — `Ok(v)` / `Some(v)` unwrap;
  records and lists become natural JSON.
- **`ai.chat(system, dir) -> Chat`** (M19.T6 reified): loads
  `*.md / .txt / .rst / .adoc / .yaml / .yml` from a directory
  into the system prompt. `chat.ask(prompt) -> string` calls the
  backend. `chat.kb_size() -> int` reports loaded files.

### M16 — string interpolation

`"x = {expr}"` replaces the legacy `\(expr)` form. `\{` / `\}`
escape literal braces. `aeris fmt --migrate-strings` rewrites
existing fixtures.

### M17 — inline error sugar

- `<expr> catch err { handler }` — block-style fallback on `Err(_)`.
- `error(msg)` — constructs `err.user(msg)`. `raise error("…")` is
  the throw form.
- `defer <stmt>` — LIFO at function exit, also on `?`, `raise`, and
  contract violation. Trace events `defer_enter` / `defer_exit`.

### M18 — time-control sugar

- `clock.sleep(d)` — L1 cap; trace event `clock_sleep`; no-op
  under replay.
- `every <d> { body }` — `loop { clock.sleep(d); body }`.
- `retry <n>, delay: <d> { body }` — bounded retry with exponential
  backoff; first `Ok` wins, last `Err` propagates.
- `timeout <d> { body }` — emits `timeout_fired` when the budget is
  exceeded; cancellation is cooperative on the next cap call.

### M19 (partial) — AI builtins

- `ai.session(system, model) -> session`,
  `ai.session_ask(session, prompt) -> (session', reply)`,
  `ai.decide(prompt, choices, retries) -> string`,
  `ai.usage() -> { total_tokens, cost_usd, calls }`.
- Deferred: `ai.extract<T>`, `ai.generate<T>`, `ai.ensemble`,
  `ai.eval`, `ai.guard`, `ai.cache`, `aeris chat` REPL.

### M23 — model extends

`model X@v2 extends X@v1 { …added fields… }` — sugar over the
explicit migration function; parent fields and `where` clauses are
merged.

### Renames

- The project manifest file is now **`aeris.toml`** (was
  `lockset.toml`). The in-memory type is **`Manifest`** (was
  `Lockset`); functions follow (`parse_manifest`,
  `ManifestError`, `EXIT_MANIFEST_ERROR`,
  `check_module_with_manifest`).
- The `lockset` Rust module is renamed to `manifest`. The "lockset"
  concept survives as a synonym for the deps section in older
  prose.

### M20 — minimal HTTP server

`net.http(port: int) -> HttpServer` binds a TCP listener and returns
a server value. `server.accept()` blocks for the next connection and
returns an `HttpReq` record with `method`, `path`, `query_raw`,
`headers`, `body`, `remote_addr`. `req.reply(status, body,
content_type?)` and `req.reply_json(status, body)` write the
response. Concurrency is the caller's job — wrap handlers in
`spawn { … }` to multiplex.

```aeris
let server = net.http(port: 8080)
loop {
  let req = server.accept()
  spawn {
    if req.path == "/health" {
      req.reply_json(200, json.encode({ status: "ok" }))
    } else {
      req.reply(404, "not found")
    }
  }
}
```

TCP / UDP listeners and `net.resolve` remain deferred to v0.4.

### M25 — kwargs + untyped parameters

- `fn f(x)` and `fn f(x, y)` parse without explicit types (treated
  as dynamic, the resolver pseudo-type `any` skips the check).
- `f(name: value)` resolves to the parameter `name` for both
  user-defined closures and L1 / L2 builtins (kwarg table in
  `runtime/eval::builtin_param_names`).
- Reserved keywords (`match`, `until`, `policy`, `agent`, …) are
  admissible as argument names: `network.run(until: "DONE")` works.

### M26 — top-level effectful statements

A `.aer` module may contain `let`, expression-statements, and
`for` / `while` / `loop` / `if` blocks outside any `fn`. They run
in declaration order before `main` (or as the program body when
`main` is absent). Module-level `var` remains forbidden — only
immutable `let` binds at module scope.

```aeris
let CLAUDE_ARGS = "--print --model claude-sonnet-4-6"
env.set(key: "AERIS_LLM_CLI", value: "claude {CLAUDE_ARGS}")
fs.mkdir("./out")

fn main() { … }
```

### M27 — script builtins

- `env.set(key, value)` + `env.must_read(key)`.
- `date.now() -> timestamp`, `date.format(t, fmt) -> string`
  (`%Y %m %d %H %M %S`).
- `list.push(x) -> int`, `list.pop() -> option<T>` — mutating
  methods on `var` bindings.
- `yaml.parse(s)`, `yaml.parse_file(path)` — v0.1-compatible
  subset: indented mappings, sequences, scalars, inline flow
  sequences. Comments and quoted strings supported.

### M28 — programmatic agent network

`ai.network(max_rounds: int) -> AiNetwork` is a free-form sibling
of the declarative `agent_net`. The runtime drives a text-based
loop: each round picks the current agent, sends the message,
records the reply in the trace, and hands off either via a
`>>NAME:` prefix in the reply or by round-robin. Termination is
the `until` sentinel match (default `"DONE"`).

```aeris
fn main() {
  var net = ai.network(max_rounds: 10)
  net.agent(name: "geologist",     system: fs.read_text("agents/geo.md"))
  net.agent(name: "risk_assessor", system: fs.read_text("agents/risk.md"))
  net.agent(name: "reporter",      system: fs.read_text("agents/rep.md"))
  let r = net.run(
    entry:   "geologist",
    message: "Analyse today's events",
    until:   "DONE",
  )
  io.println("{r.rounds} rounds")
}
```

### Deferred to v0.4+

- **TCP / UDP listeners** + `net.resolve` (parity with v0.1's
  `net.tcp.listen`).
- **M22** — full L2 handler parity with v0.1 (docker / kube /
  mongodb / minio / rabbitmq full surfaces).
- **Pipeline DSL** (`pipeline X { steps: … on_step … on_failure …
  }`) — only the `crypto-pipeline` scenario uses it.

### Acceptance

`cargo test --lib --release` → 899 / 899.
`cargo test --tests --release` → 6 / 6 (six thesis criteria).
Every example under `examples/` and every demo under `demo/`
passes `aeris check` clean.

---

## Performance baselines (M14.T3 / T4 / T5)

Measured on the v0.2.0 dev workstation (macOS arm64, release build):

| Benchmark | Result | Acceptance budget |
|---|---|---|
| Pure-fn evaluator: `sum_to(50_000)` | ~30 ms | within 5× CPython |
| JSONL trace serialisation (200_000 events) | ~3.5 M ev/s | ≥ 100 k ev/s |
| Cold start (parse + check + module env) | < 1 ms | < 50 ms |

Reproduce with:

```sh
cargo test --release --test bench_evaluator -- --nocapture
cargo test --release --test bench_trace -- --nocapture
cargo test --release --test bench_cold_start -- --nocapture
```

---

## Examples (M14.T7)

The `examples/` tree ships three minimum-viable programs that mirror
`docs/language.md` Appendices A / B / C:

| Path | What it shows |
|---|---|
| `examples/hello/main.aer` | `fn main(cap)` + `io.println` (App. A) |
| `examples/saga/main.aer` | `saga` with `intent`, `do` / `undo`, `cap.subset[...]` (App. B) |
| `examples/agent_net/main.aer` | `model@vN`, `agent`, `agent_net` with `until:` (App. C) |

Each example carries its own `aeris.toml` so `aeris check` and
`aeris run` resolve `main`'s synthesised cap end-to-end.

---

## Breaking changes

None — this is the first published version.

---

## Building from the tag

```sh
git clone https://github.com/afioravanti01/aeris-v02
cd aeris-v02
git checkout v0.2.0
cargo build --release
./target/release/aeris version    # → aeris 0.2.0
```

Cross-compilation to another platform is supported by the standard
`cargo build --release --target <triple>` recipe; install the matching
toolchain (e.g. `rustup target add x86_64-unknown-linux-musl`) and
the right linker. No binaries are pre-built — this is intentional, see
`docs/plan.md` § 9.

---

## Six success criteria (`thesis.md` § 13)

1. **Compliance officer reads a saga signature in < 30 s** —
   `examples/saga/main.aer` lists every external resource on the
   first declaration line.
2. **Every effectful call has an enclosing `intent`** — enforced
   statically by M2.T7; `examples/saga` exhibits the pattern.
3. **Failed runs reproduce bit-identically** — see
   `aeris-tests/golden/m6/saga_rollback.jsonl` and
   `aeris-tests/golden/m4/clock_random.jsonl`. `aeris replay` keeps
   `clock` / `random` pinned to the recording.
4. **Mid-step saga failure leaves only `ok` / `rolled_back` /
   `PartialFailure` outcomes** —
   `aeris-tests/golden/m6/saga_partial_failure.jsonl`.
5. **Supply-chain dep-byte swap does not execute** — M7.T2's blake3
   hash check (`lockset::lockset::verify_local_deps`).
6. **LLM-generated PR adding a network call surfaces in review** —
   M2.T12: `aeris check` prints the `.aeris/surface.lock` diff as
   the first hunk before any other diagnostics.

Criteria 2–6 are mechanically reverified by the release smoke test:

```sh
cargo test --test release_thesis_section_13
```

A green run is the acceptance gate for cutting a `v*` tag. Criterion
1 is intentionally a manual walk-through (read the saga signature,
identify every external resource in under 30 seconds — record the
session for the release notes).
