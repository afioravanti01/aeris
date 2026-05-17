# Aeris v0.3 — Cheatsheet

> Tabular reference of every construct and API in the language.
> Source of truth for the details: [`language.md`](language.md).
> Section numbers in the **§** column point into `language.md`.

---

## 1. Lexical

### 1.1 Reserved keywords (final, frozen)

| Category | Keywords |
|---|---|
| Declarations | `fn`, `record`, `enum`, `model`, `type`, `const`, `let`, `var`, `pub`, `use`, `from`, `as`, `test`, `property` |
| Capability / intent / policy | `cap`, `intent`, `policy`, `deny`, `require`, `limit`, `audit`, `when`, `match` (structural key) |
| Saga / agent | `saga`, `step`, `do`, `undo`, `agent`, `agent_net`, `flow`, `until` |
| Control flow | `if`, `else`, `match`, `for`, `in`, `while`, `loop`, `break`, `continue`, `return`, `raise` |
| Errors / time (v0.3) | `catch`, `defer`, `every`, `retry`, `timeout` |
| Types / generics | `is`, `await`, `spawn`, `with`, `where`, `extends` (v0.3) |
| Booleans / logic | `true`, `false`, `and`, `or`, `not` |
| Contracts / patterns | `requires`, `ensures`, `property` |

**§ 2.3.** No soft keywords. Identifier = `[A-Za-z_][A-Za-z0-9_]*`,
case-sensitive, no Unicode. `snake_case` for values/functions/modules,
`PascalCase` for types/agents/sagas, `SCREAMING_SNAKE` for constants.

### 1.2 Literals

| Form | Example | § |
|---|---|---|
| Integer | `42`, `42_000`, `0xff`, `0b1010` | 2.4 |
| Float | `3.14`, `1.5e-3` | 2.4 |
| Boolean | `true`, `false` | 2.4 |
| String | `"hello"`, `"with {name}"`, `"x = {f(g(1,2))}"` | 2.4 / 5.3 |
| Multi-line string | `"""..."""` | 2.4 |
| Raw bytes | `b"raw"`, `b"\xff\x00"` | 2.4 |
| Char | `'\n'` | 2.4 |
| List / map / tuple | `[1,2,3]`, `{a:1, b:2}`, `("ok", 42)` | 2.4 |
| Date | `2026-05-07` | 2.4 |
| Timestamp | `2026-05-07T08:30:00Z` | 2.4 |
| Duration | `3s`, `500ms`, `2h`, `7d` | 2.4 |
| Interpolation | `"x = {expr}"` — `\{` / `\}` for literal braces | 2.4 / 5.3 |
| Comments | `// line`, `/* block */`, `/// doc` | 2.5 |

### 1.3 Operators (precedence high → low)

| Level | Operators | Notes |
|---|---|---|
| Postfix | `.` (field), `?` (try) | `?` propagates `Err` |
| Unary | `-`, `not` |  |
| Multiplicative | `*`, `/`, `%` |  |
| Additive | `+`, `-` |  |
| Shift | `<<`, `>>` |  |
| Bitwise | `&`, `|`, `^` |  |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=` |  |
| Type | `is`, `as` |  |
| Logical AND | `and` |  |
| Null-coalesce | `??` (v0.3) | `Ok/Some/value → v`, `Err/None/() → rhs` |
| Logical OR | `or` |  |
| Range | `..`, `..=` |  |
| Assignment | `=`, `+=`, `-=`, `*=`, `/=`, `%=` |  |

No ternary, no comma operator, no overloading.
**§ 2.6.**

---

## 2. Types

### 2.1 Primitives

| Type | Range / shape | § |
|---|---|---|
| `bool` | `true` / `false` | 4.1 |
| `int` | platform-sized signed (≥ 64 bits) | 4.1 |
| `i8` `i16` `i32` `i64` | fixed signed | 4.1 |
| `u8` `u16` `u32` `u64` | fixed unsigned | 4.1 |
| `f32` `f64` | IEEE 754 | 4.1 |
| `decimal` | arbitrary-precision fixed-point (12 fractional digits default) | 4.1 |
| `string` | UTF-8 | 4.1 |
| `bytes` | immutable byte sequence | 4.1 |
| `char` | one Unicode scalar value | 4.1 |
| `uuid` | 128-bit, RFC 9562 | 4.1 |
| `date` | civil date, no time zone | 4.1 |
| `timestamp` | UTC instant, ms precision | 4.1 |
| `duration` | signed 64-bit nanoseconds | 4.1 |
| `unit` | the empty tuple `()` | 4.1 |

Numeric conversion: explicit only (`x as i64`). No implicit widening.

### 2.2 Collections and wrappers

| Type | Notes | § |
|---|---|---|
| `list<T>` | growable, ordered | 4.2 |
| `set<T>` | hash; `T` must be hashable | 4.2 |
| `map<K, V>` | hash; `K` must be hashable | 4.2 |
| `tuple<T1, T2, ...>` | fixed arity | 4.2 |
| `option<T>` | `Some(T)` / `None` | 4.2 |
| `result<T>` | `Ok(T)` / `Err(err)` — `err` is fixed (§ 18.1) | 4.2 |
| `channel<T>` | bounded MPMC | 19.2 |
| `range<T>` | `a..b`, `a..=b` | 6.2 |
| `handle<T>` | result of `spawn { … }` | 19.1 |

### 2.3 User-defined composite types

| Form | Example | § |
|---|---|---|
| Record | `record User { id: uuid, name: string, age: int where age >= 0 }` | 4.3 |
| Structural update | `let v = User { ..u, age: 37 }` | 4.3 |
| Enum (sum) | `enum Status { Pending, Active(since: timestamp), Banned { reason: string } }` | 4.4 |
| Versioned model | `model Invoice@v1 { id: uuid, amount: decimal where amount > 0 }` | 4.5 |
| Model record-level invariant | `where: status == Cancelled implies total == 0` | 16.3 |
| Model `extends` (v0.3) | `model X@v2 extends X@v1 { extra: string }` — parent fields/`where:` inherited | 16.5 |
| Type alias | `type Email = string` (pure rename, no validation) | 4.6 |
| Generics | `fn first<T>(xs: list<T>) -> option<T>` | 4.6 |

`model@vN` is validated at: construction, `json.decode<…>`, agent
boundary, HTTP ingress. Migration = explicit function
`fn migrate_v1_to_v2(old: X@v1) -> X@v2`.

---

## 3. Bindings and module-level declarations

| Form | Meaning | § |
|---|---|---|
| `let x = e` | immutable, block-scoped | 5.1 |
| `let x: T = e` | with type annotation | 5.1 |
| `var y = e` | mutable, **function-scope only** | 5.1 |
| `const PI = 3.14159` | module-level, constant-folded | 5.1 |
| `let x = x.trim()` | shadowing in a nested scope (idiomatic) | 5.1 |
| Top-level statements (v0.3) | `let` and calls outside any `fn`; run before `main`, or as the program body when `main` is absent | 3.4 |

Module-level `var` does not exist — only `const` and immutable `let`.

---

## 4. Functions and closures

| Construct | Form | § |
|---|---|---|
| Pure function | `fn add(a: int, b: int) -> int { a + b }` | 7.1 / 7.2 |
| Function with `cap` | `fn settle(items, cap: cap[…]) -> result<unit> { … }` | 7.1 |
| Contracts | `fn pay(...) requires: amount > 0 ensures: result.ok { … }` | 9.1 |
| Lambda | `let inc = fn(x: int) -> int { x + 1 }` | 7.3 |
| Generics | `fn map<T, U>(xs: list<T>, f: fn(T) -> U) -> list<U>` | 4.6 / 7.3 |
| Untyped parameters (v0.3) | `fn f(x, y) { … }` — pseudo-type `any` | 7.5 |
| Kwargs (v0.3) | `greet(name: "Alice", greeting: "ciao")` | 7.6 / M29 |
| Mixed positional + kwargs | `greet("Alice", greeting: "ciao")` | 7.6 |
| Kwargs errors | unknown / duplicate / positional-after-named / missing → `Type` or `Arity` error | 7.6 |
| Visibility | `pub fn …`, `pub model …`, `pub policy …` | 3.3 |
| Defaults / variadics / optionals | **not supported** | 7.4 |

**§ 7.2 — Structural purity.** No `pure` keyword: a function without
a `cap` parameter cannot call any capability operation; module-level
`var` does not exist, so there is no ambient mutable state.

---

## 5. Expressions and control flow

### 5.1 Statements and branching

| Construct | Form | § |
|---|---|---|
| `if/else` | `let n = if x > 0 { 1 } else { -1 }` (expression) | 5.2 / 6.1 |
| `match` | `match v { p1 -> e1, p2 if g -> e2, _ -> default }` (exhaustive) | 6.1 / 17 |
| `while` | `while cond { … }` | 6.1 |
| `loop` (v0.3) | `loop { … }` = `while true { … }` | 6.1 |
| `for` | `for i in 0..10 { … }`, `for (k,v) in map { … }`, `for x in channel { … }` | 6.1 |
| `break` / `continue` | unlabelled by default; `'name: for …`, `break 'name` | 6.1 |
| `return` | allowed but rarely needed; last expression of a block = its value | 5.2 |
| `raise` | `raise err.user("...")` ≡ `return Err(…)`; forbidden in pure fns | 18.3 |
| `defer stmt` (v0.3) | LIFO on every exit path (return, `?`, raise, contract) | 18.5 |
| `until:` | declarative, only inside `agent_net` | 6.1 / 14.1 |
| Range | `a..b` half-open, `a..=b` inclusive | 6.2 |

### 5.2 String interpolation (M16)

| Form | Meaning | § |
|---|---|---|
| `"hi {name}"` | interpolates the expression; concatenates the stringified result | 2.4 / 5.3 |
| `"{f(g(1,2))}"` | braces nest: the inner expression may contain calls | 2.4 |
| `"\{ \}"` | literal braces (no `{{`/`}}` doubling rule) | 2.4 |
| `"{}"` | **lex error** — use `"\{\}"` for the literal `{}` | 2.4 |
| Migration | `aeris fmt --migrate-strings` rewrites legacy `\(...)` | 2.4 |

### 5.3 Time-control sugar (M18, v0.3)

| Form | What it does | § |
|---|---|---|
| `every D { … }` | periodic loop; first iteration runs immediately; `break`/`continue` supported | 6.4 |
| `retry N, delay: D { … }` | re-evaluates the body on `Err`; returns the last outcome | 6.4 |
| `timeout D { … }` | wall-clock check at cooperative cancel-points; `Err(err.user("timeout"))` on overflow | 6.4 |
| `clock.sleep(D)` | read-classified primitive, recorded for replay | 22 |

### 5.4 Pattern matching

| Pattern | Example | § |
|---|---|---|
| Literal | `0 -> "zero"` | 17.1 |
| Binder | `n -> if n > 0 { … }` | 17.1 |
| Enum unit | `Pending -> …` | 17.1 |
| Enum positional | `Active(t) -> …` | 17.1 |
| Enum named | `Banned { reason } -> …` | 17.1 |
| Result | `Ok(v) -> v`, `Err(e) -> raise e` | 17.1 |
| List | `[]`, `[x]`, `[x, ..rest]`, `[first, .., last]` | 17.1 |
| Guard | `Active(t) if t < cutoff -> …` | 17.1 |
| Wildcard | `_ -> default` (explicit) | 17.1 |
| `is` / `as` | `if r is Ok(v) { … }`, `let v = r as Ok` (sugar over `match`) | 17.3 |

**Exhaustiveness.** Computed structurally, no SMT. A match whose
arms are all guarded over a non-finite domain MUST include a
guard-free catch-all (§ 17.2).

### 5.5 Method-call dispatch (§ 5.4)

Resolution priority of `x.f(a)`:

1. **Module call** (`io.println`, `fs.read_text`, …) — cap-gated.
2. **Built-in method** on the value type (`list`, `string`, `map`, `Chat`, …).
3. **Record field as callable** — when `x` is a record with field `f` holding a closure.
4. **UFCS** — `f(x, a)` if `f` is a free function in scope.

---

## 6. Capabilities (§ 8)

### 6.1 Tree (frozen, 2 levels)

| Family | Operations | Class | V2 (mandatory intent) |
|---|---|---|---|
| `io` | `print`, `println`, `eprint`, `eprintln`, `read_line` | diag | no |
| `fs` | `read_file` / `read_text` / `read_bytes`, `write_file` / …, `walk`, `stat`, `exists`, `mkdir`, `remove`, `rename` | read+write | yes on writes |
| `http` | `get`, `post`, `put`, `patch`, `delete` | read+write | yes on writes |
| `shell` | `exec`, `pipe` | write | yes |
| `env` | `read` (M27: `set`) | read+write | yes on `set` |
| `clock` | `now`, `sleep` | read (recorded) | no |
| `random` | `next` | read (recorded) | no |
| `ai` | `complete`, `chat`, `embed`, `tools` | write (tape-recorded) | yes |
| `kube` | `apply`, `delete`, `get`, `watch` | read+write | yes on writes |
| `docker` | `run`, `build`, `push`, `pull`, `inspect` | read+write | yes on writes |
| `mongodb` | `read`, `write` | read+write | yes on writes |
| `minio` | `get`, `put` | read+write | yes on `put` |
| `rabbitmq` | `publish`, `subscribe` | read+write | yes on `publish` |
| `audit` | `event` | write | yes |

### 6.2 `cap` type syntax

| Form | Example | § |
|---|---|---|
| Operation list | `cap[fs.read_file, http.get]` | 8.1 / 8.3 |
| Allow-list | `cap[http.post @ ["api.acme.com"]]` | 8.3.1 |
| Multi-entry allow-list | `cap[fs.write_file @ ["./out/**", "./.aeris/**"]]` | 8.3.1 |
| Single-element (no brackets) | `cap[http.get @ "api.acme.com"]` | 8.3.1 |
| Body call (no prefix) | `http.post(url, body)?` — resolves against the in-scope `cap` | 8.2 |
| Narrowing | `cap.subset[http.post @ ["api.acme.com"]]` | 8.4 |
| Test narrowing | `cap.test_subset[…]` | 21.4 |
| `cap[*]` | **forbidden** in user code (E65) | 8.4 / 8.7 |

### 6.3 Allow-list grammar by family (§ 8.3.1)

| Family | Form | Example |
|---|---|---|
| `http.*` | `@ <host_list>` | `http.get @ ["api.acme.com"]` |
| `fs.*` | `@ <glob_list>` | `fs.write_file @ ["./out/**"]` |
| `kube.*` | `@ <context_list>` | `kube.apply @ ["prod-eu-1"]` |
| `mongodb.*` | `@ <db.collection_list>` | `mongodb.write @ ["app.users"]` |
| `minio.*` | `@ <bucket_list>` | `minio.put @ ["releases"]` |
| `rabbitmq.*` | `@ <queue_list>` | `rabbitmq.publish @ ["events.v1"]` |
| `shell.exec` | `@ <argv0_list>` | `shell.exec @ ["kubectl", "git"]` |
| `ai.*` | `@ <model_list>` | `ai.complete @ ["claude-opus-4-7"]` |

Families with no meaningful allow-list dimension: `clock.now`,
`random.next`, `env.read`, `audit.event`, `io.*`.

### 6.4 Enforcement modes (§ 8.4.1, M15B)

| Mode | `aeris init` default | `main(cap)` | E65 | E66 | E67 | E68 | E70 | E71 | Runtime allow-list |
|---|---|---|---|---|---|---|---|---|---|
| `enforce = "off"` | **yes** | `cap[*]` synthesised | suppressed | suppressed | suppressed | error | error | suppressed | bypassed |
| `enforce = "loose"` | — | synthesised from manifest | suppressed* | error | error | error | error | error | enforced |
| `enforce = "strict"` | — | synthesised from manifest | error | error | error | error | error | error | enforced |

\* In `loose`, E65 is suppressed on functions **without** `cap`;
functions that DO declare `cap` are still checked statically.

`required = true | false` remains a back-compat alias (`true` →
`strict`, `false` → `loose`).

### 6.5 Escape rules (§ 8.7)

`cap` **cannot**:
- be stored in a record field, a `const`, or any module-level binding
- be returned (unless the return type is itself `cap[…]`)
- be sent through `channel<T>`
- cross into `spawn { }` without an explicit `cap.subset[…]` capture
- appear as `cap[*]` in user code

---

## 7. Contracts, intent, sagas, agents, policies

### 7.1 Contracts (§ 9)

| Clause | Where it fires | Error |
|---|---|---|
| `requires:` | function entry | `ContractViolation`, exit 64, not catchable |
| `ensures:` | function exit (`result` = returned value) | `ContractViolation`, exit 64 |
| `where` on a field | record / model construction | `SchemaViolation` |
| `where:` record-level | after the field checks | `SchemaViolation` |
| `where` on a match arm | runtime gate | the arm does not fire |

No SMT, no proofs, no type narrowing.

### 7.2 Intent (§ 10)

| Form | Where | § |
|---|---|---|
| Block | `intent "rotate cert" { fs.write_file(…) }` | 10.2 |
| Saga-level | `saga deploy(…) { intent "ship v{ver}" step … }` | 10.2 |
| Agent-level | `agent c { intent: "classify invoices", … }` | 10.2 |
| Trace events | `intent_enter` (intent, scope), `intent_exit` (outcome) | 10.3 |

**V2 mandatory intent.** Write-effectful calls (§ 6.1, column "V2")
are rejected outside an enclosing `intent` (E66) under `loose` and
`strict`.

### 7.3 Sagas (§ 12)

| Construct | Form | § |
|---|---|---|
| Saga | `saga settle(batch, cap: cap[…]) { intent "…" step … }` | 12.1 |
| Step | `step name { do { … } undo { … } }` | 12.1 |
| Step with `requires` | `step ledger { requires: charge.ok; do { … } undo { … } }` | 12.1 |
| `undo: noop` | allowed **only** if `do` is not write-effectful (E67) | 12.2 |
| Idempotency (N1) | `key = blake3(trace_id ‖ step ‖ idx)` injected | 12.3 |
| `http.*` injection | header `Idempotency-Key: <hex>` | 12.3 |
| `kube.apply` injection | annotation `aeris.idempotency` | 12.3 |
| `mongodb.write` injection | field / unique-index sentinel | 12.3 |
| `rabbitmq.publish` injection | AMQP `message-id` | 12.3 |
| `audit.event` injection | `idempotency_key` field | 12.3 |
| Outcomes | `ok` / `rolled_back` / `PartialFailure` (exit 74) | 12.4 |

Trace events: `saga_enter`, `step_enter`, `step_exit`, `undo_enter`,
`undo_exit`, `saga_exit`.

### 7.4 Agents (§ 13)

| Field | Type / value | § |
|---|---|---|
| `llm:` | string literal (e.g. `"claude-opus-4-7"`) | 13.2 |
| `intent:` | string (propagated into the trace) | 13.2 |
| `prompt:` | triple-quoted; routing contract auto-injected | 13.2 |
| `accept:` | `model@vN` | 13.2 |
| `produce:` | `model@vN` | 13.2 |
| `policy:` | one or more `policy` declarations | 13.2 |
| `retries:` | retries on `SchemaViolation` | 13.2 |
| `budget:` | `{ tokens: N, latency: D }` — raises `BudgetExceeded` on overflow | 13.2 |
| Call site | `classify(inv, cap.subset[ai.complete @ ["…"]])` | 13.3 |

### 7.5 `agent_net` (§ 14)

| Construct | Form | § |
|---|---|---|
| Edge | `flow a -> b -> c` | 14.1 |
| Fan-out | `flow x -> { y, z }` — type-driven | 14.1 |
| Convergence loop | `until: classify.confidence > 0.95 || iterations >= 3` | 14.1 |
| Composition | a net may be a node of another net | 14.2 |
| Cycles | rejected at parse time (E70) | 14.1 |
| Outcomes | `ok(value)` / `Err("agent_net <name> exhausted")` | 14.3 |

### 7.6 Policies (§ 15)

| Clause | Meaning |
|---|---|
| `match:` | capability paths the policy matches against (`http.*`, `ai.complete`, …) |
| `deny:` | violation when `true` |
| `require:` | violation when `false` |
| `limit:` | quota over a window (`tokens_per_minute = 60_000`, `usd_per_day = 50`) |
| `audit:` | extra fields in the trace event for matching calls |
| `when:` | environment gate (`when: env == "production"`) |

Activation: by module import, by `#[policy(name)]` on a function,
or via `aeris.toml [policies] active = [...]`.
A divergence between live and replay emits a `policy_drift` event.

---

## 8. Errors and recovery (§ 18)

### 8.1 Closed variants of `err`

| Variant | Fields | § |
|---|---|---|
| `io` | `kind: io_kind, path: string` | 18.1 |
| `net` | `kind: net_kind, host: string, after: option<duration>` | 18.1 |
| `schema` | `model: string, version: string, problems: list<string>` | 18.1 |
| `contract` | `fn_name: string, clause: string` | 18.1 |
| `policy` | `name: string, fields: map<string, string>` | 18.1 |
| `budget` | `kind: budget_kind, used: u64, cap: u64` | 18.1 |
| `partial_failure` | `saga: string, completed: list<string>, failed: string` | 18.1 |
| `llm` | `model: string, code: int, message: string` | 18.1 |
| `user(string)` | string payload (may carry a JSON-encoded structure) | 18.1 |

### 8.2 Recovery

| Construct | Form | § |
|---|---|---|
| `?` (try) | `let bytes = fs.read_file(p)?` — propagates `Err` | 18.2 |
| `raise` | `raise err.user("...")` (forbidden in pure fns) | 18.3 |
| `error(msg)` (v0.3) | alias for `err.user(msg)` | 18.5 |
| `catch` (v0.3) | `expr catch err { recovery }` | 18.5 |
| `defer` (v0.3) | LIFO on every exit path (including `?`, raise, contract shutdown) | 18.5 |
| `??` | `expr ?? rhs` — fallback on `Err`/`None`/`()` | 2.6 / 18 |

`?` does **not** catch `ContractViolation` or `PolicyViolation` —
those are fatal (§ 18.4). `defer` **does** run before contract
shutdown.

---

## 9. Concurrency (§ 19)

| Construct | Form | Notes |
|---|---|---|
| `spawn { … }` | `let h = spawn { compute(cap.subset[…]) }` — OS thread | `cap` must be narrowed explicitly |
| `await` | `let r = await h` — yields the value or propagates the error | panic → `Err(err.user(…))` |
| `channel<T>` | `let ch: channel<int> = channel(capacity: 16)` | bounded MPMC |
| `ch.send(x)?` / `for x in ch { … }` | channel API | full `send` blocks; empty `recv` blocks |
| `ch.close()` | closes the channel | `send`/`recv` on closed → `Err(err.io)` |
| `h.cancel()` | cooperative cancellation | cancel-points: `await`, `?`, capability calls, `for x in ch` |

`channel<T>` forbids `T` = `cap`, closures capturing `cap`, or `handle`.

---

## 10. Tests (§ 21)

| Construct | Form | § |
|---|---|---|
| Unit test | `test "addition is commutative" { assert add(2,3) == add(3,2) }` | 21.1 |
| Property | `property "concat is associative" with (a, b, c) { assert (a ++ b) ++ c == a ++ (b ++ c) }` | 21.3 |
| File-as-suite | `tests/foo.test.aer` → suite `foo`; `aeris test foo` | 21.2 |
| Fixture mode | `test "..." with fixture: "settle.broken_ledger" { … }` | 21.5 |
| Test cap | `cap.test_subset[…]` (read-only over `tests/fixtures/**`) | 21.1 |

### 10.1 Specialised asserts (v0.3, M21)

| Helper | Signature | What it does |
|---|---|---|
| `assert(...)` | `assert <expr>` | generic boolean assertion |
| `assert_status(resp, code)` | `(resp, int)` | passes iff `resp.status == code` |
| `assert_json(text, keys)` | `(string, list<string>)` | passes iff `text` parses as a JSON object containing all `keys` |
| `assert_semantic(actual, criteria, judge?)` | `(string, string, string?)` | uses the AI backend as a judge; `judge` defaults to the active `ai.complete` cap's first model |

---

## 11. Modules, imports, manifest

### 11.1 `use` (§ 3.2)

| Form | Meaning |
|---|---|
| `use io, json, fs, http` | L1 stdlib (multi, comma-separated) |
| `use ai, kube` | L2 native handlers |
| `use "./lib/utils.aer"` | local, path-source |
| `use utils from "./lib/utils.aer"` | namespaced alias |
| `use deploy from "github.com/x/y" deploy@"1.2.0"` | external with version pin |
| `use { rollout, status } from deploy` | selective re-export |
| `use http as net` | rename |

External dependencies MUST have `hash = "blake3:…"` in `[deps]`.
Cyclic imports are forbidden at parse time.

### 11.2 Visibility (§ 3.3)

`pub` on a top-level declaration publishes it into the module's
surface. `aeris lock surface` records it in `.aeris/surface.lock`.

### 11.3 Top-level statements (v0.3, M26)

| What | Example | § |
|---|---|---|
| Module-level `let` | `let CLAUDE_ARGS = "--print --model claude-sonnet-4-6"` | 3.4 |
| Module-level call | `env.set(key: "AERIS_LLM_CLI", value: "claude {CLAUDE_ARGS}")` | 3.4 |
| Forbidden at module scope | `var` (only `const` and `let`) | 3.4 |
| Without `main` | the module body IS the program | 3.4 |

---

## 12. Layer 1 standard library (§ 22)

### 12.1 Modules

| Module | Operations |
|---|---|
| `io` | `print(msg)`, `println(msg)`, `eprint(msg)`, `eprintln(msg)`, `read_line() -> option<string>` |
| `fs` | `read_file(path)`, `read_text(path)`, `read_bytes(path)`, `write_file(path, bytes)`, `write_text(path, s)`, `write_bytes(path, b)`, `walk(path)`, `stat(path)`, `exists(path)`, `mkdir(path)`, `remove(path)`, `rename(src, dst)` |
| `http` | `get(url)`, `post(url, body, content_type?)`, `put(url, body, content_type?)`, `patch(url, body, content_type?)`, `delete(url)`, `req`, `resp`, `header`, `query`, `body<T>` |
| `shell` | `exec(argv)`, `pipe(argv1, argv2)`, `args()`, `quote(s)` |
| `env` | `read(key) -> option<string>`, `must_read(key) -> string`, `set(key, value)` (v0.3, M27) |
| `strings` | `trim`, `lower`, `upper`, `contains`, `starts_with`, `ends_with`, `split(sep)`, `join(sep)`, `replace(from, to)`, `parse_int(s) -> result<int>` |
| `date` | `today() -> date`, `timestamp() -> int`, `now() -> timestamp`, `format(t, fmt)` (`%Y %m %d %H %M %S`) |
| `json` | `decode<T>(s) -> result<T>`, `encode(v) -> string`, `parse(s) -> result<record>`, `stringify(v) ≡ encode`, `pretty(v)` |
| `yaml` | `parse(s) -> result<record>`, `parse_file(path)` |
| `clock` | `now() -> timestamp`, `sleep(D)` (read-classified, recorded) |
| `random` | `next() -> int` (recorded) |
| `net` | `http(port: int) -> HttpServer` (v0.3, M20) |

### 12.2 Methods on built-in values (no `use` required)

| Receiver | Methods |
|---|---|
| `list<T>` | `.len()`, `.empty()`, `.first()`, `.last()`, `.slice(a, b)`, `.contains(x)`, `.join(sep)`, `.map(f)`. On `var` bindings: `.push(x) -> int`, `.pop() -> option<T>` |
| `string` | `.len()`, `.trim()`, `.lower()`, `.upper()`, `.contains(p)`, `.starts_with(p)`, `.ends_with(p)`, `.split(sep)`, `.replace(from, to)`, `.index_of(needle, from?) -> option<int>` |
| `map<K, V>` | `.len()`, `.get(k) -> option<V>` |
| `Chat` | `.ask(prompt) -> result<string>` (M32), `.kb_size() -> int` |
| `HttpServer` | `.accept() -> HttpReq` |
| `HttpReq` | `.reply(status, body, content_type?) -> unit`, `.reply_json(status, body) -> unit` |
| `AiNetwork` | `.agent(name, system) -> unit` (mutating), `.run(entry, message, until?) -> { trace, rounds }` |

`HttpReq` fields: `method`, `path`, `query_raw`, `headers` (record),
`body` (string), `remote_addr`.

### 12.3 Global intrinsics

| Function | Accepts |
|---|---|
| `len(x)` | `list`, `set`, `tuple`, `map`, `string`, `bytes` |
| `error(msg)` | string → `err.user(msg)` |
| `print(msg)` | shorthand for `io.println` (natural-display form) |

---

## 13. Layer 2 native cap handlers (§ 23)

### 13.1 Modules

| Module | Capability paths |
|---|---|
| `ai` | `ai.complete`, `ai.chat`, `ai.embed`, `ai.tools` |
| `kube` | `kube.apply`, `kube.delete`, `kube.get`, `kube.watch` |
| `docker` | `docker.run`, `docker.build`, `docker.push`, `docker.pull`, `docker.inspect` |
| `mongodb` | `mongodb.read`, `mongodb.write` |
| `minio` | `minio.get(bucket, object)`, `minio.put(bucket, object, content)`, `minio.mb(bucket)`, `minio.bucket_exists(bucket) -> bool`, `minio.list(bucket) -> list<string>` |
| `rabbitmq` | `rabbitmq.publish`, `rabbitmq.subscribe` |
| `audit` | `audit.event` |

### 13.2 Inline AI builtins (v0.3, M19 / M28)

| Builtin | Signature | What it does |
|---|---|---|
| `ai.complete(prompt, model?)` | `(string, string?) -> string` | direct backend call; tape-recorded |
| `ai.session(system, model)` | `(string, string) -> Session` | open a multi-turn conversation |
| `ai.session_ask(session, prompt)` | `(Session, string) -> (Session, string)` | append + reply; auto-compaction 40 → 20 |
| `ai.decide(prompt, choices, retries?)` | `(string, list<string>, int=3) -> string` | enum-style; retries on mismatch |
| `ai.usage()` | `() -> { total_tokens: int, cost_usd: f64, calls: int }` | in-memory diagnostic |
| `ai.chat(messages)` | `(list<message>) -> string` | v0.2 message-list API |
| `ai.chat(system, dir)` | `(string, string) -> Chat` | v0.3 KB-loaded REPL handle |
| `ai.network(max_rounds)` | `(int) -> AiNetwork` | programmatic multi-agent builder |

### 13.3 `audit.event` (always available)

| Form | What it does |
|---|---|
| `audit.event(kind, fields)` | append-only log; idempotency key auto-derived |

---

## 14. Trace & replay (§ 20)

### 14.1 Recorded events (N2 / N3)

| Source | Fields |
|---|---|
| `ai.*` | `prompt`, `model`, `response`, `tokens`, `latency` |
| `clock.now` | `value` |
| `clock.sleep` | `duration` |
| `random.next` | `value` |
| `http.*` | `url`, `method`, `status`, `req_hash`, `resp_hash` |
| `fs.read_*` | `path`, `len`, `hash` |
| `fs.write_*` | `path`, `len`, `hash` |
| `shell.exec` | `argv`, `env_pruned`, `exit`, `stdout_hash`, `stderr_hash` |
| `intent` | `intent_enter`, `intent_exit` (`outcome`) |
| `saga` | `saga_enter`, `step_enter`, `step_exit`, `undo_enter`, `undo_exit`, `saga_exit` |
| `agent_net` | `net_enter`, `edge`, `agent_call`, `net_exit` |
| `policy` | `policy_eval`, `policy_drift` (in replay) |

### 14.2 Trace channel

Path: `.aeris/traces/<trace_id>.jsonl`.
HTTP propagation: header `X-Aeris-Trace-Id: <trace_id>`.
Always on. `--full-record` enables byte-level body capture.

### 14.3 Replay

| Command | What it does |
|---|---|
| `aeris replay <trace_id>` | replays from the tape (default `--from-fixtures`) |
| `aeris replay <trace_id> --live` | live network / LLM, keeps recorded clock and random |
| `aeris trace tail [<id>]` | follow a trace |
| `aeris trace diff <a> <b>` | aligns events by `(scope, ordinal)` and reports divergences |

---

## 15. CLI

### 15.1 Commands (§ 25.1)

| Command | What it does |
|---|---|
| `aeris run <file>` | compile-and-run |
| `aeris test <file_or_glob>` | run tests |
| `aeris fmt [--narrow-caps] [--migrate-strings] <file>` | total formatter |
| `aeris check <file>` | type + cap-graph check, no run |
| `aeris doc <file>` | extract `/// doc` comments → JSONL |
| `aeris lock [surface]` | write `aeris.toml` / `.aeris/surface.lock` |
| `aeris replay <trace_id> [--live]` | replay |
| `aeris trace tail [<trace_id>]` | follow |
| `aeris trace diff <a> <b>` | trace differ |
| `aeris init` | scaffold a project (default `enforce = "off"`) |
| `aeris version` | prints `aeris 0.3.0` |

### 15.2 Exit codes (§ 25.3)

| Code | Cause |
|---|---|
| 0  | ok |
| 64 | parse / type / contract |
| 65 | capability (missing / over-broad / `cap[*]` in user code) |
| 66 | intent missing on a write |
| 67 | saga step lacks `undo` |
| 68 | model version missing / conflict |
| 69 | lockset stale / hash mismatch |
| 70 | cycle in `agent_net` |
| 71 | signature allow-list exceeds the manifest ceiling |
| 74 | saga `PartialFailure` (undo retries exhausted) |

---

## 16. Manifest (`aeris.toml`, § 24.1)

### 16.1 Sections

| Section | Fields |
|---|---|
| `[project]` | `name`, `aeris = "0.3.0"` |
| `[deps]` | `<alias> = { source = "...", version = "...", hash = "blake3:..." }` or `{ path = "./...", hash = "..." }` |
| `[caps]` | `enforce = "off" \| "loose" \| "strict"`, `http.allow`, `fs.allow_read`, `fs.allow_write`, `kube.contexts`, `ai.models`, ... |
| `[ai.backend]` | `kind = "mock" \| "http" \| "cli"`, `url` (when `kind=http`), `auth` (e.g. `env:VAR`), `cmd` (when `kind=cli`) |
| `[policies]` | `active = ["pol1", "pol2"]` |

### 16.2 Surface lock — `.aeris/surface.lock`

Generated by `aeris lock surface`. For each `pub fn`:

```toml
[surface."src/invoices.aer".settle]
caps       = ["http.post", "kube.apply", "audit.event"]
allow.http = ["api.acme.com", "api.stripe.com"]
allow.kube = ["prod-eu-1"]
```

A PR that **broadens** any surface MUST regenerate the lock (the
diff appears as the first hunk in review).

---

## 17. v0.3 — additions summary

| Construct / API | Milestone | § |
|---|---|---|
| Interpolation `"{x}"` | M16 | 2.4 / 5.3 |
| `catch`, `error()`, `defer` | M17 | 18.5 |
| `every`, `retry`, `timeout`, `clock.sleep` | M18 | 6.4 |
| `ai.session`, `ai.session_ask`, `ai.decide`, `ai.usage`, `ai.chat(dir:)` | M19 | 23 |
| `net.http(port)` + `HttpServer` / `HttpReq` | M20 | 22 |
| `assert_status`, `assert_json`, `assert_semantic` | M21 | 21.4 |
| `model X@v2 extends X@v1` | M23 | 16.5 |
| `loop { }`, `??`, methods, natural JSON, `strings.*`, `date.*` | M24 | 5.4 / 6.1 / 22 |
| Untyped params + kwargs (builtins) | M25 | 7.5 / 7.6 |
| Top-level statements | M26 | 3.4 |
| `env.set`, `list.push/pop`, `yaml.parse` | M27 | 22 |
| `ai.network(max_rounds)` programmatic | M28 | 23 |
| Kwargs on user-defined functions and closures | M29 | 7.6 |
| Modes `enforce = off \| loose \| strict` | M15B | 8.4.1 |

---

*For the detailed behaviour of each construct: [`language.md`](language.md).
For the rationale: [`thesis.md`](thesis.md). For the implementation
plan and milestones: [`plan.md`](plan.md).*
