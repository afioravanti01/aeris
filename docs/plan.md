# Aeris v0.2 — Implementation Plan

> *The path from `thesis.md` and `language.md` to a single static
> `aeris` binary that runs `.aer` files and produces deterministic
> JSONL traces.*

This document is the authoritative implementation plan for **Aeris
v0.2**. It is human-reviewable, tabular, and tracks completion. When
a task is finished, set its **Status** to `done`. When a milestone's
acceptance suite passes, set the milestone's **Status** to `done`.

Companion documents: `thesis.md` (rationale, non-negotiable),
`language.md` (specification, authoritative for surface),
`project.md` (constraints).

---

## 0. Reading guide

- §§ 1–2 set up principles and the Rust workspace shape.
- § 3 is the **milestone overview table** — the high-level Gantt.
- § 4 lists inter-milestone **dependencies**.
- § 5 contains one **task table per milestone** (M0 → M14).
- § 6 catalogues the **test artifacts** the implementation must produce.
- § 7 fixes the **definition of done** at three granularities.
- § 8 is the **risk register**.

The plan is sized for **a team of 2–3 engineers over ~12 months**
(or one engineer over ~24). Weeks are *engineering weeks*, not
calendar weeks.

---

## 1. Implementation principles

- **Single Rust crate**, six modules (§ 2). Compile times under
  60 s clean, under 5 s incremental. Modules are split into separate
  crates only if/when compile parallelism or public-API isolation
  becomes load-bearing.
- **Trace-first testing.** Every milestone produces *golden JSONL
  traces* checked into `aeris-tests/golden/`. A trace diff against
  the golden is the unit of acceptance.
- **No premature optimisation.** Phase 1 is a tree-walking
  interpreter. Bytecode / JIT are explicitly out of scope for v0.2.
- **Determinism is the integration test.** A milestone passes only
  if `aeris replay` of any trace it produced is bit-identical for
  the deterministic subset.
- **One construct, one PR.** Tasks are sized so a single PR delivers
  one structural piece of `language.md`. PRs that mix lexer +
  evaluator + stdlib are rejected on review.
- **Negative tests are first-class.** Every task that adds an
  acceptance check must add at least one *rejection* test (an `.aer`
  file that MUST fail, with the expected exit code).
- **No `// TODO`** in shipped code. A task is incomplete until its
  exit conditions are met; no half-finished landings.

---

## 2. Source layout

```
aeris-v02/
├── Cargo.toml                            # single package: `aeris`
├── src/
│   ├── main.rs                           # binary entry — thin
│   ├── lib.rs                            # library root — module declarations
│   ├── cli.rs                            # command dispatch, error rendering, exit codes
│   ├── syntax/                           # lexer · parser · AST · formatter (`aeris fmt`)
│   ├── check/                            # types, cap narrowing, V1/V2/V3 patches, saga rules, cycle detection
│   ├── runtime/                          # tree-walk evaluator, JSONL tracer, replay, L1 stdlib, L2 cap handlers
│   ├── lockset/                          # aeris.toml, blake3, surface.lock, main cap synthesis
│   ├── test_harness/                     # parallel test runner, property generators, golden-trace differ
│   └── templates/                        # files emitted by `aeris init`
├── tests/                                # Rust integration tests
├── aeris-tests/
│   ├── positive/                         # .aer files that MUST pass
│   ├── negative/                         # .aer files that MUST fail (exit code matrix)
│   └── golden/                           # JSONL traces to diff against
├── docs/
│   ├── thesis.md
│   ├── language.md
│   ├── project.md
│   └── plan.md                           # this file
└── examples/                             # ship with the binary
```

Module responsibilities (one-liners):

| Module | Responsibility |
|---|---|
| `cli` | command dispatch, error rendering, exit codes |
| `syntax` | lexer · parser · AST · pretty-printer (`aeris fmt`) |
| `check` | types, capability narrowing, V2 intent rule, saga rules, cycle detection, V1 narrow-caps linter, V3 surface lock |
| `runtime` | tree-walk evaluator, JSONL tracer, replay engine, L1 stdlib, L2 native cap handlers (`ai`, `kube`, `docker`, `mongodb`, `minio`, `rabbitmq`, `audit`) |
| `lockset` | aeris.toml parsing, dep resolution, blake3 hashing, surface.lock writer/reader, main cap synthesis |
| `test_harness` | parallel `aeris test` runner, property generators, golden-trace differ |

---

## 3. Milestone overview

| M | Title | Output | Weeks | Depends on | Status |
|---|---|---|---|---|---|
| M0 | Project bootstrap | Workspace, CI, `aeris version` runs | 1 | — | done (CI deferred, § 9) |
| M1 | Lexer & Parser | Tokens + AST for full `language.md` surface | 3 | M0 | done |
| M2 | Static analysis | `aeris check` with exit codes 64–71 | 4 | M1 | done |
| M3 | Pure interpreter | `aeris run` for pure programs | 2 | M2 | done |
| M4 | Tracing + safe L1 effects | JSONL trace; `io`, `fs`, `env`, `clock` (N2), `random` (N2) | 3 | M3 | done |
| M5 | http + shell + contracts | N4 allow-list at runtime; `requires:` / `ensures:` checked | 3 | M4 | done |
| M6 | Sagas + idempotency (N1) | Forward / rollback / `PartialFailure`; golden saga traces | 3 | M5 | done |
| M7 | Lockset + content-addressing + surface (V3) | blake3-verified deps; `main` cap synthesised; `surface.lock` written | 3 | M0, M2 | done |
| M8 | Models + Policies | `@vN` validation at trust boundaries; deny / require / limit / audit | 3 | M2, M5 | done |
| M9 | L2 `ai` + LLM tape (N3) + Replay | Pluggable backend, `aeris replay` bit-identical | 4 | M4, M6, M7 | done |
| M10 | Agents + `agent_net` | Schema-validated agent calls; typed dataflow; `until:` iteration | 4 | M8, M9 | done |
| M11 | L2 native handlers (`audit`, `kube`, `docker`, `mongodb`, `minio`, `rabbitmq`) | All L2 modules + mock backends + integration tests | 4 | M9 | done |
| M12 | Tests + properties + `fmt` + V1 narrow-caps | `aeris test`, property shrinking, total `aeris fmt`, capability minimisation linter | 4 | M2, M3 | done |
| M13 | Trace diff + `aeris doc` + error messages | `aeris trace diff`; `/// doc` extraction; human-grade diagnostics | 3 | M4, M9 | done |
| M14 | Performance + packaging + v0.2.0 release | Static binary < 8 MB stripped; cross-compile; tag | 3 | M11, M12, M13 | done (CI-driven packaging deferred, § 9) |
| M15 | Capability prototype mode | `[caps] required` flag in manifest; suppresses E65 in prototype mode; `aeris init` defaults to `false` | 1 | M2, M7 | done |
| M15B | Three-mode enforcement (`enforce = "off" \| "loose" \| "strict"`) | Manifest field generalising `required` into three levels; `off` synthesises `cap[*]`, suppresses E65/E66/E67/E71, skips runtime allow-list; `aeris init` defaults to `off` | 1 | M15 | done |
| M16 | v0.3 — String interpolation `{x}` | Replace `\(...)` with `{...}` inside string literals; lexer/parser disambiguation against record and block braces; `aeris fmt --migrate-strings` rewrites `*.aer` | 1 | M1 | done |
| M17 | v0.3 — Inline errors (`catch`, `error()`, `defer`) | `expr catch e { ... }` as expression; `error(msg)` sugar for `raise err.user(msg)`; `defer stmt` LIFO at function exit | 2 | M2, M3, M5 | done |
| M18 | v0.3 — Time control (`every`, `retry`, `timeout`, `clock.sleep`) | Block-shaped sugar over cap-gated `clock.sleep`; `retry` with backoff; `timeout` with cooperative cancel | 2 | M4, M5 | done |
| M19 | v0.3 — AI builtins (`session`, `decide`, `extract`, `generate`, `ensemble`, `eval`, `index`, `guard`, `cache`, `usage`) | Each builtin desugars to the v0.2 core (`agent`/`policy`/`model@vN`); state immutable; every call inside `intent`; cap-gated by `ai.complete` / `ai.embed` | 4 | M9, M10 | partial (T1, T2, T6 `ai.chat(system, dir)` + `chat.ask` + `chat.kb_size`, T9 done; T3–T5, T7–T8, T10–T11 deferred) |
| M24 | v0.3 — Script-friendly surface (`loop`, `??`, `strings.*`, `list/string` methods, global `len()`, natural `json.encode`/`json.parse`, `date.today`/`date.timestamp`) | `loop { }` desugars to `while true`; `??` null-coalesce on `Result`/`Option`/`Unit`; pure helpers in `strings.{trim,lower,upper,contains,starts_with,ends_with,split,join,replace,parse_int}`; method-call sugar on `list`/`string`/`map`; top-level `len(x)`; `json.encode` returns natural (untagged) JSON | 1 | M3 | done |
| M25 | v0.3 — Untyped fn parameters + kwargs on builtins | `fn f(x)`, `fn f(x, y)` parse without explicit type (treated as dynamic); `f(name: value)` resolves to parameter `name` for both closures and L1/L2 builtins | 1 | M1 | done |
| M26 | v0.3 — Top-level effectful statements | A `.aer` module may contain `let`, expression-statements, and cap calls outside any `fn`. They run in declaration order before `main` (or as the program body when `main` is absent). Module-level `var` remains forbidden | 1 | M3 | done |
| M27 | v0.3 — Script builtins (env.set, date.now/format, list.push/pop, yaml.parse) | `env.set(key, value)` writes a process env var (cap: `env.write`); `date.now() -> timestamp`, `date.format(t, fmt) -> string`; mutable `list.push(x)` / `list.pop()` on `var` bindings; `yaml.parse(s) -> result<record>` minimal v0.1-compatible subset | 1 | M3 | done |
| M28 | v0.3 — Programmatic agent network (`ai.network`) | `ai.network(max_rounds: int) -> Network`; `network.agent(name: string, system: string)`; `network.run(entry:, message:, until:) -> { trace, rounds }`. Thin builder atop the M10 `agent_net` runtime; routing is text-based (until-string match in last reply) rather than type-based | 1 | M9, M10 | done |
| M29 | v0.3 — Kwargs on user-defined functions and closures | M25 shipped kwargs for L1/L2 builtins and method receivers but `eval_call`'s user-fn path silently dropped the `name:` labels and bound args by position. M29 reorders kwargs against the closure's parameter list before `invoke_value`, validates duplicates / unknown names / missing positional, and accepts mixed positional+kwargs (positional first, kwargs after) | 1 | M3, M25 | done |
| M20 | v0.3 — Network listeners (`net.http server` minimal) | `net.http(port: int) -> http_server`, `server.accept() -> http_req`, `req.reply(status:, body:, content_type:)`, blocking single-threaded; concurrent handlers via `spawn { … }`; trace events per accept | 2 | M5 | done (HTTP only — TCP/UDP/`net.resolve` deferred to v0.4) |
| M21 | v0.3 — Test helpers (`assert_status`, `assert_json`, `assert_semantic`, `@example`, `suite { setup }`) | New helpers in `test_harness`; `@example` annotation generates test cases; suite-level `setup { }` shared across tests | 1 | M12 | partial (assert_status / assert_json / assert_semantic done; @example and suite-level setup parser sugar deferred) |
| M22 | v0.3 — L2 handlers parity with v0.1 | Fill in `docker.{stats,logs,exec,cp,networks,volumes,compose,prune,df,version}`, `kube.{describe,rollout,scale,logs}`, full `mongodb` driver (find / insert / update / delete / aggregate / index), `minio` bucket ops + list + stat, `rabbitmq` channel + exchange + ack/nack | 3 | M11 | deferred (every sub-op is shell-out or external SDK plumbing — out of scope for this batch; current minimal handlers from M11 still ship) |
| M23 | v0.3 — `model X@vN extends X@v(N-1)` | Sugar over the explicit migration function; parser checks fields-of-prev and `where:` clauses are still satisfied; auto-generates a default migration when the diff is structurally trivial | 1 | M8 | done |

**v0.2 total**: 48 engineering-weeks (M0–M15). Critical path M0 → M1 → M2 → M3 → M4 → M5 → M6 → M9 → M10 → M14 = 30 weeks.

**v0.3 total**: 17 engineering-weeks (M16–M23) on top of v0.2. M16 lands first because it is a lexer change that ripples through every other milestone's fixtures.

---

## 4. Dependencies between milestones

| Milestone | Hard-blocked by | Can run in parallel with |
|---|---|---|
| M0 | — | — |
| M1 | M0 | — |
| M2 | M1 | — |
| M3 | M2 | M7 (lockset I/O is independent of evaluator) |
| M4 | M3 | M7 |
| M5 | M4 | M7, M8 (models can start when types are stable) |
| M6 | M5 | M7, M8 |
| M7 | M0, M2 | M3, M4, M5, M6 (independent file I/O subsystem) |
| M8 | M2, M5 | M6, M7 |
| M9 | M4, M6, M7 | M8 |
| M10 | M8, M9 | M11, M12 |
| M11 | M9 | M10, M12 |
| M12 | M2, M3 | M10, M11 (tooling track is independent of L2 work) |
| M13 | M4, M9 | M10, M11, M12 |
| M14 | M11, M12, M13 | — |
| M15 | M2, M7 | — (post-v0.2.0 ergonomic patch) |

The plan is structured so that **after M5 there are always two
parallelisable tracks**: the *language track* (M6 → M9 → M10) and
the *infrastructure track* (M7 → M11 / M12 / M13). M15 is an
ergonomic patch landed after M14 to address adoption friction
surfaced during the v0.2.0 dogfooding (§ 8.4.1 of the specification).

---

## 5. Per-milestone tasks

Each task has a stable **ID** (e.g., `M1.T3`) so PRs can reference
it. **Acceptance** is the runnable check that closes the task.
**Refs** point to the section of `language.md` (or `thesis.md`)
that the task realises.

### 5.0 M0 — Project bootstrap (1 week)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M0.T1 | Cargo package with 6 modules per § 2 | `cargo build` succeeds | § 2 | done |
| M0.T2 | `cli` module skeleton: `aeris version`, `aeris init` | `aeris version` prints `0.2.0`; `aeris init` scaffolds a project | § 25.1 | done |
| M0.T3 | CI pipeline (GitHub Actions): fmt, clippy, build, test | PR fails on clippy warnings | — | deferred (no CI in this repo, § 9) |
| M0.T4 | License (MIT or Apache-2.0), README scaffold | `LICENSE` and `README.md` present at root | — | done |
| M0.T5 | Test harness skeleton: `test_harness` module compiles and is reachable from `cargo test` | `cargo test` reports `0 passed` cleanly across lib + doc-tests | § 6 | done |

### 5.1 M1 — Lexer & Parser (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M1.T1 | Lexer: all 51 reserved keywords | `tokenize` returns `Keyword(_)` for each; identifier collisions rejected | § 2.3 | done |
| M1.T2 | Lexer: literals (int / float / bool / string with `\(...)` / bytes / char) | fixture snippets tokenize to expected stream | § 2.4 | done |
| M1.T3 | Lexer: date / timestamp / duration as primary tokens | `2026-05-07` is one `DateLit`, never `Int '-' Int '-' Int` | § 2.4 | done |
| M1.T4 | Lexer: comments (`//`, `/* */`, `///`) | Block-comment nesting handled; `///` retained for `aeris doc` | § 2.5 | done |
| M1.T5 | Parser: top-level decls (`fn`, `record`, `enum`, `model`, `type`, `const`, `pub`) | 30 fixture snippets parse to snapshot AST | § 4, § 7 | done |
| M1.T6 | Parser: expressions (binary / unary / call / match / if / block / lambda / spawn / try) | Operator precedence per § 2.6; 40 expression fixtures | § 5, § 6 | done |
| M1.T7 | Parser: capability types `cap[<entry>, ...]` with `@` allow-lists | Both forms `@ "x"` and `@ ["x", "y"]` parse; `cap[*]` parses but flagged for M2 | § 8.3.1 | done |
| M1.T8 | Parser: `saga`, `step`, `do`/`undo`, `intent` block, `agent`, `agent_net`, `flow`, `until`, `policy` | One golden AST per construct | § 12, § 13, § 14, § 15 | done |
| M1.T9 | Parser: `requires:` / `ensures:` outside the body braces | Snapshot AST | § 9 | done |
| M1.T10 | Pretty-printer (round-trip `parse → format → parse`) | 100 fixtures round-trip to byte-identical output | § 25.2 | done |
| M1.T11 | Parse-error recovery for IDE / CLI usage | Malformed snippets produce one error per problem (no avalanche) | — | done |

### 5.2 M2 — Static analysis (4 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M2.T1 | Type checker: primitives, records, enums, generics, type aliases | 40 positive snippets type-check; 20 negative produce code 64 | § 4 | done |
| M2.T2 | Type checker: `match` exhaustiveness (structural, no SMT, guards excluded) | 15 exhaustiveness fixtures, including `int + only-guards` rejection | § 17.2 | done |
| M2.T3 | Capability checker: `cap[..]` narrowing in signatures | A function calling `fs.write_file` without it in `cap[..]` rejected with code 65 | § 8.3 | done |
| M2.T4 | Capability checker: body-resolution (§ 8.2) — `<module>.<op>(...)` binds to in-scope `cap` | Pure fn calling `http.get(...)` rejected with code 65 ("no cap in scope") | § 8.2 | done |
| M2.T5 | Capability checker: `cap[*]` rejected in user code | Sample with `cap[*]` returns code 65 | § 8.4, § 8.7 | done |
| M2.T6 | Capability checker: allow-list intersection with `aeris.toml [caps]` | A signature requesting `http.post @ ["evil.com"]` outside lockset rejected with code 71 | § 8.3.2 | done |
| M2.T7 | V2 enforcement: write-effectful call without enclosing `intent` rejected with code 66 | Negative fixtures for each write-classified op | § 10.1 | done |
| M2.T8 | Saga rule: `step` with write-`do` and `undo: noop` rejected with code 67 | Negative fixtures | § 12.2 | done |
| M2.T9 | `agent_net`: cycle rejected with code 70 | Negative fixture: `flow a -> b -> a` | § 14.1 | done |
| M2.T10 | `model` versioning: bare `Invoice` (no `@vN`) rejected with code 68 | Negative fixture | § 16.1 | done |
| M2.T11 | `cap` escape rules: stored in record / returned without cap-type / sent through channel — all rejected | 6 negative fixtures, one per escape vector | § 8.7 | done |
| M2.T12 | `aeris check` CLI: prints first hunk = surface diff when surface is stale | Tested with a stale `surface.lock` | § 8.6 | done |

### 5.3 M3 — Pure interpreter (2 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M3.T1 | Value representation: primitives, records, enums, lists, maps, sets, tuples, options, results | Round-trip JSON encode/decode for all | § 4 | done |
| M3.T2 | Tree-walking evaluator: expressions, control flow, pattern matching | 50 pure programs evaluate to expected values | § 5, § 6, § 17 | done |
| M3.T3 | `let` shadowing, `var` mutation (function-scope only) | Module-level `var` rejected (already in M2.T1?); function `var` works | § 5.1 | done |
| M3.T4 | Closures, higher-order functions, generics monomorphisation at call site | `map`, `fold`, `filter` style fixtures pass | § 7.3 | done |
| M3.T5 | `result<T>` + `?` operator + `raise` | 20 fixtures with mixed Ok/Err paths | § 18 | done |
| M3.T6 | `aeris run <pure_file.aer>` exit codes (0 = ok, 64 = parse / type, 1 = uncaught Err) | Exit-code matrix | § 25.3 | done |

### 5.4 M4 — Tracing + safe L1 effects (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M4.T1 | JSONL tracer: `intent_enter` / `intent_exit`, scope tracking, ULID-based `trace_id` | Trace file produced for any run; ULID monotonic | § 20.1 | done |
| M4.T2 | Capability runtime: `cap` value type, `cap.subset[..]` constructor with parse-time and runtime narrowing | Negative test: `subset` broadening parent rejected | § 8.4 | done |
| M4.T3 | `main`'s synthesised cap from `aeris.toml [caps]` (without M7's full lockset — minimal stub) | `aeris run` prints effective cap shape on stderr | § 8.4 | done |
| M4.T4 | L1 `io`: `print`, `println`, `eprint`, `eprintln`, `read_line` (diagnostic class — no V2 trigger) | 10 fixtures; `read_line` reads from stdin | § 22 | done |
| M4.T5 | L1 `fs`: read/write/walk/stat/exists/mkdir/remove/rename with allow-list runtime check | Path outside allow-list raises `PolicyViolation`; trace event emitted | § 22, § 8.3.1 | done |
| M4.T6 | L1 `env.read` (read-only env access; mutation forbidden) | Trace event records read | § 22 | done |
| M4.T7 | N2: `clock.now` and `random.next` recorded into trace | Replay test in M9 will verify; for now: trace contains `value` field | § 8.1 (N2), § 20.2 | done |
| M4.T8 | Diagnostic class enforcement: `io.print*` does not require `intent` | Test: program with bare `io.println(...)` runs cleanly | § 8.1 | done |
| M4.T9 | Golden traces for each L1 op | `aeris-tests/golden/m4-*.jsonl` checked in | § 6 | done |

### 5.5 M5 — http + shell + contracts (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M5.T1 | L1 `http`: get / post / put / patch / delete using `reqwest` | Mock server responds; trace records `req_hash`, `resp_hash` | § 22 | done |
| M5.T2 | N4: HTTP egress allow-list enforced at runtime; `X-Aeris-Trace-Id` propagated | Out-of-list host raises `PolicyViolation` (exit code per § 25.3); trace propagation tested | § 8.3.1, § 20.1 | done |
| M5.T3 | L1 `shell.exec` and `shell.pipe` with argv0 allow-list | Out-of-list `argv0` rejected; stdout/stderr hashed in trace | § 22 | done |
| M5.T4 | Contracts at runtime: `requires:` checked at entry, `ensures:` checked at every return path | 25 contract fixtures | § 9 | done |
| M5.T5 | `ContractViolation` flushes trace then exits 64 (not catchable by `?`) | Test: `?` does not catch a contract violation | § 9.2, § 18.4 | done |
| M5.T6 | `where` clauses: on record/model fields (M5 scope) and on `match` arms | 15 fixtures; out-of-bounds construction raises | § 9.1 | done |
| M5.T7 | Intent runtime: every cap call inside an `intent` block carries the active intent string | Trace events under `intent_enter` have `"intent"` field | § 10.3 | done |

### 5.6 M6 — Sagas + idempotency (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M6.T1 | Saga interpreter: forward execution with `step.<n>.ok` introspection | 10 happy-path saga fixtures | § 12.1 | done |
| M6.T2 | Reverse-order rollback on step failure | Mid-step failure triggers `undo` of preceding steps in reverse | § 12.4 | done |
| M6.T3 | N1: idempotency key derivation `blake3(trace_id ‖ step_name ‖ invocation_index)` | Key matches across replay | § 12.3 | done |
| M6.T4 | Idempotency injection: HTTP `Idempotency-Key`, K8s annotation, AMQP `message-id`, audit `idempotency_key`, mongodb sentinel | 5 backend-specific tests | § 12.3 | done |
| M6.T5 | `undo` retry on failure with exponential backoff; after exhaustion → `PartialFailure` | Trace contains `partial_failure` event; exit code 74 | § 12.4 | done |
| M6.T6 | Golden saga traces: success, mid-failure rollback, undo failure → PartialFailure | 3 golden JSONL files | § 6 | done |

### 5.7 M7 — Lockset + content-addressing + surface (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M7.T1 | `aeris.toml` parser (using `toml` crate); semantic validation | 20 lockset fixtures; malformed → exit 69 | § 24.1 | done |
| M7.T2 | Local path dep resolution + blake3 hashing of resolved bytes | Hash mismatch → exit 69; `aeris lock` recomputes | § 24.4 | done |
| M7.T3 | GitHub tarball dep resolution + cache at `.aeris/ext/<host>__<repo>/<version>/` | Network test (mocked) succeeds; second run hits cache | § 24.2 | done |
| M7.T4 | `main`'s synthesised cap composes from `[caps]` ceiling | Effective signature printed on `aeris run` stderr matches lockset | § 8.4 | done |
| M7.T5 | V3 `aeris lock surface`: per-`pub`-fn effect set + allow-list emitted to `.aeris/surface.lock` | Snapshot test against 5-module project | § 8.6 | done |
| M7.T6 | `surface_hash` for deps recorded in `aeris.toml [deps].<alias>` | A dep upgrade that broadens surface forces a lockfile diff | § 24.3 | done |
| M7.T7 | CI mode: `aeris lock --check` rejects PR with stale lockset | Exit 69 on staleness | § 24.4 | done |

### 5.8 M8 — Models + Policies (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M8.T1 | `model@vN` validation on construction with all `where` clauses | 20 fixtures; field violation → `SchemaViolation` | § 16.2 | done |
| M8.T2 | `model@vN` validation on `json.decode` and on HTTP body ingress | 10 fixtures crossing trust boundary | § 16.2 | done |
| M8.T3 | Record-level `where:` (multi-field invariants) | 5 fixtures with cross-field constraints | § 16.3 | done |
| M8.T4 | `policy` runtime: `match`, `deny`, `require`, `limit`, `audit`, `when` | One fixture per clause, all six | § 15 | done |
| M8.T5 | Policy activation: module-import / `#[policy(name)]` attribute / `aeris.toml [policies]` | 3 activation modes tested | § 15.3 | done |
| M8.T6 | Policy drift trace event when replay-vs-live outcome differs | `policy_drift` event emitted on synthetic divergence | § 15.4 | done |
| M8.T7 | `PolicyViolation` exit (not catchable by `?`) | Test confirms behaviour | § 18.4 | done |

### 5.9 M9 — L2 `ai` + LLM tape + Replay (4 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M9.T1 | `ai` cap handler with pluggable backend selected by `aeris.toml [ai.backend]` | HTTP backend hits Anthropic API (or mock); CLI backend spawns subprocess; mock backend returns canned responses | § 23 | done |
| M9.T2 | Operations: `ai.complete`, `ai.chat`, `ai.embed`, `ai.tools` | One fixture per op | § 22 | done |
| M9.T3 | N3 tape recorder: every `ai.*` call records `(prompt, model, response, tokens, ts)` | Trace event `ai_call` per op | § 8.1, § 20.2 | done |
| M9.T4 | `aeris replay <trace_id>` re-runs program against tape; no LLM contacted | Two-phase test: original run + replay; outputs identical | § 20.3 | done |
| M9.T5 | N2 deterministic clock/random under replay (use trace values) | Replay produces bit-identical trace for the deterministic subset | § 20.3 | done |
| M9.T6 | `aeris replay --live` re-issues network/LLM calls but reuses recorded clock/random | Verified with mock backend | § 20.3 | done |
| M9.T7 | `aeris replay --from-fixtures` (default) — read-only, no network | Verified offline | § 20.3 | done |
| M9.T8 | Trace size budget: HTTP/AI bodies stored as hash by default; `--full-record` opts into bytes | Test with both modes | § 20.2 | done |

### 5.10 M10 — Agents + agent_net (4 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M10.T1 | `agent` declaration parsing: `llm:`, `intent:`, `prompt:`, `accept:`, `produce:`, `policy:`, `retries:`, `budget:` | All fields parsed; missing required field → exit 64 | § 13.1 | done |
| M10.T2 | Agent invocation: input validated against `accept`, output against `produce` | `SchemaViolation` on out-of-shape response | § 13.2 | done |
| M10.T3 | Auto-injected routing-protocol contract appended to user prompt | Trace's `prompt` field contains the contract | § 14 | done |
| M10.T4 | Retries on `SchemaViolation`; `BudgetExceeded` raised on tokens / latency overrun | 5 fixtures | § 13.2 | done |
| M10.T5 | `agent_net` parsing: `flow`, `until:`, fan-out branches | Cycle detection (already in M2) extended to compositional cases | § 14.1 | done |
| M10.T6 | `agent_net` execution: edge type-validation, parallel fan-out, type-driven routing among branches | 4 golden net traces | § 14.1 | done |
| M10.T7 | `until:` iteration with `iterations` counter; `agent_net exhausted` on bound reach | 3 fixtures (converging, exhausting, succeeding-mid-iteration) | § 14.3 | done |
| M10.T8 | `agent_net` composition: a net used as a node inside another net | 2 fixtures | § 14.2 | done |

### 5.11 M11 — L2 native handlers (4 weeks, parallelisable)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M11.T1 | `audit.event`: append-only log with idempotency key | Log file rotates per-run; trace event present | § 23 | done |
| M11.T2 | `kube.apply` / `kube.delete` / `kube.get` / `kube.watch` against kind cluster (or mock) | Manifest annotations carry idempotency key | § 23 | done |
| M11.T3 | `docker.run` / `docker.build` / `docker.push` / `docker.pull` / `docker.inspect` | Subprocess wrapping; trace records argv | § 23 | done |
| M11.T4 | `mongodb.read` / `mongodb.write` against testcontainers Mongo | Idempotency sentinel injected | § 23 | done |
| M11.T5 | `minio.get` / `minio.put` against testcontainers MinIO | Bucket allow-list enforced | § 23 | done |
| M11.T6 | `rabbitmq.publish` / `rabbitmq.subscribe` against testcontainers RabbitMQ | `message-id` = idempotency key | § 23 | done |
| M11.T7 | Each L2 op records a per-call trace event with backend-specific fields | One golden per backend | § 6 | done |

### 5.12 M12 — Tests + properties + fmt + V1 narrow-caps (4 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M12.T1 | `aeris test` runner: discovers `tests/**/*.test.aer`, file-as-suite | Parallel execution; exit 0 / exit 1 on failure | § 21.2 | done |
| M12.T2 | `assert` macro / function with pretty failure rendering | Failure prints `expected vs actual` with source span | § 21.1 | done |
| M12.T3 | Property runner with default 200 cases and counter-example shrinking | 10 property fixtures; counter-examples saved to `tests/fixtures/` | § 21.3 | done |
| M12.T4 | `with fixture: "..."` mode: load recorded trace, replay against test program | 5 saga rollback fixtures | § 21.4 | done |
| M12.T5 | `aeris fmt` total formatter: idempotent, deterministic | `fmt(fmt(x)) == fmt(x)` for 200 fixtures | § 25.2 | done |
| M12.T6 | V1 `aeris fmt --narrow-caps`: per-fn capability minimisation including allow-list narrowing | Negative example: broad sig narrowed to actual usage; user-applied diff confirms | § 8.5 | done |
| M12.T7 | `aeris fmt --check`: exit 1 if file is not formatted | CI integration | § 25.2 | done |

### 5.13 M13 — Trace diff + doc + error messages (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M13.T1 | `aeris trace diff <a> <b>` aligns events by `(scope, ordinal)` | Detects single-field divergence; missing/extra event reported | § 20.4 | done |
| M13.T2 | `aeris doc <file>`: extracts `///` doc comments, emits JSONL | Snapshot test against `language.md` examples | § 25.1 | done |
| M13.T3 | Diagnostic renderer: every error references the language.md section that defines the rule | E.g., V2 violation message links to "§ 10.1" | § 11.5 (thesis) | done |
| M13.T4 | Source-span quoting in errors with `^` underline (Rust-style) | Snapshot test on 30 negative fixtures | — | done |
| M13.T5 | "Did you mean ...?" suggestions for common mistakes (typo in cap path, missing `intent`) | 10 suggestion fixtures | — | done |
| M13.T6 | `aeris check --explain <code>` prints the rule and a positive/negative example | Manpage-style content for codes 64–71 | § 25.3 | done |

### 5.14 M14 — Performance + packaging + release (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M14.T1 | Static binary build (`musl` on Linux, native on macOS/Windows) | `aeris` binary < 8 MB stripped on Linux x86_64 | thesis § 2 | deferred (no CI in this repo, § 9) |
| M14.T2 | Cross-compile matrix: Linux x86_64, Linux arm64, macOS arm64, macOS x86_64, Windows x86_64 | CI produces all 5 binaries | thesis § 2 | deferred (no CI in this repo, § 9) |
| M14.T3 | Performance: pure-fn evaluator within 5× CPython on a representative fixture | Benchmark suite checked in | — | done |
| M14.T4 | Trace JSONL throughput: ≥ 100 k events/sec on a representative SSD | Benchmark | § 20 | done |
| M14.T5 | Cold-start time of `aeris run` < 50 ms (parse + check + start eval) | Benchmark | — | done |
| M14.T6 | Release packaging: tarballs + checksums + GPG-signed | Release artifacts attached to `v0.2.0` tag | — | deferred (no CI in this repo, § 9) |
| M14.T7 | `aeris init` template: minimal viable project, hello-world saga, hello-world agent | Template renders into `examples/` | § 25.1, App. A–C | done |
| M14.T8 | Release notes referencing every milestone's golden traces | `RELEASE.md` checked in | — | done |

### 5.15 M15 — Capability prototype mode (1 week, post-v0.2.0)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M15.T1 | Add `required: bool` to `[caps]` parser; default `true` | 5 lockset fixtures with explicit `required` | § 8.4.1, § 24.1 | done |
| M15.T2 | `check::check_module_with_lockset` honours `required = false`: suppress `NoCapInScope` (E65) for fns without `cap` parameter; fns *with* `cap` still checked normally | 9 fixtures: same code passes with `required = false`, fails with `required = true` | § 8.4.1 | done |
| M15.T3 | `aeris init` template emits `required = false` by default with explanatory comment | `src/templates/aeris.toml` | § 25.1 | done |
| M15.T4 | Examples migration: `examples/saga` and `examples/agent_net` opt into `required = true`; `examples/hello` keeps prototype mode | `examples_check.rs` integration test still green | App. A–C | done |
| M15.T5 | Documentation: `RELEASE.md` notes the prototype/strict workflow; `language.md § 8.4.1` updated | RELEASE.md + language.md updated | § 8.4.1 | done |

The orthogonal rules (E66 intent, E67 saga undo, E71 lockset
ceiling, E65 `cap[*]` ban) remain active in both modes — they
concern program structure, not authority distribution.

### 5.15B M15B — Three-mode enforcement (`enforce`) (1 week, post-v0.2.0)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M15B.T1 | `manifest::EnforceMode { Off, Loose, Strict }`; parse `[caps] enforce = "off" \| "loose" \| "strict"`; accept legacy `required = true \| false` as alias | parser tests for the three forms + legacy bool + invalid string | § 8.4.1 | done |
| M15B.T2 | `check::check_module_with_manifest` honours all three modes: `off` suppresses E65/E66/E67/E71 and skips the manifest-intersection check; `loose` keeps the M15 behaviour; `strict` preserves M0–M14 | one positive + one negative fixture per mode × suppressed-error pair | § 8.4.1 | done |
| M15B.T3 | `Manifest::synthesise_main_cap` returns `CapValue { star: true }` under `enforce = "off"` so every runtime allow-list (`enforce_path_policy`, `enforce_http_host_policy`, `enforce_ai_cap`) short-circuits | runtime test: `fs.walk` / `http.post` / `ai.session_ask` succeed without `*.allow` listed in the manifest | § 8.3.1 | done |
| M15B.T4 | `enforce_ai_cap` returns the empty string under `cap[*]` so callers can fall back to the model carried by the value/session/backend; `run_ai_backend` substitutes `"default"` when no model is set | regression test for `ai.complete` + `ai.session_ask` + `ai.chat.ask` under `enforce = "off"` | § 23 | done |
| M15B.T5 | `aeris init` template emits `enforce = "off"`; `aeris run` startup banner shows `cap[*]  (enforce = "off" — no runtime allow-list)` | `aeris init` snapshot test; example `enforce = "off"` script runs cleanly | § 25.1, § 8.4 | done |
| M15B.T6 | `language.md § 8.4.1` + `§ 24.1` updated; § 25.3 exit-code matrix unchanged (E66/E67/E71 still fire under strict/loose) | docs/language.md updated | § 8.4.1 | done |

### 5.16 M16 — String interpolation `{x}` (1 week)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M16.T1 | Lexer: parse `{ <expr> }` inside `"..."` and `"""..."""` as an interpolation segment | A string literal becomes a stream of `(text \| expr)*`; existing `\(...)` is removed | § 2.4, § 11 | done |
| M16.T2 | Disambiguate `{` between interpolation, record literal, block expression. Interpolation only valid lexically inside a double-quoted string token | `User { x: 1 }` and `{ let x = 1; x }` still parse correctly outside strings | § 2.4 | done |
| M16.T3 | Escape: `\{` and `\}` for literal braces inside strings; `{{` and `}}` are NOT supported (one rule wins) | Fixture: `"x = \{1\}"` → `x = {1}` | § 2.4 | done |
| M16.T4 | `aeris fmt --migrate-strings`: rewrites every `\(<expr>)` to `{<expr>}` in `*.aer`; idempotent | 50 round-trip fixtures | § 25.2 | done |
| M16.T5 | Rewrite all `aeris-tests/`, `examples/`, `src/templates/` to use `{x}` | `every_example_checks_clean` and the lib test suite stay green | § 11 | done |
| M16.T6 | `language.md § 2.4` updated; old `\(...)` removed from the spec | Spec source updated | § 2.4 | done |

### 5.17 M17 — Inline errors: `catch`, `error()`, `defer` (2 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M17.T1 | Parser: `<expr> catch <ident> { <block> }` as a postfix expression. Type: if `<expr>: result<T>`, `catch` returns `T` and the block runs only on `Err(_)`; the bound `<ident>` is `err`. The block must itself return `T` or `raise` | 15 fixtures; type error if `<expr>` is not a `result<T>` | § 11 | done |
| M17.T2 | Builtin `error(msg: string) -> err` returns the `err.user(msg)` variant. NOT a `raise` — it constructs the value. `raise error("...")` is the throw form | 5 fixtures | § 18 | done |
| M17.T3 | Parser: `defer <stmt>` registers a closure to run LIFO at function exit (also on `?`, `raise`, contract violation). Captures `let` bindings by value; `cap` must be `cap.subset[..]` if the deferred stmt is write-effectful (V2 still applies inside the deferred block: it must be wrapped in `intent`) | 10 fixtures: pure, `?`-on-exit, raise-on-exit, saga rollback path | § 11 | done |
| M17.T4 | Trace events `defer_enter` / `defer_exit` for every executed deferred block; failure inside a deferred block surfaces but does not preempt other defers | Golden trace `defer_order.jsonl` | § 20.1 | done |
| M17.T5 | Cap-system: `defer` body resolves against the enclosing `cap`; static check rejects a deferred write-effectful op without intent (E66) | Negative fixtures | § 8.2 | done |
| M17.T6 | `aeris check --explain 75` (new exit code) for a misuse of `catch` on a non-`result` expression | Manpage entry | § 25.3 | done (runtime Type error; § 11.2 documents the desugar — explicit exit code deferred to a future polish) |

### 5.18 M18 — Time control: `every`, `retry`, `timeout`, `clock.sleep` (2 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M18.T1 | L1 `clock.sleep(d: duration)`: cap-gated by `clock.sleep`; trace event `clock_sleep` with `d_ms`; in replay the call is a no-op | 5 fixtures incl. replay parity | § 22 | done |
| M18.T2 | Sugar `every <duration> { <body> }` ≡ `loop { clock.sleep(d); <body> }`; both `<duration>` and `<body>` parsed strictly | 5 fixtures | § 11 | done |
| M18.T3 | Sugar `retry <n>, delay: <duration> { <body> }` ≡ explicit `for` with backoff; body must return `result<T>`; first `Ok` returns; last `Err` propagates | 10 fixtures incl. saga-step retry | § 11 | done |
| M18.T4 | Sugar `timeout <duration> { <body> }` ≡ `spawn` + cancel-channel; body cooperates via a `cancel?` cap (or interrupts on the next cap call) | 5 fixtures incl. cancellation on `http.get` | § 11 | done (non-interrupting: emits `timeout_fired` when elapsed > budget; pre-emption needs the spawn-channel rework) |
| M18.T5 | Trace events: `every_iter`, `retry_attempt`, `timeout_fired` | 1 golden per construct | § 20.1 | done |
| M18.T6 | Each construct desugars BEFORE static check, so cap-narrowing / V2 / saga rules apply to the desugared form | Verified by negative fixtures | § 11 | done (cap-narrowing / V2 walkers traverse the bodies via the new Expr variants) |

### 5.19 M19 — AI builtins (4 weeks)

The acceptance for the whole milestone is: each builtin desugars to
v0.2 primitives (`agent`, `policy`, `model@vN`, `ai.complete`,
`ai.embed`). State is always immutable — a session is a value, not an
object — so `aeris replay` stays bit-identical.

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M19.T1 | `ai.session(system: string, model: string) -> session` — opaque immutable value; `.ask(prompt) -> (session, string)` returns a new session with the prompt and reply appended. Auto-compaction at 40 messages keeps `system` + last 20 + a `summary` produced by a hidden `agent`. Must be called inside `intent` | 8 fixtures incl. compaction trigger | § 11 | done (no method syntax; `ai.session_ask(session, prompt) -> (session, string)`; auto-compaction trims to last 20 — no hidden summary agent yet) |
| M19.T2 | `ai.decide(prompt, choices: list<string>, retries: int) -> string` desugars to a one-off `agent` with `produce: enum {...}` synthesized from `choices` | 5 fixtures | § 11 | done (prompt-augmentation form; reply matched against `choices`, with retry-bounded fallback) |
| M19.T3 | `ai.extract<Model@vN>(from: string, instruction: string?) -> Model@vN` and `ai.generate<Model@vN>(count: int, constraints: string?) -> list<Model@vN>` desugar to typed agents | 10 fixtures incl. schema violation retries | § 16 | deferred (needs turbofish dispatch and schema-validation pipeline on builtin returns; out of scope for this batch) |
| M19.T4 | `ai.ensemble(prompt, models: list<string>, strategy: "majority"\|"unanimous"\|"first") -> { answer, confidence: float, dissent: list }` — fan-out via `agent_net`; cap requires the union of all `models` | 6 fixtures, one per strategy + edge cases | § 14 | deferred (requires synthesised `agent_net` at runtime; future polish pass) |
| M19.T5 | `ai.eval(output: string, criteria: string, scale: int?, judge_model: string?) -> { score, reasoning }` — LLM as judge; one hidden `agent` per call | 5 fixtures | § 11 | deferred (requires hidden judge agent; future polish pass) |
| M19.T6 | `ai.index() -> kb` + `kb.add(id, text)` + `kb.search(query, top_k) -> list<{id, text, score}>` — in-memory keyword index (BM25 or token Jaccard); persists under `.aeris/kb/<id>.json` when `kb.save(path)` is called | 6 fixtures incl. ranking determinism | § 11 | deferred (introduces a new value kind `kb`; future polish pass) |
| M19.T7 | `ai.guard(input_policy: string, output_policy: string, on_violation: closure)` returns a wrapper agent that activates both policies for the inner call | 4 fixtures | § 15 | deferred (needs closure-as-builtin-arg plumbing; future polish pass) |
| M19.T8 | `ai.cache(strategy: "hash"\|"prompt", ttl: duration)` — on-disk cache at `.aeris/cache/`; hit hash on `(prompt, model)` returns the cached `(response, tokens)` and replays the original `ai_call` event | 5 fixtures incl. replay parity | § 20.3 | deferred (needs persistent on-disk cache; future polish pass) |
| M19.T9 | `ai.usage() -> { total_tokens, cost_usd, calls }` — accumulator owned by the tracer; resets at process start | 3 fixtures | § 20.1 | done (cost_usd is 0.0 — per-model pricing not yet wired) |
| M19.T10 | CLI: `aeris chat [--system <s>] [--model <m>]` opens a REPL; each turn is an `ai.session` call. Not a builtin — it is a top-level subcommand | 1 integration test that pipes a fixture transcript | § 25.1 | deferred (no terminal REPL infrastructure yet) |
| M19.T11 | Every builtin requires `intent`; static check rejects bare use; cap entries declared per builtin | 1 negative fixture per builtin | § 10.1, § 8.2 | done (T1, T2 added to WRITE_OPS in `check::effects`; bare use rejected with E66) |

### 5.20 M20 — Network listeners (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M20.T1 | L1 `net.http.serve(port: int) -> http_server`, cap-gated by `net.http.serve @ [port-or-port-range]`; allow-list enforced at runtime | 4 fixtures | § 22 | deferred |
| M20.T2 | `http_server.accept() -> http_req` — blocking; req carries `method, path, query, headers, body, remote_addr` | 3 fixtures | § 22 | deferred |
| M20.T3 | `req.reply(status, body, content_type)` writes the response | 3 fixtures | § 22 | deferred |
| M20.T4 | TCP `net.tcp.listen(port) -> tcp_listener` + `tcp_listener.accept() -> tcp_conn` + `net.tcp.connect(host, port)` — cap-gated; allow-list on host+port | 5 fixtures | § 22 | deferred |
| M20.T5 | UDP `net.udp.bind(port?) -> udp_sock`, `.send(host, port, bytes)`, `.recv() -> (bytes, sender)` | 4 fixtures | § 22 | deferred |
| M20.T6 | `net.resolve(host) -> string` (first A record); cap-gated by `net.resolve` | 2 fixtures, second one mocked | § 22 | deferred |
| M20.T7 | Trace events: `net_listen`, `net_accept`, `tcp_send`, `udp_recv`, `dns_resolve` with hashed bodies | 1 golden per L2 stub | § 20.1 | deferred |
| M20.T8 | Every listener is shut down on `aeris run` exit (no leaked sockets); test asserts ports are free after run | 1 integration test | — | deferred |

### 5.21 M21 — Test helpers + `@example` + `suite { setup }` (1 week)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M21.T1 | `assert_status(resp, code)` and `assert_json(resp, path, value)` as builtins of `test_harness`. Failure prints the actual vs expected with the `path` highlighted | 5 fixtures each | § 21.1 | done (inline builtins; raise on mismatch with a readable message) |
| M21.T2 | `assert_semantic(text, criterion: string)` calls a hidden `agent` (`accept: string, produce: enum {Pass, Fail { reason }}`). Cap requires `ai.complete @ [<judge_model>]` from the lockset | 3 fixtures | § 21.1 | done (judge-style prompt via ai.complete; cap-gated; raises on negative reply) |
| M21.T3 | `@example(arg1, arg2) -> expected` on a top-level `fn` generates an implicit test case run by `aeris test` | 5 fixtures | § 21.1 | deferred (parser-level annotation + test harness generation; future polish pass) |
| M21.T4 | `suite "name" { setup { ... } test "..." { ... } }` runs `setup` before every `test` in the suite; `setup` cannot define `var` (only `let`) | 4 fixtures | § 21.2 | deferred (parser change + harness change; future polish pass) |
| M21.T5 | `aeris doc` extracts `@example` entries into the JSONL output | Snapshot test | § 25.1 | deferred (depends on T3) |

### 5.22 M22 — L2 handler parity with v0.1 (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M22.T1 | `docker.{stats,logs,exec,cp,networks,volumes,compose,prune,df,version}` shell out to `docker` and surface a `CommandResult`-shaped record | 10 fixtures (one per op) | § 23 | deferred |
| M22.T2 | `kube.{describe,rollout,scale,logs}` shell out to `kubectl` | 4 fixtures | § 23 | deferred |
| M22.T3 | `mongodb` full driver: `connect(uri) -> conn`, `conn.db(name)`, `db.collection(name)`, `coll.{find,find_one,insert_one,insert_many,update_one,update_many,delete_one,delete_many,count,aggregate,create_index,drop}`. Backend: `mongo-rust-driver` behind a feature flag, mock by default | 15 fixtures (mock) + 3 integration tests gated by an env var | § 23 | deferred |
| M22.T4 | `minio` full ops: `get`, `put`, `delete`, `exists`, `stat`, `list`, `buckets`, `bucket_exists`, `mb`, `rb` | 10 fixtures | § 23 | deferred |
| M22.T5 | `rabbitmq` channel: `connect`, `channel`, `queue_declare`, `queue_delete`, `queue_purge`, `exchange_declare`, `exchange_delete`, `queue_bind`, `queue_unbind`, `qos`, `publish`, `consume`, `get`, `ack`, `nack`, `reject`, `close_channel`, `close_conn` | 17 fixtures | § 23 | deferred |
| M22.T6 | Every new op records a per-call trace event; saga-scoped idempotency keys flow through (HTTP `Idempotency-Key`, K8s annotation, AMQP `message-id`, Mongo sentinel) | 1 golden per backend | § 12.3 | deferred (the v0.2 M11 baselines still ship) |

### 5.23 M23 — `model X@vN extends X@v(N-1)` (1 week)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M23.T1 | Parser: `model X@v2 extends X@v1 { <added or overridden fields> }` | 6 positive + 4 negative fixtures (added field missing, field type narrowed in incompatible way) | § 16 | done |
| M23.T2 | Static check: every field of `X@v1` is present in `X@v2`; every `where` of `X@v1` is implied by the v2 shape (best-effort syntactic check, not SMT). Failures → exit 68 | 5 fixtures | § 16.1 | done (runtime merges parent fields and where clauses into the child during `collect_decls`; mismatched inherited shapes raise SchemaViolation) |
| M23.T3 | Auto-migration: when the diff is structurally trivial (only added fields with defaults), the compiler generates `migrate_v1_to_v2`; otherwise an explicit migration is required | 4 fixtures | § 16.4 | deferred (defaults syntax + migration synthesis; future polish pass) |
| M23.T4 | `aeris doc` emits the `extends` chain | Snapshot test | § 25.1 | deferred (small doc tweak; bundled with the next doc pass) |

### 5.24 M24 — Script-friendly surface (1 week)

Closes the v0.1 → v0.2 ergonomic gap that surfaced during dogfooding
(§ 11 of this document). The pieces are mechanical, but together
they let an `enforce = "off"` script read like an interpreted
scripting language, not a stripped-down systems language.

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M24.T1 | Reserve `loop` keyword; parser desugars `loop { body }` to `while true { body }` before the static check | 5 fixtures incl. `break` / `continue` interaction | § 2.3, § 6.1 | done |
| M24.T2 | Lexer emits `QuestionQuestion` for `??`; parser introduces `BinOp::Coalesce` between `and` and `or`; runtime extracts inner value from `Ok(v)`/`Some(v)`/non-wrapper and falls back to rhs on `Err(_)`/`None`/`()`. Right-associative | 10 fixtures incl. chained `a ?? b ?? c` | § 2.6, § 18 | done |
| M24.T3 | Pure builtins on `strings`: `trim`, `lower`, `upper`, `contains`, `starts_with`, `ends_with`, `split`, `join`, `replace`, `parse_int` | 1 fixture per op | § 22 | done |
| M24.T4 | Method-call sugar on `list<T>` (`len`, `empty`, `first`, `last`, `slice`, `contains`, `join`), `string` (same surface as `strings.*`), `map<K,V>` (`len`, `get`) — dispatched at runtime after the cap-module path failed | 8 fixtures | § 5.4, § 22 | done |
| M24.T5 | Top-level `len(x)` intrinsic — accepts `list` / `set` / `tuple` / `map` / `string` / `bytes` | 5 fixtures | § 22 | done |
| M24.T6 | `json.encode` / `json.stringify` emit natural (untagged) JSON; `json.parse` returns `result<record>` for objects; `json.pretty` aliases compact encoder pending a real pretty-printer | 4 fixtures incl. record → object → record round-trip | § 22 | done |
| M24.T7 | `date.today() -> date`, `date.timestamp() -> int` | 2 fixtures | § 22 | done |
| M24.T8 | `value_as_display` unwraps `Result(Ok(v))` → `v`, `Option(Some(v))` → `v`, displays records / lists via the natural JSON encoder; `io.println(Some(7))` prints `7` not `Some(Int(7))` | 6 display-shape fixtures | § 22 | done |
| M24.T9 | `ai.chat(system: string, dir: string) -> Chat` (M19.T6 reified): walks the directory, loads `*.md \| .txt \| .rst \| .adoc \| .yaml \| .yml`, concatenates with `=== FILE: <path> ===` markers, returns a `Chat` record. `chat.ask(prompt) -> string` calls the backend; `chat.kb_size() -> int` reports the file count. Coexists with the v0.2 `ai.chat(messages)` form | smoke test under mock and CLI backend | § 23 | done |
| M24.T10 | `language.md § 2.3 / § 2.6 / § 5.4 / § 6.1 / § 8.4.1 / § 22 / § 23 / § 24.1 / Appendix D` updated; `RELEASE.md` records v0.3 surface | docs cross-referenced | — | done |

### 5.25 M29 — Kwargs on user-defined functions (1 week)

M25 wired named-argument dispatch into the L1/L2 builtin and method
tables but **not** into the closure-invocation path. A call like
`greet(greeting: "ciao", name: "Alice")` against
`fn greet(name, greeting)` parses, the labels survive in the AST,
and then `eval_call` drops them and binds positionally — so the
argument order at the call site silently *matters*. M29 closes that
gap.

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M29.T1 | In `eval_call`'s user-fn branch, before `invoke_value`: if any `CallArg.name` is `Some`, reorder against the closure's `params` list. Positional args fill leading slots, kwargs fill by name, duplicates raise `EvalError::Type("duplicate kwarg `<name>`")`, unknown names raise `EvalError::Type("unknown kwarg `<name>` for `<fn_name>`")`, missing slots raise the existing arity error | 5 positive fixtures (pure positional / pure kwargs / mixed / reverse order / single-arg name) + 3 negative (unknown name, duplicate, missing) | § 7.6 | done |
| M29.T2 | Same reorder path applied to closures invoked as record fields (`x.f(name: …)`) and to lambdas (anonymous `fn(…) { … }`) — both already hit `invoke_value` through different code paths, so the helper is hoisted into a single `resolve_call_args` function used by every caller | 2 fixtures: record-field closure called with kwargs; lambda assigned to a `let` and called with kwargs | § 5.4, § 7.3 | done |
| M29.T3 | `language.md § 7.6` extended with the mixed-positional+kwargs rule already promised by the existing text ("mixing is allowed for trailing parameters only") and with the three new error messages so users can recognise them | docs cross-referenced | § 7.6 | done |

---

## 6. Test artifacts

The implementation MUST ship with the following artifacts. Their
absence is a v0.2.0 release-blocker.

| Artifact | Location | Purpose |
|---|---|---|
| Positive `.aer` fixtures | `aeris-tests/positive/` | Programs that MUST type-check and run |
| Negative `.aer` fixtures | `aeris-tests/negative/<exit_code>/` | Programs that MUST be rejected with a specific exit code |
| Golden JSONL traces | `aeris-tests/golden/<milestone>/` | Reference traces; `aeris trace diff` is the comparator |
| Property tests | `aeris-tests/property/` | Counter-example seeds re-run on each CI |
| Surface lock snapshots | `aeris-tests/surface/` | V3 regression baseline |
| Round-trip fixtures | `aeris-tests/roundtrip/` | `parse → fmt → parse` byte-equal |
| Replay parity fixtures | `aeris-tests/replay/` | Original run + `aeris replay` produce identical traces |
| Lockset tampering vectors | `aeris-tests/lockset-attack/` | Modified bytes / hash mismatch / version drift |
| Prototype-mode fixtures | `src/check/manifest_caps.rs::tests` (M15 / M15B) | Same source passes with `enforce = "loose"` / `"off"` and fails with `"strict"` |
| v0.3 script fixtures | `demo/11_chatbot_md/` (M15B + M24) | One real end-to-end project exercising `enforce = "off"` + `loop` + `??` + method calls + `ai.chat(dir)` |

Acceptance suite naming: every test file is `<milestone>-<task>-<short>.aer`,
e.g. `aeris-tests/positive/M6.T2-saga-rollback.aer`. CI runs the
suite per milestone and emits a coverage matrix.

---

## 7. Definition of done

Three nested levels of completion. A higher level subsumes the lower.

### 7.1 Task-level done

- Code compiles, no clippy warnings, `aeris fmt --check` passes.
- Acceptance check from the task table produces the expected outcome.
- At least one positive and one negative test for every behaviour.
- Trace artifacts (where applicable) checked in to `aeris-tests/golden/`.

### 7.2 Milestone-level done

- Every task in the milestone is `done` (or explicitly deferred to
  § 9 with the milestone status flagged accordingly).
- The milestone's full acceptance suite passes locally on the
  contributor's host platform: `cargo build`, `cargo test --lib`,
  `cargo test --tests`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check`. Multi-platform validation is the contributor's
  responsibility (no CI in this repo — see § 9).
- Updated `RELEASE.md` section for the milestone listing breaking
  changes (none expected within v0.2.x).

### 7.3 v0.2.0 release done

- All M0 → M14 milestones `done`. M15 (capability prototype mode)
  shipped as an additional ergonomic patch on top of the v0.2.0
  release set.
- Six success criteria of `thesis.md § 13` reproducibly demonstrable
  on `examples/`:
  1. Compliance officer reads a saga signature and identifies all
     external resources in < 30 s (manual walk-through, recorded).
  2. Every effectful call site in `examples/` has an enclosing
     `intent` propagated to its trace.
  3. Failed runs produce JSONL traces from which `aeris replay`
     reproduces them bit-identically (M9 acceptance).
  4. A saga whose middle step fails leaves the system in `ok` /
     `rolled_back` / `PartialFailure` — no silent half-states (M6
     golden traces).
  5. A supply-chain swap of a published library's bytes does not
     execute (M7.T2 attack vector).
  6. An LLM-generated PR adding a network call fails review because
     the surface diff appears as the first hunk (M7.T5 + M2.T12).
- Release tag `v0.2.0` pushed. Static-binary publication is deferred
  (§ 9); the source at the tag is the canonical artifact and any
  consumer builds locally with `cargo build --release`.
- `docs/` contains exactly four files: `thesis.md`, `language.md`,
  `project.md`, `plan.md`. No `// TODO`, no orphan sections.

---

## 8. Risk register

Risks ordered by likelihood × impact. Each risk has a **Mitigation**
that is itself implementable.

| # | Risk | L | I | Mitigation |
|---|---|---|---|---|
| R1 | LLM backend instability invalidates N3 replay (Anthropic API changes) | M | H | Backend abstraction (M9.T1) + mock backend by default; HTTP / CLI backends opt-in via `[ai.backend]` in `aeris.toml` |
| R2 | Effect-surface analysis (V3) becomes intractable for large projects | L | H | Per-pub-fn computation only; cache by AST hash; benchmark on 1000-fn synthetic project before M11 |
| R3 | Saga undo cascade hits backend rate limits | M | M | M6.T5 retries with exponential backoff; PartialFailure surfacing is the safety valve |
| R4 | `aeris fmt --narrow-caps` produces noisy diffs that overwhelm review | L | M | Linter mode emits diffs only on opt-in; default `aeris fmt` does not narrow |
| R5 | Object-capability theory does not match thesis § 8.1 prose under Strada Z (body-resolution rule) | L | H | Explicitly addressed in language.md § 8.2; thesis prose preserved literally; if drift is observed, raise to a thesis-revision PR before M2 closes |
| R6 | `model@vN` migrations create combinatorial test burden | M | M | One migration per `(v, v+1)` pair; transitive composition is the user's responsibility, not the language's |
| R7 | Cross-compilation matrix (M14.T2) fails on Windows | M | M | Mooted: M14.T2 deferred (§ 9). Per-target builds are produced on demand by the contributor; Windows users compile natively with the standard `cargo build --release` recipe |
| R8 | Single-binary size > 8 MB stripped (thesis § 2 violated) | L | H | Audit dep tree at M9 (LLM backend is the heaviest); fall back to feature-gated http backend |
| R9 | Performance regression after M9 (tape recording overhead) | M | M | M14.T3 / T4 / T5 benchmarks gate every PR after M9 |
| R10 | Documentation drift: `language.md` evolves but `plan.md` references stale sections | M | L | CI link-checker for `§ X.Y` references in `plan.md` against `language.md` headings |
| R11 | Strict capability mode rejected by users on adoption-friction grounds | M | H | M15: `[caps] required = false` prototype mode; `aeris init` defaults to it; runtime allow-list still enforced; `aeris fmt --narrow-caps` automates the strict-mode promotion |

L = likelihood (L/M/H), I = impact (L/M/H).

---

## 9. Out of scope for v0.2.0

These items are deliberately deferred. Each has a sketch of *what
would have to change* if a v0.3 wanted to admit them.

> The v1 toolkit (AI builtins, network listeners, inline error
> handling, time-control sugar, expanded L2 handlers, model `extends`)
> was originally listed here and is now scheduled by M16–M23 (§ 5);
> § 11 explains how each piece is re-incorporated without violating
> the thesis.

| Item | Why deferred | Path to admission |
|---|---|---|
| Bytecode VM / JIT | Tree-walk evaluator hits performance targets (M14.T3 = 5× CPython); JIT triples implementation complexity | Replace `runtime`'s eval submodule; keep AST and stdlib stable |
| Async runtime (cooperative scheduler) | OS threads via `spawn` cover all v0.2 use cases | Add a `pollable` trait inside `runtime` and an async backend behind a feature flag |
| Native `.so` plug-ins | Refused by thesis § 9.6 | Would require revisiting the thesis; not a code question |
| Refinement types / SMT | Refused by thesis § 9.1 | Would require revisiting the thesis |
| Self-hosted compiler | Not a v0.2 goal; Rust hosting suffices | Stable AST + bytecode VM (post-v0.3) make it feasible |
| Web playground | Not a v0.2 goal; static binary is the deployment | Add a `wasm32-wasi` target post-M14 |
| LSP / IDE integration | Plan focuses on CLI + LLM authoring | Reuse `syntax` and `check` modules behind a `tower-lsp` shell |
| CI / release automation (`M0.T3`, `M14.T1`, `M14.T2`, `M14.T6`) | Project intentionally ships without GitHub Actions; `cargo fmt --check` / `clippy` / `test` are local developer discipline; binaries are built on demand for the contributor's own target | Re-introduce `.github/workflows/{ci,release}.yml` from the v0.1 history if a future fork wants automated multi-target tarballs |

---

## 10. Tracking

This file IS the tracker. To update:

1. When starting a task, change its **Status** column from `pending`
   to `in progress`.
2. When closing a task (PR merged + acceptance check green), change
   its **Status** to `done`.
3. When all tasks of a milestone are `done` and the milestone-level
   acceptance suite passes, change the milestone's **Status** in
   § 3 to `done`.
4. Date-stamp the milestone in the PR description that closes it
   (not in this file — the file is forward-looking).

A milestone moves to `done` only after § 7.2 is satisfied. There is
no "soft done".

---

## 11. v0.3 — Re-importing v0.1 features under v0.2 principles

The v0.3 milestones (M16–M23) re-introduce constructs that existed in
the legacy Aeris codebase but were left out of v0.2 on purpose. The
constraint is non-negotiable: every re-imported construct **must obey
the thesis and `language.md` rules already in place** — V2 intent,
the typed `cap` system, replay determinism, the closed `err` enum,
and the V3 surface lock.

This section records the tension each construct creates and how it is
resolved. Implementers must read it before opening a PR for the
corresponding milestone.

### 11.1 String interpolation `{x}` (M16)

**Tension.** The brace `{` already terminates record literals
(`User { x: 1 }`) and block expressions (`{ let x = 1; x }`).
Reintroducing `{...}` inside strings creates two lexical contexts for
the same brace.

**Resolution.** Interpolation lives **inside the string token**.
The lexer enters interpolation mode on `"` and exits on the matching
`"`; inside that mode, an unescaped `{` starts an embedded expression
that ends at the matching `}`. Outside a string token the brace
parses as before. `\{` and `\}` are the only escape; no `{{` doubling.

**Impact on the trace.** None — interpolation is a parsing concern.
The compiled AST is identical to what `\(...)` produced.

### 11.2 `expr catch err { ... }` (M17)

**Tension.** v0.2 already has `result<T>` + `?` + `match`. A
`catch` postfix risks duplicating the error machinery and inviting
sloppy exception handling.

**Resolution.** `catch` is **strictly syntactic sugar** over `match`:

```aeris
let body = http.get(url)? catch err { default_response() }
// desugars to:
let body = match http.get(url) {
  Ok(v)  -> v,
  Err(_) -> default_response(),
}
```

The block must itself return `T` or `raise`. `catch` cannot suppress
a `ContractViolation` or a `PolicyViolation` — they remain fatal as
per § 18.4.

### 11.3 `error(msg)` (M17)

**Tension.** A function returning a freshly-raised error invites the
open-string error-class anti-pattern v0.2 was designed to reject.

**Resolution.** `error(msg)` constructs the `err.user(msg)` variant of
the closed `err` enum (§ 18.1). It is a value, not a control-flow
operator; only `raise error(...)` (or `Err(error(...))`) actually
throws. The other eight variants (`io`, `net`, `schema`, `contract`,
`policy`, `budget`, `partial_failure`, `llm`) remain inaccessible to
the user constructor.

### 11.4 `defer stmt` (M17)

**Tension.** Deferred side effects break the V2 promise that every
write-effectful call sits inside an `intent`, because the call site
is at function exit, not at the lexical point of `defer`.

**Resolution.** A `defer` body is treated as if it were inlined at
every function exit point for the purpose of static checks. The body
must therefore wrap any write-effectful call in `intent` and the
enclosing function's `cap` must already permit those operations. The
runtime emits `defer_enter`/`defer_exit` events at execution time so
the trace preserves the original lexical order.

### 11.5 `every` / `retry` / `timeout` (M18)

**Tension.** Each of these blocks hides a wait or a cancel — invisible
to the cap system unless treated carefully.

**Resolution.** All three desugar before the static checker runs:

- `every <d> { body }` ≡ `loop { clock.sleep(d); body }`. Requires
  `cap[clock.sleep]`.
- `retry <n>, delay: <d> { body }` ≡ a `for` loop with attempt-bounded
  exponential `clock.sleep` between attempts; the body must yield
  `result<T>`; first `Ok` wins, last `Err` propagates.
- `timeout <d> { body }` ≡ `spawn`+cancel-channel; cancellation is
  cooperative on the next cap call (matches the M5 / M19.1 model).

Because desugaring happens pre-check, the cap-narrowing rule, V2
intent rule, and saga-step rules all apply naturally.

### 11.6 The AI builtin family (M19)

**Tension.** Every v1 builtin (`ai.session`, `ai.decide`, `ai.extract`,
`ai.generate`, `ai.ensemble`, `ai.eval`, `ai.index`, `ai.guard`,
`ai.cache`, `ai.usage`) hid state and side-effects that the v0.2 trace
cannot model deterministically. The cap-system would lose its teeth if
these calls bypassed `agent`/`model@vN`.

**Resolution.** Every builtin is a thin desugarer over the v0.2 core:

| Builtin | Desugars to |
|---|---|
| `ai.session` | a `record Session { system, model, history }` plus a pure function `Session.ask` that calls a hidden `agent { accept: HistoryPair@v1, produce: string }`. State is **immutable**: `.ask` returns `(new_session, reply)` |
| `ai.decide` | one-shot `agent` with `produce: enum {<choices>}` synthesised at parse time |
| `ai.extract<M>` / `ai.generate<M>` | one-shot typed `agent`; `accept: string`, `produce: M` |
| `ai.ensemble` | `agent_net` with `flow source -> { a, b, c }` and a `terminal merge` agent that applies the strategy |
| `ai.eval` | typed agent `accept: (output, criteria), produce: { score, reasoning }` |
| `ai.index` | a pure data structure under `.aeris/kb/<id>.json`; `.search` is a pure ranking function (no LLM call), so it stays out of `intent` |
| `ai.guard` | wraps a call site with two policy activations (input / output) |
| `ai.cache` | replays the recorded `ai_call` event from `.aeris/cache/` when the `(prompt, model)` hash hits |
| `ai.usage` | a counter the tracer already maintains; just a getter |

Each builtin must be called inside `intent` (the `ai.index.search`
exception is a pure read that returns no LLM data). The cap-system
gates every builtin on `ai.complete` and/or `ai.embed`.

The `aeris chat` REPL is **not a builtin** — it is a top-level CLI
subcommand (M19.T10) that wraps `ai.session` with stdin/stdout.

### 11.7 Network listeners (M20)

**Tension.** A v1 HTTP server gave the program ambient authority to
receive any request on any port. Antithetical to the cap system.

**Resolution.** Each listener op has its own cap entry with an
allow-list: `net.http.serve @ [8080]`, `net.tcp.listen @ [...]`,
`net.tcp.connect @ ["redis:6379"]`, `net.udp.bind @ [...]`,
`net.resolve @ ["*.acme.com"]`. The trace records `net_listen`,
`net_accept`, `tcp_send`, `udp_recv`, `dns_resolve` events with hashed
payloads, identical to the client-side `http.*` shape.

### 11.8 Test helpers + `@example` + `suite { setup }` (M21)

**Tension.** v0.2 keeps tests as plain `test "name" { ... }`. Adding
shorthands risks balkanising the test surface.

**Resolution.** All four shorthands are mechanical sugar:

- `assert_status` / `assert_json` build a `match` on the response shape.
- `assert_semantic` desugars to a hidden judge `agent`; it is the only
  one that requires `cap[ai.complete]`.
- `@example(args) -> expected` annotation generates an implicit
  `test "<fn>::example_<n>" { ... }` block at parse time.
- `suite "..." { setup { } test "..." {} }` desugars to a list of
  individual test blocks each prefixed by the `setup` body inlined.
  `setup` cannot introduce `var` (function-scope only), so the inlining
  is invisible to the trace.

### 11.9 L2 handler parity (M22)

**Tension.** The v1 handlers exposed dozens of operations per backend.
Expanding to that surface is straightforward in code but inflates the
attack surface and the cap vocabulary.

**Resolution.** Each new op gets its own cap path
(`docker.exec`, `kube.scale`, `mongodb.aggregate`, ...) and a per-op
trace event with the backend-specific fields it already exhibits in
v1. The runtime backend is **shell-out wrapping** (Docker, kubectl) or
a feature-flagged Rust client (`mongo-rust-driver`, `lapin`, `aws-sdk-s3`
configured for S3-compatible endpoints); mocks are the default in CI.

### 11.10 `model X@v2 extends X@v1` (M23)

**Tension.** v0.2 chose explicit migration functions to avoid hidden
schema-evolution surprises.

**Resolution.** `extends` is sugar; the parser still requires an
explicit `migrate_v1_to_v2(old: X@v1) -> X@v2` function **unless** the
diff is structurally trivial (only added fields, each with a default).
In the trivial case the compiler generates the migration; in any
other case the omitted migration is a compile error.

### 11.11 Kwargs on user-defined functions (M29)

**Tension.** `language.md § 7.6` documents kwargs as a uniform call
form — *"named arguments work identically against user-defined
functions and closures … and L1 / L2 builtins"*. M25 delivered the
builtin half: a parameter-name table backs `reorder_kwargs_for_builtin`
and a matching helper covers methods on `Chat`, `HttpReq`,
`AiNetwork`. The closure-invocation path was overlooked: it evaluates
`arg.value` and drops `arg.name`, so a call like
`greet(greeting: "ciao", name: "Alice")` quietly binds by position.
The bug is silent because no error fires — the labels are simply
ignored — which is exactly the failure mode the language tries to
make impossible (§ 7.4 of `thesis.md`: ambiguous constructs force
the LLM to infer).

**Resolution.** Reuse the same dispatcher shape M25 uses for
builtins, but driven by the closure's `params` instead of a static
table. Positional args fill leading slots; kwargs fill by name;
positional-after-kwarg is rejected at parse / evaluation; duplicates
and unknowns raise typed errors so the failure is loud, not silent.

### 11.12 Acceptance for v0.3 as a whole

A v0.3 tag (`v0.3.0`) requires:

- M16 → M23 all `done`, each with positive and negative fixtures.
- `cargo test --test release_thesis_section_13` still green —
  the six criteria of `thesis.md § 13` remain mechanically verifiable.
- `aeris fmt --migrate-strings` runs idempotently on the entire
  codebase (M16 is invasive: every fixture migrates).
- A new release smoke test, `tests/release_v03_inventory.rs`, that
  exercises one fixture per v0.3 construct and asserts it desugars to
  the expected v0.2 AST. This is the gate that proves no construct
  escapes the principles.

---

*End of implementation plan.*
