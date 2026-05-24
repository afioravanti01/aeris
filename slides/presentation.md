---
marp: true
theme: aeris
paginate: true
html: true
size: 16:10
title: "Aeris v0.3"
header: 'Technical presentation · v0.3'
footer: 'Aeris v0.3 · interpreted language for operations, AI and governance'
---


<style>
  /* Bumped from theme default (36px) so the slimmer slides fill the canvas. */
  section { font-size: 44px; }
  figure.aeris-figure {
    margin: 0.4em auto;
    width: 100%;
    text-align: center;
  }
  figure.aeris-figure svg {
    width: 100%;
    height: auto;
    display: block;
  }
  section.divider code,
  section.divider p code,
  section.divider blockquote code,
  section.divider li code,
  section.divider h1 code {
    background: rgba(255, 255, 255, 0.14) !important;
    color: var(--cream, #F6F3F0) !important;
    border: 1px solid rgba(255, 255, 255, 0.18) !important;
  }
</style>

<!-- _class: cover -->

<p class="eyebrow">Technical presentation · v0.3</p>

## AERIS v0.3

An interpreted language for **automation**, **AI orchestration**, **operations** and **governance** — with capabilities, intent, and sagas as first-class constructs.

---

# Agenda

| # | Section | What it covers |
|---|---|---|
| **1** | Aeris at a glance | What it is, what it's for |
| **2** | How an interpreted language works | Lexer · parser · check · tree-walk |
| **3** | The four layers | Architectural rationale |
| **4** | Core language | Types, control flow, sagas, concurrency, modules |
| **5** | AI primitives | Sessions, decisions, knowledge bases, multi-agent |
| **6** | Verifiability | Capabilities, allow-lists, enforce modes |
| **7** | Governance & reasoning | Intent, contracts, policy, trace, supply chain |
| **8** | Putting it together | End-to-end SRE alert triage |

---

# Aeris at a glance

> A general-purpose interpreted language written in Rust, built around a specific domain: **operations, AI orchestration, and governance**.

- **Runtime** — single static binary `aeris`, < 8 MB. Zero external runtime requirements. Tree-walk interpreter; one file extension `.aer`; one project manifest `aeris.toml`.
- **Libraries** — general-purpose stdlib · native domain handlers · external `.aer` modules pinned by cryptographic hash.
- **LLM integration** — pluggable backend: HTTP API or local CLI process, selected in `aeris.toml`.
- **What it replaces in one file** — `bash` / Python / Terraform scripts, Airflow / Argo workflow manifests, LangChain / CrewAI agent graphs, OPA / Rego security rules.

> One grammar covers a 30-line script and a multi-agent system. Discipline is **opt-in by depth**.

---

<!-- _class: tight -->

# Hello world

<div class="columns">
<div class="column">

```rust
// Script mode — no main, no ceremony.
use io

io.println("hello, aeris")
```

```rust
// With a main function.
use io

fn main() {
  io.println("hello, aeris")
}
```

</div>
<div class="column compact">

- A `.aer` file without `fn main` is a **valid program** — top-level statements run in declaration order.
- A `.aer` file with `fn main` runs the function after the top-level statements.
- `use io` is **mandatory** to call `io.println` — modules must be brought into scope.
- The standard library is **closed** — no third-party deps. Adding a built-in requires an Aeris release.

</div>
</div>

---

<!-- _class: tight -->

# How an interpreted language works

<div class="columns">
<div class="column compact">

**1 · Lexer** — reads the bytes of a `.aer` file and emits typed tokens annotated with line numbers. An unknown character ends the run.

**2 · Parser** — recursive-descent over the tokens. Builds the **AST** — a tree where each node is a language construct (a `let`, a call, a `saga`, an `agent`).

**3 · Static check** — pre-run pass that verifies structural properties: schemas, idempotency-key obligations, agent graphs. Failure → distinct exit codes per category.

**4 · Interpreter** — walks the AST node by node. Each statement updates the scope; each expression returns a value. Side effects go through the standard library.

</div>
<div class="column">

```rust
// The Aeris source tree, mapped to the pipeline.
src/lexer.rs        // bytes  → tokens
src/parser.rs       // tokens → AST
src/checker.rs      // AST    → ok / error
src/interpreter.rs  // AST    → value, side effects
```

> The AST **is** the program. No bytecode, no intermediate representation, no compilation step.

> ~6 KLOC of Rust core. Zero external runtime. Single-shell-script deployment.

</div>
</div>

---

<!-- _class: tight -->

# The AST walk

<div class="columns">
<div class="column">

```rust
fn walk(node: Node, env: &mut Env) -> Value {
  match node {
    Let(name, e)   => env.set(name, walk(e, env)),
    If(c, t, f)    => if walk(c, env).is_truthy() {
                        walk(t, env)
                      } else { walk(f, env) },
    Call(f, args)  => apply(f, args.map(|a| walk(a, env))),
    Block(stmts)   => stmts.for_each(|s| walk(s, env)),
    Return(e)      => unwind(walk(e, env)),
    // ...one arm per AST variant
  }
}
```

</div>
<div class="column compact">

**Reading the walk**

- An *expression* node returns a `Value` — data flows back up the tree.
- A *statement* node updates the scope or triggers an effect.
- `return` / `break` / `continue` are error variants that unwind to the right frame.

**A function call is a sub-walk**

- The function's body is an AST sub-tree.
- A new scope is pushed, parameters bound, the body recursed into.
- Closures snapshot their environment, so `spawn { ... }` keeps the scope chain alive.

</div>
</div>

---

# The four layers

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 360" role="img" aria-label="The four layers stacked">
<defs>
<marker id="arrL" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="20" y="15" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="45" font-size="22" font-weight="700" fill="#0E1020">L1 — AI-native syntax</text>
<text x="40" y="72" font-size="16" fill="#5F6470">one canonical form, all keywords reserved</text>
<line x1="300" y1="85" x2="300" y2="100" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="100" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="130" font-size="22" font-weight="700" fill="#0E1020">L2 — Verifiable semantics</text>
<text x="40" y="157" font-size="16" fill="#5F6470">capabilities-as-values, contracts, intent</text>
<line x1="300" y1="170" x2="300" y2="185" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="185" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="215" font-size="22" font-weight="700" fill="#0E1020">L3 — Agentic loop</text>
<text x="40" y="242" font-size="16" fill="#5F6470">saga with do/undo, derived idempotency</text>
<line x1="300" y1="255" x2="300" y2="270" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="270" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="300" font-size="22" font-weight="700" fill="#0E1020">L4 — Multi-agent orchestration</text>
<text x="40" y="327" font-size="16" fill="#5F6470">typed agent_net, schema at every edge</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

- Each layer **composes** with the ones below.
- A program **pays only** for the layers it uses.
- A 30-line script lives in **L1**.
- A self-recovering pipeline uses **L1 + L2 + L3**.
- A coordinated multi-agent system uses **all four**.

</div>
</div>

---

# Why these four layers?

> Code is increasingly **generated** by LLMs and **read** by LLMs — for reasoning, debugging, modifying. That changes the design constraints of the language itself.

<div class="columns">
<div class="column compact">

**Two requirements that matter now**

- **Reduce non-determinism** at every level the language can control.
- **Make code mechanically verifiable** — what is in the source is the truth.

**The four layers, as a response**

- **L1 — syntax.** Density and zero ambiguity reduce hallucinations.
- **L2 — semantics.** `cap` makes a function's intent mechanically checkable.

</div>
<div class="column compact">

- **L3 — agentic loop.** Per-step trace + idempotent compensations make recovery deterministic over non-deterministic execution.
- **L4 — multi-agent.** When 3+ agents coordinate, the routing protocol *is* the program. Lifting it to a typed graph eliminates coordination-as-prompt-string.

> Opt-in is the contract. A throwaway script uses only L1; a regulated production system uses all four. Verification, agents, governance — each layer activates *only when needed*.

</div>
</div>

---

<!-- _class: divider -->

# Core language

> Curly braces, named arguments, models, sagas, concurrency. Everything that ships in the binary.

---

<!-- _class: tight -->

# Language at a glance

<div class="columns">
<div class="column">

```rust
// Variables: let immutable, var mutable, const file-level.
let title  = "report"
var count  = 0
const PI   = 3.14159

// Type annotations optional, encouraged at API boundaries.
let amount: decimal = 12.50

// Functions return the last expression — no return needed.
fn greet(name: string) -> string { "Hello, {name}!" }
greet(name: "Aeris")

// Closures are first-class values.
let double = fn(x) { x * 2 }
let evens  = [1, 2, 3, 4].filter(fn(x) { x % 2 == 0 })

// String interpolation, multi-line strings.
let log = "request {req.method} {req.path}"
let prompt = """
  You are an SRE.
  Analyse: {log}
"""
```

</div>
<div class="column compact">

- Three binding forms: `let` (immutable, default), `var` (mutable, function-scope only), `const` (file-level).
- Types are **inferred**; annotations live at API boundaries.
- Functions return the last expression of their body — no `return` keyword needed.
- Call sites use **named arguments**: `greet(name: "Aeris")` — order-independent.
- Strings interpolate with `{expr}`; triple-quoted strings preserve newlines.

</div>
</div>

---

<!-- _class: tight -->

# Control flow — `if`, `match`, loops

<div class="columns">
<div class="column">

```rust
// if and match are expressions — they yield a value.
let label = if score >= 90 { "A" }
            else if score >= 70 { "B" }
            else { "C" }

let summary = match resp.status {
  200      -> "ok",
  400..499 -> "client error",
  500..599 -> "server error",
  _        -> "other",
}

// Loops.
for tag in tags { io.println(tag) }
for (k, v) in map { io.println("{k}={v}") }
while count > 0 { count = count - 1 }
loop {
  let line = io.read_line() ?? ""
  if line == "quit" { break }
}
```

</div>
<div class="column compact">

- `if` and `match` are **expressions** — they evaluate to a value.
- `match` patterns: literals, ranges (`400..499`), wildcards (`_`), enum variants.
- `match` is **exhaustive** — the compiler tells you which case you forgot.
- `for x in iter` walks any iterable (`list`, `range`, `map`, `channel`).
- `loop { ... }` is sugar for `while true { ... }`.

</div>
</div>

---

<!-- _class: tight -->

# Pattern matching — enums and destructuring

<div class="columns">
<div class="column">

```rust
enum Status {
  Pending,
  Active(since: timestamp),
  Banned { reason: string, until: option<date> },
}

match s {
  Pending           -> "pending",
  Active(t)         -> "since {t}",
  Banned { reason } -> "blocked: {reason}",
}

// List destructuring with a rest binding.
match xs {
  []                -> "empty",
  [x]               -> "single {x}",
  [first, ..rest]   -> "{first} + {len(rest)} more",
  [a, .., z]        -> "{a} ... {z}",
}

// Result destructuring.
match http.get(url) {
  Ok(resp) -> use_body(resp.body),
  Err(e)   -> raise e,
}
```

</div>
<div class="column compact">

- `enum` variants can carry **typed payload fields** (positional or named).
- Variant patterns **bind** their payload — `Active(t)` makes `t` available in the arm.
- List patterns support `..rest` for "everything else".
- Pattern matching covers `option<T>`, `result<T>`, and any user enum with one structural API.

</div>
</div>

---

<!-- _class: tight -->

# Models — records, enums, versioned schemas

<div class="columns">
<div class="column">

```rust
// record — a named-field struct.
record User {
  id:   uuid
  name: string
  age:  int  where age >= 0
}

let u = User { id: uuid_v7(), name: "Ada", age: 36 }
let v = User { ..u, age: 37 }       // structural update

// model X@vN — a record + a version tag + constraints.
model Invoice@v1 {
  id:       uuid
  amount:   decimal where amount > 0
  customer: string  where len(customer) <= 64

  where: status == Cancelled implies amount == 0
}

// v2 extends v1 — inherits fields and constraints.
model Invoice@v2 extends Invoice@v1 {
  currency: string where len(currency) == 3
}
```

</div>
<div class="column compact">

- `record` is a struct with named, typed fields.
- `model X@vN` is a **versioned record**, validated at every **trust boundary** (HTTP ingress, JSON decoding, agent edges).
- `where` clauses run on construction — both per-field and at the record level.
- `extends` inherits the parent's shape: you may add fields, never remove or rename.
- A bare `model X` (no `@vN`) at a trust boundary is a parse error.

</div>
</div>

---

<!-- _class: tight -->

# Errors & recovery — `result`, `?`, `??`, `catch`, `defer`

<div class="columns">
<div class="column">

```rust
// result<T> is Ok(value) or Err(error) — errors are values.
fn read_config(path: string) -> result<Config> {
  let bytes = fs.read_file(path)?     // propagate Err
  json.decode<Config>(bytes)
}

// ?? substitutes on Err / None / unit.
let nick = lookup_nickname() ?? "anonymous"

// catch — handle Err inline, supply a fallback.
let data = fs.read_file("config.json") catch err {
  io.eprintln("missing: {err.message}")
  b"{}"
}

// defer — LIFO cleanup on every exit path.
fn render(items: list<Item>) -> result<unit> {
  let tmp = fs.create_temp()?
  defer fs.remove(tmp)
  fs.write_file("./out/report.html", build(items, tmp))?
  Ok(())
}
```

</div>
<div class="column compact">

- `result<T>` is `Ok(value)` or `Err(error)` — **errors are values**, not exceptions.
- `?` after an expression: if `Err`, return early from the caller.
- `??` substitutes on `Err`, `None`, or unit.
- `catch err { ... }` handles errors inline; `err` binds to the error value.
- `defer stmt` runs on **every** exit path (return, `?`, `raise`) — like Go's `defer`.

</div>
</div>

---

<!-- _class: tight -->

# Time control — `every`, `retry`, `timeout`

```rust
// Periodic loop — first iteration runs immediately.
every 5m {
  let h = http.get("https://api.acme.com/health")
  if !h.ok { audit.event("api.down", { ts: clock.now() }) }
}

// Bounded retry on Err.
let r = retry 3, delay: 1s {
  http.get("https://api.acme.com/status")
}

// Wall-clock bound; not preempted mid-statement.
let r = timeout 30s { long_running_call() }
```

- `every D` runs the body, waits `D`, repeats. `break` exits, `continue` skips.
- `retry N, delay: D` re-runs the body on `Err`, up to `N` times with a pause `D`.
- `timeout D` fails the block with `Err(...)` if it exceeds the wall-clock budget.
- `clock.sleep(D)` is recorded so that `aeris replay` reproduces the same timeline.

> No external scheduler (cron / systemd / Airflow) needed — the loop is part of the program, observable in the trace, replayable offline.

---

<!-- _class: tight -->

# Saga — the flagship construct for writes

```rust
saga settle(batch: list<Invoice@v1>) {
  intent "settle invoice batch"
  step charge {
    do   { for it in batch { http.post("/charge", it)? } }
    undo { for it in batch { http.post("/refund", it)? } }
  }
  step ledger {
    requires: charge.ok
    do   { kube.apply(ledger_manifest(batch))? }
    undo { kube.delete(ledger_manifest(batch))? }
  }
}
```

- `saga` groups a **multi-step operation that needs compensation** if a later step fails.
- Each `step` declares **both** `do` (the action) and `undo` (the compensation).
- If a later step fails, the runtime runs the `undo`s of the completed steps in reverse order.
- `undo: noop` is allowed **only when `do` does not write** — every external write must declare how to undo itself.
- Three deterministic outcomes: `ok`, `rolled_back`, or `PartialFailure` (when even the rollback fails — exit code 74).

> `intent "..."` declares the saga's purpose; it appears in every trace event emitted from inside the saga.

---

# Idempotency key — generated for you

**The problem.** Network drops mid-write — you sent `POST /charge`, you don't know if the backend received it. If you retry, you risk a **double charge**. If you don't, you risk **no charge at all**.

**The standard fix.** Attach a unique string — an *idempotency key* — to every write. The backend remembers keys it has already seen and drops the duplicates. Used by Stripe, AWS, Kubernetes, queues.

**What Aeris does.** Inside every `saga` step, the runtime derives one key per invocation from three stable values:

```text
key = blake3( trace_id  ‖  step_name  ‖  invocation_index )
            └─ run id ─┘  └─ in saga ─┘  └─ retry count ─┘
```

| Backend | Where the key goes |
|---|---|
| `http.{post, put, patch}` | header `Idempotency-Key` |
| `kube.apply` | annotation `aeris.idempotency` |
| `rabbitmq.publish` | `message-id` field |

- Generated **for every saga step**, in all `enforce` modes (`off` / `loose` / `strict`).
- Outside a saga, calls do not get the derived key — `saga` is the trigger.
- Recorded into the trace alongside the call — `aeris replay` reuses the same keys.

> Same program replayed → same `trace_id` → **same keys** → the backend drops every duplicate.

---

<!-- _class: tight -->

# Concurrency — `spawn`, `channel`, cancellation

<div class="columns">
<div class="column">

```rust
// spawn returns a handle; await yields the result.
let h_a = spawn { fetch_a() }
let h_b = spawn { fetch_b() }
let (a, b) = (await h_a, await h_b)

// Bounded channel between threads.
let ch: channel<int> = channel(capacity: 16)
spawn {
  for x in 1..100 { ch.send(x)? }
  ch.close()
}
for x in ch { io.println("{x}") }

// Cooperative cancellation.
let h = spawn { long_running() }
h.cancel()      // delivered at the next cancel-point
```

</div>
<div class="column compact">

- `spawn { ... }` returns a `handle<T>`; `await h` yields the body's value.
- `channel<T>` is a bounded queue between threads — `send` on full blocks, `recv` on empty blocks.
- Cancellation is **cooperative**: cancel-points are `await`, `?`, capability calls, `for x in ch`.
- The current runtime executes `spawn` inline on the same thread; a `spawn_inline` trace event marks the limitation.

</div>
</div>

---

<!-- _class: tight -->

# Modules — three layers, one keyword

<div class="columns">
<div class="column">

```rust
// Layer 1: general-purpose stdlib (built in).
use io, json, fs, http, shell

// Layer 2: native domain handlers (built in).
use ai, kube, mongodb

// Layer 3: external .aer libraries.
use deploy
  from "github.com/acmecorp/aeris-devops"
       deploy@"1.2.0"
use "./lib/utilities.aer"
use utilities from "./lib/utilities.aer"
use http as net           // rename at the use site
```

</div>
<div class="column compact">

- All imports use the same keyword: `use`. The layer is inferred from the form.
- **Layer 1 & 2 are built into the `aeris` binary** — adding a module requires an Aeris release.
- **Layer 3 is `.aer` source**, pinned by `blake3` hash in `aeris.toml`. No `.so` / `.dll` at runtime.
- `use` is **mandatory** for every module reference (exit code 72).
- Cyclic imports are rejected at parse time.

</div>
</div>

---

# Standard library — general-purpose modules

<div class="columns">
<div class="column">

| Module | Operations |
|---|---|
| `io` | `print`, `println`, `read_line` |
| `fs` | `read_*`, `write_*`, `walk`, `mkdir`, `remove` |
| `http` | `get`, `post`, `put`, `patch`, `delete` |
| `shell` | `exec`, `pipe` |
| `strings`, `date`, `json`, `yaml` | parsing, formatting |
| `clock`, `random` | recorded for replay |
| `net` | `http(port) -> HttpServer` |

</div>
<div class="column">

```rust
use io, fs, http, json

let cfg  = json.decode<Config>(fs.read_text("./cfg.json"))?
let resp = http.get("https://api.acme.com/{cfg.path}")?
io.println("{resp.status} — {resp.body.len()} bytes")
```

> `use` brings the module into scope; what each operation does is defined by the runtime.

</div>
</div>

---

# Standard library — native domain handlers

<div class="columns">
<div class="column">

| Module | Operations |
|---|---|
| `ai` | `complete`, `chat`, `embed`, `tools` |
| `kube` | `apply`, `delete`, `get`, `watch` |
| `docker` | `run`, `build`, `push`, `pull` |
| `mongodb` | `read`, `write` |
| `minio`, `rabbitmq` | object store, message queue |
| `audit` | `event` |

</div>
<div class="column">

```rust
use ai, kube, audit

intent "scale the inference pod" {
  kube.apply(scale_manifest(pod, replicas: 4))?
  audit.event("scaled", { pod, to: 4 })
}
```

> These modules are **compiled into the `aeris` binary**. No runtime plug-ins, no `.so` / `.dll` loading.

</div>
</div>

---

<!-- _class: tight -->

# A full HTTP server — `net.http`

<div class="columns">
<div class="column">

```rust
use net, fs, json

let server = net.http(port: 8080)

loop {
  let req = server.accept()
  spawn {
    if req.path == "/api/health" {
      req.reply_json(200, json.encode({ status: "ok" }))
    } else if req.path == "/" {
      req.reply(200, fs.read_text("./index.html"), "text/html")
    } else {
      req.reply(404, "not found")
    }
  }
}
```

</div>
<div class="column compact">

- `net.http(port) -> HttpServer` opens a blocking TCP listener.
- `server.accept() -> HttpReq` returns the next request.
- `req.reply` / `req.reply_json` write the response.
- Per-request fan-out via `spawn`.
- A `net_listen` event opens the trace; an `http_request` event is recorded per accepted request.

> Part of the binary — no framework, no linked library.

</div>
</div>

---

<!-- _class: tight -->

# Tests — built into the language

<div class="columns">
<div class="column">

```rust

test "addition is commutative" {
  assert add(2, 3) == add(3, 2)
}

test "GET /health" {
  let resp = http_for_tests().get("/health")
  assert_status(resp, 200)
  assert_json(resp.body, ["status", "version"])
}

test "summary is faithful" {
  assert_semantic(
    actual:   summarise(doc),
    criteria: "faithful and complete to the original",
    judge:    "claude-haiku-4-5",
  )
}

property "concat is associative" with (
  a: list<int>, b: list<int>, c: list<int>,
) {
  assert (a ++ b) ++ c == a ++ (b ++ c)
}
```

</div>
<div class="column compact">

| Helper | Checks |
|---|---|
| `assert e` | boolean |
| `assert_status(r, c)` | HTTP status |
| `assert_json(t, keys)` | JSON object has keys |
| `assert_semantic(...)` | model as judge |

- `test` and `property` blocks are top-level. The **file** is the grouping unit — no `suite` keyword.
- `assert_semantic` uses the model as a judge — pass iff it agrees the criterion is met.
- `aeris test <file>` runs the tests; exit 0 = all pass.

</div>
</div>

---

<!-- _class: divider -->

# AI primitives

> Model calls as standard-library functions. Sessions, decisions, knowledge bases, multi-agent graphs.

---

<!-- _class: tight -->

# AI primitives — direct call and multi-turn

<div class="columns">
<div class="column">

```rust
use ai, io

// One-shot.
let answer = ai.complete("Analyse: {log}")

// Multi-turn; auto-compaction past 40 messages
// (last 20 kept, earlier turns summarised).
let s        = ai.session(
  system: "You are an SRE assistant.",
  model:  "claude-haiku-4-5",
)
let (s2, a)  = ai.session_ask(s,  "What does this log mean?")
let (s3, b)  = ai.session_ask(s2, "And how do I react?")
```

</div>
<div class="column compact">

- **`ai.complete(prompt, model?)`** — single-shot call to the model.
- **`ai.session` + `ai.session_ask`** — rolling conversation. Past 40 messages the history is compacted: the last 20 remain, earlier turns become a single system summary.
- Every call is recorded as an `ai_call` event — `aeris replay` returns it from the trace, without contacting the model.

</div>
</div>

---

<!-- _class: tight -->

# AI primitives — constrained choice and usage

<div class="columns">
<div class="column">

```rust
// Enum-style decision, auto-retry on mismatch.
let action = ai.decide(
  prompt:  "CPU at 95%. What to do?",
  choices: ["scale_up", "restart", "alert", "ignore"],
  retries: 3,
)?

// Per-process counters.
let u = ai.usage()
io.println("spent ${u.cost_usd} over {u.calls} calls")
```

</div>
<div class="column compact">

**`ai.decide`**

- Post-validates the reply against `choices`.
- Retries on mismatch; `Err(err.llm(...))` after `retries` failures.

**`ai.usage`**

- Read-classified diagnostic.
- Counter is in-memory; cost via static price table indexed by model name.

</div>
</div>

---

<!-- _class: tight -->

# `ai.chat` — knowledge base and integrated server

<div class="columns">
<div class="column">

```rust
// Form 1 — handle with a KB loaded from a directory.
let chat = ai.chat(
  system: "Answer only from the loaded documents.",
  dir:    "./docs",
)
io.println("{chat.kb_size()} files loaded")
io.println(chat.ask("how do capabilities work?")?)

// Form 2 — same KB + integrated HTTP server.
ai.chat(
  system: "You are the Aeris assistant.",
  dir:    "./docs",
  port:   8080,
)
// blocking accept loop; does not return
```

</div>
<div class="column compact">

**Form 1 — knowledge base**

- Loads `*.md`, `*.txt`, `*.rst`, `*.adoc`, `*.yaml` into the system prompt.
- Returns a `Chat` value with `.ask(prompt)` and `.kb_size()`.

**Form 2 — `port: int`**

- Same KB plus an HTTP server on the given port: `GET /`, `POST /api/chat`, `GET /api/health`, CORS preflight.
- A complete chatbot — KB, server, healthcheck, CORS — in **one stdlib call**.

</div>
</div>

---

<!-- _class: tight -->

# Multi-agent — `agent_net` (declarative) vs `ai.network` (programmatic)

<div class="columns">
<div class="column">

```rust
// Declarative — typed on model@vN.
model Doc@v1      { text: string }
model Summary@v1  { headline: string, bullets: list<string> }

agent summarise {
  llm:     "claude-haiku-4-5"
  accept:  Doc@v1
  produce: Summary@v1
  prompt:  "Summarise in <= 5 bullets."
}

agent_net summarise_loop {
  flow summarise -> critique
  until: critique.ok == true || iterations >= 3
}
```

</div>
<div class="column">

```rust
// Programmatic — runtime-discovered agents.
fn main() {
  var net = ai.network(max_rounds: 10)

  net.agent(name: "geologist",
            system: fs.read_text("./agents/geo.md"))
  net.agent(name: "risk",
            system: fs.read_text("./agents/risk.md"))

  let r = net.run(
    entry:   "geologist",
    message: "Analyse today's events.",
    until:   "DONE",
  )
}
```

</div>
</div>

- **`agent_net`** when schemas are stable: every edge validated against `accept` / `produce`, cycles rejected at parse time, iteration via `until:`.
- **`ai.network`** when the agent set is discovered at runtime (e.g. loaded from a prompt directory). Text-based hand-off: a reply prefixed `>>NAME:` routes to that node.

---

<!-- _class: divider -->

# Verifiability

> The signature is the truth about what a function can do. `cap`, allow-lists, narrowing, enforce modes.

---

<!-- _class: tight -->

# `cap` — a permission, carried as a value

<div class="columns">
<div class="column">

```rust
// A function that reaches the network must declare it.
fn fetch(cap: cap[http.get @ ["api.acme.com"]]) -> result<string> {
  http.get("https://api.acme.com/users")?.body
}

// A function without `cap` cannot reach anything external.
fn total(items: list<Invoice@v1>) -> decimal {
  items.fold(0, fn(acc, it) { acc + it.amount })
}
```

</div>
<div class="column compact">

- `cap` is a **parameter** whose type lists the allowed operations.
- A function with **no `cap`** cannot reach the network, the file system, or the model.
- Allow-lists (`@ ["api.acme.com"]`) restrict *which* endpoints, paths, models are reachable.
- **Pure ⇔ no `cap`** — purity is a *structural* property of the signature, not a keyword.

</div>
</div>

> Aeris applies a well-known idea — *permissions as parameters* — to LLM-generated code. **Not novel research.**

---

<!-- _class: tight -->

# Allow-list — per-family endpoint restriction

<div class="columns">
<div class="column">

```rust
fn settle(items, cap: cap[
  http.post     @ ["api.acme.com", "api.stripe.com"],
  kube.apply    @ ["prod-eu-1"],
  fs.write_file @ ["./out/**"],
  ai.complete   @ ["claude-opus-4-7"],
]) -> result<unit> { ... }
```

| Family | Form |
|---|---|
| `http.*` | hosts |
| `fs.*` | path globs |
| `kube.*` | contexts |
| `ai.*` | models |
| `shell.exec` | `argv0` list |

</div>
<div class="column compact">

- Each effectful operation names its **reachable endpoints**.
- The allow-list is part of the **type** of `cap`.
- A signature outside the project's ceiling is a parse error (exit code 71).
- A reviewer reads the signature and learns which external systems are touched **and which endpoints** are reachable — without entering the body.

</div>
</div>

---

<!-- _class: tight -->

# Narrowing and `main(cap)`

<div class="columns">
<div class="column">

```rust
// Pass a tighter sub-cap to the callee.
settle(batch, cap.subset[
  http.post @ ["api.stripe.com"],
])
```

```text
$ aeris run src/main.aer
[aeris] effective main cap:
  http.{get,post}  @ ["api.acme.com", "api.stripe.com"]
  fs.write_file    @ ["./out/**"]
  kube.{apply,get} @ ["prod-eu-1"]
  ai.complete      @ ["claude-opus-4-7"]
  audit.event
```

</div>
<div class="column compact">

**`cap.subset[...]`**

- Restricts the parent cap; never broadens.
- An endpoint outside the parent is a parse error.

**`main(cap)`**

- Synthesised from `aeris.toml [caps]` at startup.
- The **only way** a `cap` value enters the program.
- Reviewing the manifest = reviewing the whole authority surface.

</div>
</div>

---

# `enforce` — three modes, one grammar

<div class="columns">
<div class="column">

| | `strict` | `loose` | `off` |
|---|---|---|---|
| `main(cap)` from `[caps]` | yes | yes | `cap[*]` |
| `cap` on fn (65) | error | suppressed | suppressed |
| `intent` on write (66) | error | error | suppressed |
| `undo: noop` on write (67) | error | error | suppressed |
| Runtime allow-list | enforced | enforced | bypassed |

</div>
<div class="column compact">

```toml
[caps]
enforce = "strict"   # off | loose | strict
```

- **`off`** — script mode (`aeris init` default).
- **`loose`** — manifest is the runtime ceiling.
- **`strict`** — full discipline.

</div>
</div>

> Modes govern the **static check** only. Trace, replay, schema validation and policy evaluation stay active in all three modes.

---

<!-- _class: divider -->

# Governance & reasoning

> Intent, contracts, policy, trace, supply chain. Non-determinism made explicit, isolated, and governable.

---

# The thesis — controlled non-determinism

> *A small language in which the **visibility** of effects, the **compensation** of external writes, the **integrity** of the supply chain, and the **intent** are structural properties of the source.*

**Three sources of non-determinism**

| Source | Nature | What addresses it |
|---|---|---|
| **The model** | Same prompt, different output | Trace + replay |
| **The grammar** | Ambiguous constructs force the model to guess | One canonical form, reserved keywords |
| **The world** | Networks drop, databases mutate, file systems change | `cap`, `intent`, `policy`, `model@vN` |

> Aeris does not try to *eliminate* non-determinism — it makes it **explicit, isolated and governable**.

---

# From a language for humans to a language for agents (1/2)

> Programming languages were always an interface between **the human mind** and **the machine**. Every design choice — readable syntax, clear error messages, idiomatic style — minimised the cognitive load of the human writing and reading. **That assumption has fallen.**

<div class="columns">
<div class="column compact">

**WHAT, not HOW**

The principal *author* of code is now an LLM. An LLM does not have a mental model — it has a probability distribution over the next token. Writing code is, for an LLM, an **intrinsically stochastic** process.

So the question stops being *"how do I lay out the syntax to be readable?"* and becomes *"what intentions can I let an agent express directly, without encoding them as mechanism?"*

In Aeris, `saga`, `agent`, `intent`, `policy` are not mechanisms — they are **complete intentions** lifted to first-class constructs.

</div>
<div class="column compact">

**High abstraction, not low**

There is an opposite temptation: keep the language *as low as possible*, close to the hardware, so the LLM has less room to fail. **Wrong logic.**

An LLM generates correct code with probability proportional to:

- how much the code **resembles its training corpus**, and
- how **constrained** the space of valid completions is by the language itself.

High abstraction does both: fewer decisions to make → fewer points of failure; higher signal-to-noise per token generated.

</div>
</div>

---

# From a language for humans to a language for agents (2/2)

> Programming languages historically separated **what the code does** (semantics) from **why it does it** (commits, tickets, PR descriptions). The separation was necessary for humans; the machine did not need the *why*.

<div class="columns">
<div class="column compact">

**The cost of that separation**

An LLM reading a `.aer` file *without* the *why* must reverse-engineer purpose from mechanics. **Every inference is a point of non-determinism.**

An agent *executing* code without knowing *why* cannot decide autonomously whether to continue, stop, or escalate when something looks off — it has no acceptance criterion against which to judge unexpected state.

</div>
<div class="column compact">

**Why-as-grammar**

In Aeris the *why* is part of the grammar.

`intent`, `requires:` / `ensures:`, `policy` are **traceable, structurally enforced constructs** that:

- shrink the space of valid interpretations the agent can adopt,
- make the program's purpose **machine-readable**,
- propagate as structured data into the trace, where another agent can consume them.

</div>
</div>

> *The goal is not a language humans write better — it is a language agents **execute with more certainty**.*

---

<!-- _class: tight -->

# `intent` — executable documentation

```rust
intent "monitor API latency, alert above the threshold" {
  every 1m {
    let p99 = http.get("https://metrics/p99").json<f64>()
    if p99 > 500.0 {
      http.post(
        url:  "https://slack/hook",
        body: { text: "High latency: {p99}ms" },
      )
    }
  }
}
```

- A piece of code's *why* exists today only in commits, tickets, PR descriptions — out-of-band channels the agent never sees.
- `intent` brings the *why* **into the grammar**.
- **Mandatory** around every write-effectful call. Lexical check at compile time — exit code 66 when missing.
- The runtime emits `intent_enter` and `intent_exit` events; every nested event carries the active `intent` string.
- Does **not** verify the body matches the string. Makes **omission** impossible, not dishonesty.

---

<!-- _class: tight -->

# `requires:` / `ensures:` — pre and post-conditions

<div class="columns">
<div class="column">

```rust
fn discount(amount: decimal, pct: decimal) -> decimal
  requires: amount >= 0
  requires: pct >= 0 and pct <= 1
  ensures:  result >= 0 and result <= amount
{
  amount * (1 - pct)
}

saga deploy(version: string)
  requires: env.read("DATABASE_URL") != None
  ensures:  http.get("https://prod/health").status == 200
{
  intent "ship release {version}"
  step apply  { do { kube.apply(...)? } undo { kube.delete(...)? } }
  step verify { do { http.wait("/health", timeout: 2m)? } undo { noop } }
}
```

</div>
<div class="column compact">

- `requires:` lists **pre-conditions** — checked at function entry, before any body code runs.
- `ensures:` lists **post-conditions** — checked at every exit path. `result` refers to the returned value.
- Both available on **functions and sagas**.
- A violation produces a fatal `ContractViolation` — **not catchable** with `?` or `catch`. Exit code 64.
- **Runtime contracts** — checked at boundaries, not proved statically.

</div>
</div>

---

<!-- _class: tight -->

# `policy` — declarative governance

<div class="columns">
<div class="column">

```rust
policy production_egress {
  match: http.*
  deny:  url.host not in ["api.acme.com", "api.stripe.com"]
  audit: { url, method }
  when:  env == "production"
}

policy model_budget {
  match: ai.*
  limit: tokens_per_minute = 60_000
  limit: usd_per_day       = 50
}

policy pii_redact {
  match:   ai.*
  require: not contains_pii(prompt)
  deny:    contains_email(response)
}
```

</div>
<div class="column compact">

- `match:` picks which calls the rule applies to.
- `deny:` blocks the call if the condition is true; `require:` blocks if false.
- `limit:` enforces a quota over a window.
- `audit:` adds extra fields to the trace event for matching calls.
- `when:` gates activation on the environment.

> Rules live in the program — not in the system prompt. The model cannot forget them; the runtime evaluates them on every matching call.

</div>
</div>

---

# Trace — what every run records

JSONL stream at `<project>/<output_dir>/traces/<id>.jsonl`, **always on**.
Default `output_dir = ".aeris"`, configurable in `[runtime]`.

```text
$ aeris run main.aer
[aeris] trace_id = 01JFEZH7W… → ./.aeris/traces/01JFEZH7W….jsonl
```

| Source | Recorded fields |
|---|---|
| `ai.*` | `prompt`, `model`, `response`, `tokens`, `latency` |
| `clock.now`, `random.next` | `value` |
| `http.*` | `url`, `method`, `status`, hashes |
| `fs.read_*` / `fs.write_*` | `path`, `len`, `hash` |
| `minio.*`, `mongodb.*`, `rabbitmq.*` | family-specific fields + `backend` |
| `intent`, `saga`, `agent_net`, `policy` | structured events |

```json
{"kind":"ai_call","scope":"classify","model":"claude-opus-4-7",
 "tokens":142,"intent":"classify the invoice","ts":"..."}
```

> Trace IDs are propagated across HTTP via `X-Aeris-Trace-Id: <id>` — a single request stays contiguous across processes. The trace path resolves against the **project root** (`main.aer`'s directory), so `cd ~ && aeris run /path/to/demo/main.aer` writes to the demo, not to `$HOME`.

---

<!-- _class: tight -->

# Replay and bisect — `aeris replay`, `aeris trace diff`

<div class="columns">
<div class="column">

```text
$ aeris replay 01JFE...
[aeris] replaying from trace 01JFE...
[aeris] ai.complete  → recorded response
[aeris] clock.now    → recorded value
[aeris] ✓ bit-identical on deterministic subset

$ aeris replay 01JFE... --live
[aeris] live HTTP for http.*; trace for clock/random/ai

$ aeris trace diff 01JFE... 01JG0...
@ ai_call[classify]:
  response: "{\"kind\":\"utilities\"}"
       !=   "{\"kind\":\"software\"}"
```

</div>
<div class="column compact">

**`aeris replay <id>`**

- `ai.*` returns the recorded response (no model call).
- `clock.now`, `random.next` emit recorded values.
- `http.*` replays fixtures (default) or hits live (`--live`).
- **Bit-identical** on the deterministic subset.

**`aeris trace diff`**

- Aligns events by `(scope, ordinal)` and reports field-level differences.
- Foundation for regression **bisect**.

</div>
</div>

---

<!-- _class: tight -->

# External libraries — content-addressed supply chain

<div class="columns">
<div class="column">

```rust
use deploy
  from "github.com/acmecorp/aeris-devops"
       deploy@"1.2.0"
```

```toml
[deps]
deploy = { source  = "github.com/acmecorp/aeris-devops",
           version = "1.2.0",
           hash    = "blake3:7e2c...c1a4" }
```

</div>
<div class="column compact">

- Each dependency is identified by the **blake3 hash** of its bytes.
- If the fetched bytes do not match the hash, the run fails **before any code from the dep executes**.
- No `latest`, no `*`, no movable Git tags — the version answer is always in `aeris.toml`.
- External libs are always `.aer` source. No `.so` / `.dll` to load.

> Same content-addressing approach already used by Cargo, npm, Nix.

</div>
</div>

---

<!-- _class: tight -->

# Manifest and lock file — `aeris.toml`, `surface.lock`

<div class="columns">
<div class="column">

```toml
# aeris.toml — single project reference
[project]
name  = "settle-pipeline"
aeris = "0.3.0"

[caps]
enforce         = "strict"
http.allow      = ["api.acme.com"]
kube.contexts   = ["prod-eu-1"]
ai.models       = ["claude-opus-4-7"]

[ai.backend]
kind = "http"
url  = "https://api.anthropic.com"
auth = "env:ANTHROPIC_API_KEY"
```

</div>
<div class="column">

```toml
# .aeris/surface.lock — produced by `aeris lock surface`
[surface."src/invoices.aer".settle]
caps       = ["http.post", "kube.apply", "audit.event"]
allow.http = ["api.acme.com"]
```

- **`[caps]`** is the project-wide ceiling on authority.
- **`[ai.backend]`** picks where AI calls go — HTTP API or local CLI process.
- `surface.lock` has one entry per `pub fn`. A PR that **broadens** any surface must regenerate the lock — the diff is the first hunk in review.

</div>
</div>

---

<!-- _class: divider -->

# Putting it together

> An end-to-end SRE alert triage system — typed agents, runtime policy, compensating saga, every-loop driver.

---

<!-- _class: tight -->

# End-to-end — SRE alert triage (1/2)

```rust
model Alert@v1     { id: uuid, service: string, message: string }
model Diagnosis@v1 {
  severity:   string  where severity in ["critical","high","medium","low"]
  kind:       string  where kind in ["database","api","infrastructure"]
  confidence: f64     where confidence >= 0.0 and confidence <= 1.0
}
model FixPlan@v1   { commands: list<string>, rollback: list<string> }

agent classify {
  llm:     "claude-haiku-4-5"
  accept:  Alert@v1
  produce: Diagnosis@v1
  prompt:  "Classify alert {input.message} on {input.service}."
}

agent plan {
  llm:     "claude-opus-4-7"
  accept:  Diagnosis@v1
  produce: FixPlan@v1
  prompt:  "Propose a fix and rollback for a {input.severity} alert."
}

agent_net triage {
  flow classify -> plan
  until: classify.confidence > 0.85 || iterations >= 3
}
```

> A typed graph of agents. Each edge is validated against `accept` / `produce`. A model hallucination producing out-of-shape JSON is rejected by the schema check, not by the reviewer.

---

<!-- _class: tight -->

# End-to-end — SRE alert triage (2/2)

```rust
saga apply_fix(plan: FixPlan@v1, alert: Alert@v1) {
  intent "apply fix for alert {alert.id}"
  step snapshot {
    do   { shell.exec("kubectl get all > /tmp/{alert.id}.yaml") }
    undo { shell.exec("rm -f /tmp/{alert.id}.yaml") }
  }
  step apply {
    requires: snapshot.ok
    do   { for c in plan.commands { shell.exec(c)? } }
    undo { for c in plan.rollback { shell.exec(c)? } }
  }
}

every 30s {
  let alerts = json.decode<list<Alert@v1>>(
    http.get("https://alertmanager/api/alerts")?.body)?
  for a in alerts { apply_fix(triage(a)?, a)? }
}
```

> One file: typed agents (previous slide), a saga with compensation, a 30-second poll loop. AI orchestration, runtime governance, compensation, scheduling — all in one grammar.

---

# Error model — layered exit codes

<div class="columns">
<div class="column">

| Phase | Failure | Exit |
|---|---|---|
| Lex / Parse | malformed syntax | 1 |
| Static check | type / contract | 64 |
| Static check | `cap` missing / over-broad | 65 |
| Static check | missing `intent` on write | 66 |
| Static check | saga step `undo: noop` on write | 67 |

</div>
<div class="column">

| Phase | Failure | Exit |
|---|---|---|
| Static check | `model` without `@vN` on boundary | 68 |
| Static check | dep hash mismatch | 69 |
| Static check | `agent_net` cycle | 70 |
| Static check | allow-list over ceiling | 71 |
| Static check | module reference without `use` | 72 |
| Runtime | `saga` `PartialFailure` | 74 |

</div>
</div>

> The static check produces **distinct exit codes** so CI can react differently. A failed `intent` check (66) is not the same kind of failure as a missing-undo (67) or a bad model version (68).

---

# Honest limits

- **First model run stays non-deterministic.** Replay is reproducibility *after* the first run.
- **In-body correctness inside a legitimate `cap` is not verified** — tests, property checks, backend RBAC.
- **Cap over-broadening is a process problem** — the `surface.lock` diff makes it visible; CI enforces.
- **Cascading undo is best-effort** — `PartialFailure` (exit 74) when retries exhaust.

> Aeris is the **first defensive layer**, not the only one.

---

# What Aeris refuses on principle

- **No automatic formal proofs** — verdicts that depend on the machine and on the solver's heuristics.
- **No automatic inference of capabilities** — the signature must be the truth; hidden changes break PR review.
- **No mutable dependency references** — no `latest`, no `*`, no movable Git tags.
- **No native runtime plug-ins** — would add an effect surface the static checker cannot see.

> Every refusal pays a **declared cost** — accepted to keep what is in the source the truth.

---

<!-- _class: divider -->

# Thanks

> Aeris is an open project.
> Questions, feedback, contributions welcome.

> Sources of truth: `docs/thesis.md`, `docs/language.md`, `docs/project.md`, `docs/plan.md`, `docs/cheatsheet.md`.
