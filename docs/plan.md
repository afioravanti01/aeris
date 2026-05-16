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
│   ├── lockset/                          # lockset.toml, blake3, surface.lock, main cap synthesis
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
| `lockset` | lockset.toml parsing, dep resolution, blake3 hashing, surface.lock writer/reader, main cap synthesis |
| `test_harness` | parallel `aeris test` runner, property generators, golden-trace differ |

---

## 3. Milestone overview

| M | Title | Output | Weeks | Depends on | Status |
|---|---|---|---|---|---|
| M0 | Project bootstrap | Workspace, CI, `aeris version` runs | 1 | — | done |
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
| M14 | Performance + packaging + v0.2.0 release | Static binary < 8 MB stripped; cross-compile; tag | 3 | M11, M12, M13 | done |
| M15 | Capability prototype mode | `[caps] required` flag in lockset; suppresses E65 in prototype mode; `aeris init` defaults to `false` | 1 | M2, M7 | done |

**Total**: 48 engineering-weeks. Critical path M0 → M1 → M2 → M3 → M4 → M5 → M6 → M9 → M10 → M14 = 30 weeks.

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
| M0.T3 | CI pipeline (GitHub Actions): fmt, clippy, build, test | PR fails on clippy warnings | — | done |
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
| M2.T6 | Capability checker: allow-list intersection with `lockset.toml [caps]` | A signature requesting `http.post @ ["evil.com"]` outside lockset rejected with code 71 | § 8.3.2 | done |
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
| M4.T3 | `main`'s synthesised cap from `lockset.toml [caps]` (without M7's full lockset — minimal stub) | `aeris run` prints effective cap shape on stderr | § 8.4 | done |
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
| M7.T1 | `lockset.toml` parser (using `toml` crate); semantic validation | 20 lockset fixtures; malformed → exit 69 | § 24.1 | done |
| M7.T2 | Local path dep resolution + blake3 hashing of resolved bytes | Hash mismatch → exit 69; `aeris lock` recomputes | § 24.4 | done |
| M7.T3 | GitHub tarball dep resolution + cache at `.aeris/ext/<host>__<repo>/<version>/` | Network test (mocked) succeeds; second run hits cache | § 24.2 | done |
| M7.T4 | `main`'s synthesised cap composes from `[caps]` ceiling | Effective signature printed on `aeris run` stderr matches lockset | § 8.4 | done |
| M7.T5 | V3 `aeris lock surface`: per-`pub`-fn effect set + allow-list emitted to `.aeris/surface.lock` | Snapshot test against 5-module project | § 8.6 | done |
| M7.T6 | `surface_hash` for deps recorded in `lockset.toml [deps].<alias>` | A dep upgrade that broadens surface forces a lockfile diff | § 24.3 | done |
| M7.T7 | CI mode: `aeris lock --check` rejects PR with stale lockset | Exit 69 on staleness | § 24.4 | done |

### 5.8 M8 — Models + Policies (3 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M8.T1 | `model@vN` validation on construction with all `where` clauses | 20 fixtures; field violation → `SchemaViolation` | § 16.2 | done |
| M8.T2 | `model@vN` validation on `json.decode` and on HTTP body ingress | 10 fixtures crossing trust boundary | § 16.2 | done |
| M8.T3 | Record-level `where:` (multi-field invariants) | 5 fixtures with cross-field constraints | § 16.3 | done |
| M8.T4 | `policy` runtime: `match`, `deny`, `require`, `limit`, `audit`, `when` | One fixture per clause, all six | § 15 | done |
| M8.T5 | Policy activation: module-import / `#[policy(name)]` attribute / `lockset.toml [policies]` | 3 activation modes tested | § 15.3 | done |
| M8.T6 | Policy drift trace event when replay-vs-live outcome differs | `policy_drift` event emitted on synthetic divergence | § 15.4 | done |
| M8.T7 | `PolicyViolation` exit (not catchable by `?`) | Test confirms behaviour | § 18.4 | done |

### 5.9 M9 — L2 `ai` + LLM tape + Replay (4 weeks)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M9.T1 | `ai` cap handler with pluggable backend selected by `lockset.toml [ai.backend]` | HTTP backend hits Anthropic API (or mock); CLI backend spawns subprocess; mock backend returns canned responses | § 23 | done |
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
| M14.T1 | Static binary build (`musl` on Linux, native on macOS/Windows) | `aeris` binary < 8 MB stripped on Linux x86_64 | thesis § 2 | done |
| M14.T2 | Cross-compile matrix: Linux x86_64, Linux arm64, macOS arm64, macOS x86_64, Windows x86_64 | CI produces all 5 binaries | thesis § 2 | done |
| M14.T3 | Performance: pure-fn evaluator within 5× CPython on a representative fixture | Benchmark suite checked in | — | done |
| M14.T4 | Trace JSONL throughput: ≥ 100 k events/sec on a representative SSD | Benchmark | § 20 | done |
| M14.T5 | Cold-start time of `aeris run` < 50 ms (parse + check + start eval) | Benchmark | — | done |
| M14.T6 | Release packaging: tarballs + checksums + GPG-signed | Release artifacts attached to `v0.2.0` tag | — | done |
| M14.T7 | `aeris init` template: minimal viable project, hello-world saga, hello-world agent | Template renders into `examples/` | § 25.1, App. A–C | done |
| M14.T8 | Release notes referencing every milestone's golden traces | `RELEASE.md` checked in | — | done |

### 5.15 M15 — Capability prototype mode (1 week, post-v0.2.0)

| ID | Task | Acceptance | Refs | Status |
|---|---|---|---|---|
| M15.T1 | Add `required: bool` to `[caps]` parser; default `true` | 5 lockset fixtures with explicit `required` | § 8.4.1, § 24.1 | done |
| M15.T2 | `check::check_module_with_lockset` honours `required = false`: suppress `NoCapInScope` (E65) for fns without `cap` parameter; fns *with* `cap` still checked normally | 9 fixtures: same code passes with `required = false`, fails with `required = true` | § 8.4.1 | done |
| M15.T3 | `aeris init` template emits `required = false` by default with explanatory comment | `src/templates/lockset.toml` | § 25.1 | done |
| M15.T4 | Examples migration: `examples/saga` and `examples/agent_net` opt into `required = true`; `examples/hello` keeps prototype mode | `examples_check.rs` integration test still green | App. A–C | done |
| M15.T5 | Documentation: `RELEASE.md` notes the prototype/strict workflow; `language.md § 8.4.1` updated | RELEASE.md + language.md updated | § 8.4.1 | done |

The orthogonal rules (E66 intent, E67 saga undo, E71 lockset
ceiling, E65 `cap[*]` ban) remain active in both modes — they
concern program structure, not authority distribution.

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
| Prototype-mode fixtures | `src/check/lockset_caps.rs::tests` (M15) | Same source passes with `required = false` and fails with `required = true` |

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

- Every task in the milestone is `done`.
- The milestone's full acceptance suite passes on CI on all five
  target platforms (M14.T2 matrix; for milestones before M14, on at
  least Linux x86_64 + macOS arm64).
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
- Release tag `v0.2.0` pushed; static binaries published.
- `docs/` contains exactly four files: `thesis.md`, `language.md`,
  `project.md`, `plan.md`. No `// TODO`, no orphan sections.

---

## 8. Risk register

Risks ordered by likelihood × impact. Each risk has a **Mitigation**
that is itself implementable.

| # | Risk | L | I | Mitigation |
|---|---|---|---|---|
| R1 | LLM backend instability invalidates N3 replay (Anthropic API changes) | M | H | Backend abstraction (M9.T1) + mock backend by default in CI; HTTP backend behind a feature flag |
| R2 | Effect-surface analysis (V3) becomes intractable for large projects | L | H | Per-pub-fn computation only; cache by AST hash; benchmark on 1000-fn synthetic project before M11 |
| R3 | Saga undo cascade hits backend rate limits | M | M | M6.T5 retries with exponential backoff; PartialFailure surfacing is the safety valve |
| R4 | `aeris fmt --narrow-caps` produces noisy diffs that overwhelm review | L | M | Linter mode emits diffs only on opt-in; default `aeris fmt` does not narrow |
| R5 | Object-capability theory does not match thesis § 8.1 prose under Strada Z (body-resolution rule) | L | H | Explicitly addressed in language.md § 8.2; thesis prose preserved literally; if drift is observed, raise to a thesis-revision PR before M2 closes |
| R6 | `model@vN` migrations create combinatorial test burden | M | M | One migration per `(v, v+1)` pair; transitive composition is the user's responsibility, not the language's |
| R7 | Cross-compilation matrix (M14.T2) fails on Windows | M | M | Plan a Windows VM in CI from M0; flag musl-only deps early |
| R8 | Single-binary size > 8 MB stripped (thesis § 2 violated) | L | H | Audit dep tree at M9 (LLM backend is the heaviest); fall back to feature-gated http backend |
| R9 | Performance regression after M9 (tape recording overhead) | M | M | M14.T3 / T4 / T5 benchmarks gate every PR after M9 |
| R10 | Documentation drift: `language.md` evolves but `plan.md` references stale sections | M | L | CI link-checker for `§ X.Y` references in `plan.md` against `language.md` headings |
| R11 | Strict capability mode rejected by users on adoption-friction grounds | M | H | M15: `[caps] required = false` prototype mode; `aeris init` defaults to it; runtime allow-list still enforced; `aeris fmt --narrow-caps` automates the strict-mode promotion |

L = likelihood (L/M/H), I = impact (L/M/H).

---

## 9. Out of scope for v0.2.0

These items are deliberately deferred. Each has a sketch of *what
would have to change* if a v0.3 wanted to admit them.

| Item | Why deferred | Path to admission |
|---|---|---|
| Bytecode VM / JIT | Tree-walk evaluator hits performance targets (M14.T3 = 5× CPython); JIT triples implementation complexity | Replace `runtime`'s eval submodule; keep AST and stdlib stable |
| Async runtime (cooperative scheduler) | OS threads via `spawn` cover all v0.2 use cases | Add a `pollable` trait inside `runtime` and an async backend behind a feature flag |
| Native `.so` plug-ins | Refused by thesis § 9.6 | Would require revisiting the thesis; not a code question |
| Refinement types / SMT | Refused by thesis § 9.1 | Would require revisiting the thesis |
| Self-hosted compiler | Not a v0.2 goal; Rust hosting suffices | Stable AST + bytecode VM (post-v0.3) make it feasible |
| Web playground | Not a v0.2 goal; static binary is the deployment | Add a `wasm32-wasi` target post-M14 |
| LSP / IDE integration | Plan focuses on CLI + LLM authoring | Reuse `syntax` and `check` modules behind a `tower-lsp` shell |

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

*End of implementation plan.*
