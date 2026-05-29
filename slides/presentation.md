---
marp: true
theme: aeris
paginate: true
html: true
size: 16:10
title: "Aeris v0.3 — a language for the era of generated code"
header: 'Aeris v0.3 · technical overview'
footer: 'Aeris v0.3 · a small interpreted language where effects, compensation and intent are structural properties of the source'
---


<style>
  /* Theme default is 36px; prose bumped modestly; code blocks
     pinned so the bump doesn't overflow the canvas. */
  section { font-size: 40px; line-height: 1.3; }
  section pre,
  section pre code { font-size: 26px; line-height: 1.45; }
  section li > ul { margin-top: 0.15em; }
  figure.aeris-figure {
    margin: 0.3em auto;
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

<p class="eyebrow">Technical overview · v0.3</p>

## AERIS

A small interpreted language.

> *"Aeris does not aim to prove code correct. It makes the rationally suspicious code physically incapable of hiding."*

---

# What we'll cover

| # | Section | What it covers |
|---|---|---|
| **1** | **Aeris at a glance** | the problem, the shape, the four layers |
| **2** | **How an interpreted language works** | lexer → parser → AST → tree-walk |
| **3** | **L1 · The core language** | values, functions, pattern matching, models, libraries, automation |
| **4** | **L2 · Verifiability & governance** | `cap`, `intent`, contracts, `policy`, supply chain |
| **5** | **L3 · Reversible execution** | `saga`, trace & replay |
| **6** | **L4 · AI & agents** | `ai.*`, `agent`, `agent_net` |
| **7** | **The theory behind Aeris** | trilemma, *what*-not-*how*, why-as-grammar |

---

<!-- _class: divider -->

<p class="eyebrow">Section 1</p>

# Aeris at a glance

> Why it exists, what it looks like, the four layers it stacks, and how it differs from tools you already know.

---

# Aeris

> A **personal experiment** — let an **LLM** build a language.

- A small **interpreted language**
  - target: **AI · automation · operations**
  - one `.aer` file
  - **written in Rust**, single static binary **< 8 MB**
  - no JVM · no Python · no container

- **Familiar surface, native constructs**
  - reads like **Rust / Go / Swift / TS**
  - plus `saga` · `agent` · `policy` · `intent` · `model` · `cap`

---

# Built **docs-first**

- **Four authoring files** — written before any code
  - **`thesis.md`** — the *rationale*
  - **`language.md`** — the *spec*
  - **`project.md`** — the *constraints*
  - **`plan.md / plan.json`** — **~45 milestones** ([→ full plan](plan.html))

- **The loop**
  - **read** the plan
  - **for each task** in the next milestone
    - **LLM implements** the code
    - **verify** — acceptance check + functional tests
    - **mark done** → next task

---

# Change of perspective

> The author of code is now an **LLM** — no mental model, just the next token. Human (still) in the loop.

- **Two problems** the language must answer

  1. **Opacity** — *what side effects does this function actually perform?*
     - a leaf helper does `http.post()` **6 frames deep**
     - `grep -r http` misses **wrapped SDK clients**
     - signatures (`fn f(x: User) -> Order`) say **nothing about I/O**
     - **code review** can't enforce what the type system can't see

  2. **Non-determinism** — *will the same input produce the same output?*
     - **LLM** — sampling, temperature, model version drift
     - **runtime** — `clock.now()`, `uuid4()`, `os.urandom()`, RNG seeds
     - **world** — HTTP responses, DB rows, file mtimes, env vars
     - flaky tests → **retries hide the root cause**

> Aeris's goal: **every effect declared in the signature** + **every run reproducible from the trace**.

---

# What today's defences miss

> Opacity: **which function can touch which resource?**

| Tool | What it **checks** | What it **misses** |
|---|---|---|
| **Sandboxing** (Docker, gVisor) | the **outside** of the program — what the whole process can do | what happens **inside** — any function can still call any API |
| **Static types** (TS, Java, Rust) | the **shape** of data (`string`, `number`, `list`) | what the code **does** with that data — read a file? call HTTP? |
| **Effect systems** (F\*, Dafny, Koka) | the **effects**, with mathematical proof | usable only by **experts** — too complex for everyday teams |
| **Frameworks** (Airflow, Temporal) | the **order** of steps and the retries | what each step **actually does** inside its body |

> Aeris answers **in the signature** — never read the body.

---

<!-- _class: tight -->

# The four layers

> Aeris stacks **four layers**, each building on the one below. They are the frame for everything that follows — and the rest of this talk visits them in order.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 360" role="img" aria-label="The four layers stacked">
<defs>
<marker id="L4" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="20" y="15" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="45" font-size="22" font-weight="700" fill="#0E1020">L1 — AI-native syntax</text>
<text x="40" y="72" font-size="16" fill="#5F6470">one canonical form, all keywords reserved</text>
<line x1="300" y1="85" x2="300" y2="100" stroke="#1C2035" stroke-width="2.5" marker-end="url(#L4)"/>
<rect x="20" y="100" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="130" font-size="22" font-weight="700" fill="#0E1020">L2 — Verifiable semantics</text>
<text x="40" y="157" font-size="16" fill="#5F6470">capabilities-as-values, contracts, intent</text>
<line x1="300" y1="170" x2="300" y2="185" stroke="#1C2035" stroke-width="2.5" marker-end="url(#L4)"/>
<rect x="20" y="185" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="215" font-size="22" font-weight="700" fill="#0E1020">L3 — Reversible execution</text>
<text x="40" y="242" font-size="16" fill="#5F6470">saga with do/undo, trace &amp; replay</text>
<line x1="300" y1="255" x2="300" y2="270" stroke="#1C2035" stroke-width="2.5" marker-end="url(#L4)"/>
<rect x="20" y="270" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="300" font-size="22" font-weight="700" fill="#0E1020">L4 — Multi-agent orchestration</text>
<text x="40" y="327" font-size="16" fill="#5F6470">typed agent_net, schema at every edge</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

- **L1 · syntax**
  - dense, unambiguous
  - → **fewer wrong guesses**
- **L2 · semantics**
  - `cap` · `intent` · contracts · `policy`
  - **compiler** catches stray effects
- **L3 · reversible execution**
  - `saga`, trace & replay
  - **clean recovery** from any failure
- **L4 · multi-agent**
  - `ai.*` · `agent` · `agent_net`
  - routing = **typed graph**

> **Opt in by depth** — script: L1; production: all four.

</div>
</div>

---

<!-- _class: divider -->

<p class="eyebrow">Section 2</p>

# How an interpreted language works

> How Aeris turns source text into a syntax tree, then evaluates that tree directly.

---

# From source to behaviour

> A compiler emits machine code to run later. An **interpreter** executes the program *directly* by walking its structure. Aeris is a **tree-walking interpreter** shipped as one static binary.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 560 380" role="img" aria-label="Interpreter pipeline: source to tokens to AST to evaluator to effects">
<defs>
<marker id="pp" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="120" y="8"   width="320" height="56" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="280" y="34" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">source .aer</text>
<text x="280" y="55" text-anchor="middle" font-size="14" fill="#5F6470">UTF-8 text</text>
<line x1="280" y1="64" x2="280" y2="84" stroke="#1C2035" stroke-width="2.5" marker-end="url(#pp)"/>
<rect x="120" y="88"  width="320" height="56" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="280" y="114" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">lexer → tokens</text>
<text x="280" y="135" text-anchor="middle" font-size="14" fill="#5F6470">keywords, literals, operators</text>
<line x1="280" y1="144" x2="280" y2="164" stroke="#1C2035" stroke-width="2.5" marker-end="url(#pp)"/>
<rect x="120" y="168" width="320" height="56" rx="10" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="280" y="194" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">parser → AST</text>
<text x="280" y="215" text-anchor="middle" font-size="14" fill="#5F6470">+ static checks: caps, intent, cycles</text>
<line x1="280" y1="224" x2="280" y2="244" stroke="#1C2035" stroke-width="2.5" marker-end="url(#pp)"/>
<rect x="120" y="248" width="320" height="56" rx="10" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="280" y="274" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">tree-walk evaluator</text>
<text x="280" y="295" text-anchor="middle" font-size="14" fill="#5F6470">visit each node, in source order</text>
<line x1="280" y1="304" x2="280" y2="324" stroke="#1C2035" stroke-width="2.5" marker-end="url(#pp)"/>
<rect x="120" y="328" width="320" height="48" rx="10" fill="#0E1020" stroke="#1C2035" stroke-width="2"/>
<text x="280" y="357" text-anchor="middle" font-size="19" font-weight="700" fill="#F6F3F0">effects + JSONL trace</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

**Three stages**

- **Lexer** → tokens
  - keywords reserved → no positional meaning
- **Parser** → AST
  - **static checks here**: caps, intent, cycles
- **Evaluator** → effects
  - one recursive **walk** over the tree

> **Parse-time** = verify · **walk-time** = trace.

</div>
</div>

---

<!-- _class: tight -->

# Tree-walking, concretely

> Each node is either a **statement** (changes the environment) or an **expression** (returns a value). Evaluation is a `depth-first walk`: children first, then the parent.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 540 360" role="img" aria-label="AST walk for let x = add(2, 3)">
<defs>
<marker id="aa" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#5F6470"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="10" y="10" width="520" height="36" rx="4" fill="#F6F3F0" stroke="#1C2035" stroke-width="1"/>
<text x="22" y="34" font-family="JetBrains Mono, monospace" font-size="18" fill="#0E1020">let x = add(2, 3)</text>

<rect x="180" y="70" width="180" height="46" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="270" y="99" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Let("x", _)</text>
<circle cx="168" cy="93" r="14" fill="#1C2035"/>
<text x="168" y="98" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">1</text>
<text x="372" y="88" font-size="13" font-style="italic" fill="#5F6470">effect:</text>
<text x="372" y="107" font-family="JetBrains Mono, monospace" font-size="13" font-weight="700" fill="#0E1020">env { x: 5 }</text>

<line x1="270" y1="116" x2="270" y2="160" stroke="#5F6470" stroke-width="2" marker-end="url(#aa)"/>

<rect x="180" y="165" width="180" height="46" rx="8" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="270" y="194" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Call("add", _)</text>
<circle cx="168" cy="188" r="14" fill="#1C2035"/>
<text x="168" y="193" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">2</text>
<text x="372" y="183" font-size="13" font-style="italic" fill="#7C3AED">returns:</text>
<text x="372" y="202" font-family="JetBrains Mono, monospace" font-size="14" font-weight="700" fill="#7C3AED">Value(5)</text>

<line x1="225" y1="211" x2="135" y2="258" stroke="#5F6470" stroke-width="2" marker-end="url(#aa)"/>
<line x1="315" y1="211" x2="405" y2="258" stroke="#5F6470" stroke-width="2" marker-end="url(#aa)"/>

<rect x="65" y="263" width="110" height="46" rx="8" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="120" y="292" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Lit(2)</text>
<circle cx="53" cy="286" r="14" fill="#1C2035"/>
<text x="53" y="291" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">3</text>
<text x="120" y="335" text-anchor="middle" font-size="13" font-style="italic" fill="#7C3AED">↑ Value(2)</text>

<rect x="365" y="263" width="110" height="46" rx="8" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="420" y="292" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Lit(3)</text>
<circle cx="487" cy="286" r="14" fill="#1C2035"/>
<text x="487" y="291" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">4</text>
<text x="420" y="335" text-anchor="middle" font-size="13" font-style="italic" fill="#7C3AED">↑ Value(3)</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

**Reading the walk**

- **Function call** = sub-walk on the callee
  - push a scope, bind params, recurse
- `return` / `break` / `continue` = **unwind**

**Why it matters for Aeris**

- The walk is where **effects are recorded**
  - `cap.*` · `clock.now` · `random.next` · `ai.*`
- `aeris replay` = **same tree, against the tape**
  - same run, **offline**

</div>
</div>

---

<!-- _class: divider -->

<p class="eyebrow">Section 3 · L1</p>

# The core language

> The conventional foundation: values and types, functions and control flow, versioned schemas — and the libraries you import.

---

<!-- _class: tight -->

# Surface, values, types

> Expressions everywhere, immutable by default, **one construct per concept**. Every keyword is reserved.

<div class="columns">
<div class="column">

```rust
use io, json, fs, kube, docker   // stdlib + native

const PI    = 3.14159     // module-level, folded
let   RATE  = 0.02        // module-level let (v0.3)
type  Email = string      // type alias (pure rename)

record User {
  id:    uuid
  name:  string
  email: Email
  age:   int   where age >= 0   // checked at construction
  joined: timestamp
}

let bob  = User { id: uuid_v7(), name: "Bob",
                  email: "bob@acme.com",
                  age: 30, joined: now() }
let bob2 = User { ..bob, age: 31 }    // structural update

enum Status {
  Pending                              // unit variant
  Active(since: timestamp)             // positional
  Banned { reason: string }            // named record
}

fn first<T>(xs: list<T>) -> option<T>  // generic
  { if xs.empty() { None } else { Some(xs[0]) } }

let sign = if RATE > 0 { "+" } else { "-" }   // blocks = values
```

</div>
<div class="column compact">

- **Primitives** — first-class literals
  - `decimal` · `uuid` · `date` · `timestamp` · `duration`
  - `2026-05-07`, `500ms` — recognised by lexer
- **Records** — immutable, **by-value**
  - **structural update** via `..u`
- **Enums** — 3 variant shapes
  - unit · positional · named record
- **Type aliases** — `type Email = string`
- **Generics** — parametric, no bounds
- **Bindings** — `let` · `var` · `const`
  - `var` = **function-scope only**

</div>
</div>

---

<!-- _class: tight -->

# Functions, control flow, errors

> The signature tells the whole truth about a function. Errors are **values** — passed up with `?`, never hidden.

<div class="columns">
<div class="column">

```rust
use http, io                // L1 stdlib

fn discount(amount: decimal, pct: decimal) -> decimal
  requires: amount >= 0
  requires: pct >= 0 and pct <= 1
  ensures:  result >= 0 and result <= amount
{
  amount * (1 - pct)              // pure: no `cap`
}

fn refund(id: uuid,
          cap: cap[http.post @ ["api.acme.com"]])
  -> result<unit>
{
  intent "refund {id}" {
    if id == nil { raise error("missing id") }
    defer io.println("refund {id} done")    // LIFO at exit
    let r = retry 3, delay: 1s {            // bounded retry
      http.post("/refund", { id })
    } catch err {                           // inline recover
      return Err(err)
    }
    if r.status != 200 { raise error("HTTP {r.status}") }
    Ok(())
  }
}

let h1 = spawn { refund(o1, cap.subset[...]) }   // concurrency
let h2 = spawn { refund(o2, cap.subset[...]) }
let (r1, r2) = (await h1, await h2)
```

</div>
<div class="column compact">

- **Purity is structural**
  - **no `cap` ⇒ no side effects**
- **Contracts**
  - `requires:` at entry · `ensures:` at return
- **Errors are values** — `result<T> = Ok(T) | Err(err)`
  - `?` propagates · `catch` recovers · `raise` short-circuits
- **Time control** (v0.3)
  - `every` · `retry` · `timeout` · `defer`
- **Control flow** — `if` · `match` · `for` · `while` · `loop`
- **Concurrency** — `spawn { ... }` · `await`

</div>
</div>

---

<!-- _class: tight -->

# Pattern matching & collections

> `match` is exhaustive and destructures; standard containers cover the rest. A missing case is a **parse error**, not a runtime surprise.

<div class="columns">
<div class="column">

```rust
let xs: list<int> = [1, 2, 3, 4, 5]
let m    = { "a": 1, "b": 2 }      // map
let pair = ("ok", 200)              // tuple
let opt: option<string> = Some("hi")
let res: result<int>    = Ok(42)

let label = match status {
  Pending                       -> "waiting",
  Active(t) if t < deadline     -> "expired",         // guard
  Active(_)                     -> "live",
  Banned { reason: "spam", .. } -> "blocked: spam",   // partial
  Banned { .. }                 -> "blocked",
}

let summary = match xs {
  []                  -> "empty",
  [x]                 -> "one: {x}",
  [first, .., last]   -> "range {first}..{last}",     // list pattern
}

let head = xs.first() ?? 0          // option fallback
let body = http.get(url)? catch err { "" }    // inline recover

if res is Ok(v) { use(v) }          // refinement check
for (k, v) in m { io.println("{k}={v}") }
```

</div>
<div class="column compact">

- **`match`** — **exhaustive**
  - patterns: literal · binder · enum · tuple · list · record
  - optional **`if` guard**
- **List patterns** — `[]` · `[x]` · `[x, ..rest]` · `[first, .., last]`
- **`is` / `as`** — refinement check / coercion
- **Collections** — `list` · `set` · `map` · `tuple`
  - plus `option<T>` · `result<T>`
- **`??`** — fallback for `None` / `Err` / `()`
- **Methods** — `.first()` · `.empty()` · `.map(f)` · `.contains(x)` · `.len()`

</div>
</div>

---

<!-- _class: tight -->

# Models — versioned trust-boundary schemas

> A `model` validates **untrusted data at the door**: LLM reply, HTTP body, queue message.

<div class="columns">
<div class="column">

```rust
model Invoice@v1 {
  id:       uuid
  amount:   decimal  where amount > 0
  customer: string   where len(customer) <= 64
}

// schema evolves → bump the tag
model Invoice@v2 extends Invoice@v1 {
  currency: string
    where currency in ["EUR", "USD"]
}

// explicit bridge between versions
fn migrate_v1_to_v2(
  old: Invoice@v1,
) -> Invoice@v2 { ... }
```

</div>
<div class="column compact">

- **`@vN` = version of the schema shape**
  - same idea as `/api/v1` · Avro · Protobuf
  - bump it when **fields change** or **constraints evolve**
- **`v1` ≠ `v2` — they are distinct types**
  - no implicit conversion
  - **explicit migration** function bridges them
- **`where` runs at every boundary**
  - construction · `json.decode` · agent edge · HTTP / queue ingress
- **Bad shape** → `SchemaViolation`, rejected **at the door**

> LLM output forced into a **known, versioned shape**.

</div>
</div>

---

<!-- _class: tight -->

# Libraries — three tiers

> Everything a program can touch comes in through `use`. Each import sits in one of three tiers, with rising trust requirements. *(These tiers are about where code comes from — distinct from the four architectural layers.)*

<div class="columns">
<div class="column">

```rust
use io, json, fs, http        // stdlib
use ai, kube, docker          // native modules
use deploy from
  "github.com/acme/devops" deploy@"1.2.0"  // external
```

</div>
<div class="column compact">

- **stdlib** — built in
  - **12 modules**, frozen registry
  - `io` · `fs` · `http` · `shell` · `env` · `clock`
  - `random` · `strings` · `date` · `json` · `yaml` · `net`
- **native modules** — signed `.so` / `.dylib`
  - `ai` · `kube` · `docker` · `mongodb` · `minio` · `rabbitmq` · `audit`
  - **blake3-pinned** + **registry-signed**
- **external libraries** — third-party `.aer`
  - **blake3 hash** in `aeris.toml`
  - **no** `latest` · `*` · movable tags

</div>
</div>

---

<!-- _class: tight -->

# `pipeline` — automation that rolls forward, fully traced

> When the right move on failure is **roll forward, not back**, a `pipeline` runs ordered steps over `docker` / `kube` / `http`, **stops on the first error**, and **tapes every stage** — the lighter sibling of `saga` for deploys and ops.

<div class="columns">
<div class="column">

```rust
use docker, kube, http, io

pipeline Deploy(
  version: string,
  cap: cap[
    docker.build, docker.push,
    kube.apply @ ["prod-eu-1"],
    http.get   @ ["prod.acme.com"],
  ],
) {
  intent "roll a tagged build out to prod-eu-1"   // mandatory
  steps:
    | "build":  docker.build(".", "app:{version}") as img
    | "push":   docker.push("registry/app:{version}")
    | "apply":  kube.apply("./k8s/prod-eu-1.yaml")
    | "health": http.get("https://prod.acme.com/health")

  on_step:    fn(name, result) { io.println("[{name}] ok") }
  on_failure: io.println("stopped at {last_step}: {last_error}")
}

Deploy.run(version: "1.4.2") catch err { alert(err) }
Deploy.run(version: "1.4.2", on_error: "continue")  // roll past
```

</div>
<div class="column compact">

- **Ordered steps** — labelled or anonymous
  - `as` binds a result for later steps
- **Stops on first error** → `on_failure`
  - or `on_error: "continue"` to roll past
- **Still `cap`-checked + `intent`-gated**
  - the writes are **declared, never hidden**
- **Every stage taped**
  - `pipeline_enter` · `step_exit` · `pipeline_exit`
  - idempotency key per step → **safe re-run**

> **`saga` vs `pipeline`** — undo & roll **back** · vs · trace & roll **forward**.

</div>
</div>

---

<!-- _class: divider -->

<p class="eyebrow">Section 4 · L2</p>

# Verifiability & governance

> The structural core: `cap` · `intent` · contracts · `policy` · supply chain. **Verified at parse time, enforced at run time.**

---

# Structural, not semantic

> Don't *prove* code correct — make the **suspicious code unable to hide**.

- **Parse-time** — visible in the source
  - **what** a function touches → `cap`
  - **why** → `intent` (mandatory on writes)

- **Runtime** — declared, enforced
  - **invariants** → `requires` / `ensures`
  - **guardrails** → `policy` (`deny` · `limit` · `require`)

- **Out of scope**
  - logic *inside* a legitimate `cap`
  - tests · review · backend RBAC

---

<!-- _class: tight -->

# Capabilities are values

> **Authority is a value you pass as a parameter, not something the whole process holds.** Hold the value and you can make the call; without it you can't — *the code won't even parse*.

<div class="columns">
<div class="column">

```rust
// pure: no cap ⇒ cannot do IO at all
fn total(items: list<Invoice@v1>) -> decimal {
  var sum: decimal = 0
  for it in items { sum += it.amount }
  sum
}

// authority is on the signature, with allow-lists
fn settle(
  batch: list<Invoice@v1>,
  cap: cap[
    http.post  @ ["api.acme.com"],
    kube.apply @ ["prod-eu-1"],
    audit.event,
  ],
) -> result<unit>
```

</div>
<div class="column compact">

- **Signature = contract**
  - "what does this touch?" **without reading the body**
- **`cap` cannot escape**
  - no fields · no return · no channels
  - `cap[*]` **forbidden** in user code

**Enforcement** — project decision
  - `off` · `loose` · `strict` in `aeris.toml`

**Lineage — object-capability security**

- **Dennis & Van Horn, 1966** — coined it
- **Mark Miller's E**, late 1990s — first practical use
- Capsicum · Genode · Pony — *applied engineering*

</div>
</div>

---

# `intent` — the *why* in the grammar

> The *why* lives in commits and tickets — **the model never sees them**.

```rust
intent "rotate the leaked TLS cert" {
  fs.write_file(cert_path(), new_cert)?
  audit.event("cert.rotated", { path: cert_path() })
}
```

- **Mandatory on every write**
  - `fs.write_*` · `http.post` · `kube.apply` · `audit.event` · `ai.*`
  - without it → **won't parse**
- **Flows into the trace**
  - emits `intent_enter` / `intent_exit`
  - every inner event **carries the intent**
- **Checks presence, not truth**
  - LLM PR can't hide a write **in silence**

---

# Contracts — `requires` · `ensures` · `where`

> **Runtime** checks. **Not** solver proofs.

- **Where they live**
  - `requires:` — at **entry**
  - `ensures:` — at **every return** (`result` = the value)
  - `where` — on **fields** and **`match` arms**

- **On violation**
  - `ContractViolation` — **fatal**, not catchable by `?`
  - logged · **exit code 64**

---

<!-- _class: tight -->

# `policy` — runtime guardrails the model can't forget

> Guardrails **run on every matching call**, instead of being "remembered" in a system prompt. They use the same capability paths as signatures — no separate mini-language.

<div class="columns">
<div class="column">

```rust
policy production_egress {
  match: http.*
  deny:  url.host not in ["api.acme.com"]
  audit: { url, method }
}

policy model_budget {
  match: ai.*
  limit: tokens_per_minute = 60_000
  limit: usd_per_day = 50
}
```

</div>
<div class="column compact">

- **Clauses**
  - `match:` — which **cap paths**
  - `deny:` / `require:` → `PolicyViolation`
  - `limit:` — quota over a window
  - `audit:` — extra trace fields
- **Activation**
  - on import · per `fn` · `aeris.toml`
- **Recorded**
  - live ≠ replay → `policy_drift` event

> Defense against **malicious LLM injection**.

</div>
</div>

---

# Content-addressed supply chain

> Every external dependency is identified by **the hash of its bytes**.

```toml
[deps]
deploy = { source = "github.com/acmecorp/aeris-devops",
           version = "1.2.0", hash = "blake3:7e2c...c1a4" }
```

- **Hash = identity**
  - aliases bound to **blake3** hashes
  - mismatch → **fatal before any code runs**

- **No `latest` · no `*` · no movable tags**
  - "what's in this build?" → read `aeris.toml`

- **Native modules**
  - also **ed25519-signed** (Aeris registry)
  - verified **before load**

> **Lineage:** Nix store · Cargo lock · Go `GOSUMDB`.

---

<!-- _class: divider -->

<p class="eyebrow">Section 5 · L3</p>

# Reversible execution

> How **a single program** acts on the world — *do the work, record it, undo on failure*. Every write step is **reversible**, every run **replayable**.

---

<!-- _class: tight -->

# What reversible execution means

> A program doing real work on the outside world runs the same cycle on **every write step**: do it, record it, and undo it if a later step fails.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 300" role="img" aria-label="Reversible execution cycle: do a step, record it; on success move to the next step, on failure undo the completed steps">
<defs>
<marker id="lp" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
<marker id="lpf" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#FF7E51"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<path d="M380,70 C380,24 120,24 120,70" fill="none" stroke="#1C2035" stroke-width="2" marker-end="url(#lp)"/>
<text x="250" y="17" text-anchor="middle" font-size="14" font-style="italic" fill="#5F6470">step ok → next</text>
<rect x="40" y="70" width="160" height="52" rx="10" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="120" y="93" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">1 · do the step</text>
<text x="120" y="113" text-anchor="middle" font-size="13" fill="#5F6470">act on the world</text>
<rect x="300" y="70" width="160" height="52" rx="10" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="380" y="93" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">2 · record it</text>
<text x="380" y="113" text-anchor="middle" font-size="13" fill="#5F6470">one trace event</text>
<line x1="200" y1="96" x2="300" y2="96" stroke="#1C2035" stroke-width="2" marker-end="url(#lp)"/>
<line x1="380" y1="122" x2="380" y2="215" stroke="#FF7E51" stroke-width="2" marker-end="url(#lpf)"/>
<text x="392" y="172" text-anchor="start" font-size="14" font-style="italic" fill="#D14600">fails</text>
<rect x="300" y="215" width="160" height="52" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="380" y="238" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">3 · undo</text>
<text x="380" y="258" text-anchor="middle" font-size="13" fill="#5F6470">reverse the done steps</text>
<text x="380" y="290" text-anchor="middle" font-size="13" font-style="italic" fill="#5F6470">→ rolled_back · PartialFailure</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

**Why the language must help**

- Long-running work on the outside world **will fail**
  - dropped network · bad reply · half-applied change
- By hand → **half-done mess**

**What this layer guarantees**

- Every run ends **defined**
  - fully done, or fully rolled back
- Recording → **exact offline replay**

> **Two constructs:** `saga` + trace & replay.

</div>
</div>

---

<!-- _class: tight -->

# Sagas — reversible by construction

> The **only** place you can make multi-step writes to external state. Every `step` declares **`do` and `undo`** — and the compiler rejects a write whose `undo` is `noop`.

<div class="columns">
<div class="column">

```rust
saga settle(
  batch: list<Invoice@v1>,
  cap: cap[
    http.post  @ ["api.acme.com"],
    kube.apply @ ["prod-eu-1"],
    audit.event,
  ],
) {
  intent "settle invoice batch and notify finance"
  step charge {
    do   { http.post("/charge", batch)? }
    undo { http.post("/refund", batch)? }
  }
  step ledger {
    requires: charge.ok
    do   { kube.apply(manifest(batch))? }
    undo { kube.delete(manifest(batch))? }
  }
  step notify {
    requires: ledger.ok
    do   { audit.event("settle.done") }
    undo { audit.event("settle.undone") }
  }
}
```

</div>
<div class="column compact">

- **Failure → compensation**
  - failed step → **undo completed in reverse**
  - rollback fails → honest **`PartialFailure`**
- **Idempotency keys**
  - `blake3(trace_id, step, idx)` — auto-injected
  - **safe replay** of a half-done saga

**Lineage — the SAGA pattern**

- **Garcia-Molina & Salem, 1987** (SIGMOD)
  - DBs without long transactions
  - today: **Temporal** · Step Functions
- Aeris: **compensation in the syntax**, not optional

</div>
</div>

---

<!-- _class: tight -->

# Trace & replay — capture, not control

> We do **not** promise deterministic LLM code — that is physically false. We promise **reproducibility after the first run**.

<div class="columns">
<div class="column">

```json
{ "kind": "saga_enter", "saga": "settle",
  "intent": "settle invoice batch..." }
{ "kind": "ai_call", "model": "claude-opus-4-7",
  "tokens": 412, "resp_hash": "b3:9f2c..." }
{ "kind": "step_exit", "step": "charge",
  "outcome": "ok" }
```

```text
$ aeris replay 01JFE...
  ai.*        → recorded response, no network
  clock.now   → recorded value
  random.next → recorded value
```

</div>
<div class="column compact">

- **Every run → JSONL trace**
  - one event per line
  - each carries the **active `intent`**
- **Always on**
  - `ai.*` · `clock.now` · `random.next`
  - **no opt-in** in production
- **`aeris replay` re-walks the tree**
  - **bit-identical** for the deterministic part
  - the rest **fixed by the tape**
- **All offline**
  - audit · debug · regression · post-mortem

</div>
</div>

---

<!-- _class: tight -->

# What gets recorded

> Every non-deterministic read and every external write is taped — enough to **replay the run offline**, not so much that secrets leak.

| Source | Recorded into the trace |
|---|---|
| `ai.*` | prompt, model, response, tokens, latency |
| `clock.now` · `random.next` | the value returned |
| `clock.sleep` | the recorded duration (returns instantly on replay) |
| `http.*` | url, method, status, request & response **hash** |
| `fs.read_*` · `fs.write_*` | path, length, content **hash** |
| `shell.exec` | argv, exit code, stdout / stderr **hash** |

- **Always on** — no opt-out
  - bodies as **hashes** by default
  - `--full-record` keeps the raw bytes
- **`aeris trace diff a b`**
  - aligns events by scope
  - → fast **regression bisect**

---

<!-- _class: divider -->

<p class="eyebrow">Section 6 · L4</p>

# AI & agents

> From a single model call to a typed multi-agent graph — the LLM is a **first-class** part of the language.

---

<!-- _class: tight -->

# Talking to a model — the `ai` module

> The building blocks beneath agents: plain calls into the `ai` native module. Each needs `ai.*` in `cap` and is **tape-recorded**.

<div class="columns">
<div class="column">

```rust
// one-shot completion
let summary = ai.complete("Summarise:\n{doc}")?

// constrained choice — retried until valid
let kind = ai.decide(
  prompt:  "Classify: utilities, software, travel, other.",
  choices: ["utilities", "software", "travel", "other"],
)?

// multi-turn session, auto-compacting history
let s = ai.session(system: "Be concise.", model: "claude-haiku-4-5")
let (s2, reply) = ai.session_ask(s, "explain capabilities")
```

</div>
<div class="column compact">

- **`ai.complete`** — prompt → text
- **`ai.decide`** — pick from a fixed list
  - **retried** until valid
- **`ai.session` / `ai.session_ask`**
  - multi-turn, **auto-compacts** ~40 turns
- **`ai.embed`** — text → vector (RAG)
- **`ai.tools`** — model **calls your functions**
- **`ai.usage()`** — tokens · cost · calls

</div>
</div>

---

<!-- _class: tight -->

# `ai.chat` — grounded chat, and a server in one call

> A higher-level helper: load a folder of docs as a **knowledge base**, then ask — or expose the same KB as an HTTP endpoint.

<div class="columns">
<div class="column">

```rust
// load ./docs into the system prompt, then ask
let chat = ai.chat("You are concise.", "./docs")
io.println("loaded {chat.kb_size()} files")
let answer = chat.ask("how do capabilities work?")?

// the same KB, exposed as a blocking HTTP chat server
ai.chat("You are concise.", "./docs", port: 8080)
```

</div>
<div class="column compact">

- **`ai.chat(system, dir)`**
  - loads `*.md` / `*.txt` / `*.yaml` as a **KB**
- **`chat.ask(p)?`** — query the KB
- **`chat.kb_size()`** — file count
- **`ai.chat(system, dir, port)`**
  - same KB → **HTTP chat server**
  - a bot in **one line**
- Backend in `[ai.backend]`
  - every turn **traced**

</div>
</div>

---

<!-- _class: tight -->

# `agent` — a single LLM unit

> An `agent` wraps one LLM call in a typed declaration: a fixed model, a prompt, a **schema in and a schema out**.

<div class="columns">
<div class="column">

```rust
model Ticket@v1  { subject: string, body: string }
model Triage@v1  { team: string, urgent: bool }

agent triage {
  llm:     "claude-opus-4-7"   // pinned → replayable
  intent:  "route an incoming support ticket"
  accept:  Ticket@v1           // schema IN
  produce: Triage@v1           // schema OUT
  prompt:  "Pick the owning team and whether it's urgent."
  policy:  pii_redact          // runs on every call
  retries: 2
}
```

</div>
<div class="column compact">

- **`llm:`** — pinned model
  - → run is **replayable**
- **`accept:` / `produce:`** — input/output `model@vN`
  - validated **at the boundary**
  - malformed reply → **never reaches your code**
- **`prompt:`** — your instruction
  - JSON contract **auto-appended**
- **`policy:` / `retries:`**
  - guardrails + bounded failure
- **Call as a function**
  - `triage(ticket, cap)` with `cap` carrying `ai.*`
  - every call **tape-recorded**

</div>
</div>

---

<!-- _class: tight -->

# `agent_net` — a typed dataflow of agents

> When several agents coordinate, **the routing between them *is* the program**. Aeris makes it a typed graph instead of a tangle of prompt strings.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 260" role="img" aria-label="agent_net support_desk: a DAG where triage fans out to draft_reply and escalate; draft_reply feeds review. The whole DAG is re-run until review approves the draft.">
<defs>
<marker id="an" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
<marker id="anf" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#FF7E51"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="14" y="118" width="104" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="66" y="145" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">triage</text>
<rect x="180" y="60" width="128" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="244" y="87" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">draft_reply</text>
<rect x="180" y="176" width="128" height="44" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="244" y="203" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">escalate</text>
<rect x="356" y="60" width="110" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="411" y="87" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">review</text>
<line x1="118" y1="134" x2="180" y2="90" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<line x1="118" y1="146" x2="180" y2="194" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<line x1="308" y1="82" x2="356" y2="82" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<path d="M411,60 C411,10 66,10 66,118" fill="none" stroke="#FF7E51" stroke-width="2" stroke-dasharray="5 5" marker-end="url(#anf)"/>
<text x="240" y="32" text-anchor="middle" font-size="13" font-style="italic" fill="#D14600">re-run DAG until review.approved</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

```rust
// agents declare their schemas (llm, prompt omitted)
agent triage      { accept: Ticket@v1, produce: Triage@v1 }
agent draft_reply { accept: Triage@v1, produce: Draft@v1  }
agent escalate    { accept: Triage@v1, produce: Case@v1   }
agent review      { accept: Draft@v1,  produce: Review@v1 }

agent_net support_desk {
  intent "triage a ticket, then draft a reply or escalate"

  flow triage -> { draft_reply, escalate }
  flow draft_reply -> review

  until: review.approved || iterations >= 3
}
```

- **Typed edges**
  - `triage` produces `Triage@v1`
  - exactly what `draft_reply` & `escalate` accept
  - mismatch → **won't route**
- **Bounded iteration**
  - `until:` predicate or `iterations`
  - **cycles rejected at parse time**

</div>
</div>

---

<!-- _class: tight -->

# `agent_net` — composition & failure

> A net is observable and bounded by construction: every edge is traced, and failure can't run away.

<div class="columns">
<div class="column">

```json
{ "kind": "net_enter", "net": "support_desk", "iter": 0 }
{ "kind": "edge", "from": "triage", "to": "draft_reply",
  "schema": "Triage@v1" }
{ "kind": "agent_call", "agent": "draft_reply",
  "model": "claude-opus-4-7", "tokens": 318 }
{ "kind": "edge", "from": "draft_reply", "to": "review",
  "schema": "Draft@v1" }
{ "kind": "net_exit", "net": "support_desk",
  "outcome": "ok", "iters": 2 }
```

</div>
<div class="column compact">

- **Composition**
  - a net is **itself a node**
  - no recursion
- **Every edge traced**
  - records the `model@vN` schema
- **Bounded failure**
  - retries burned → `err.llm` → propagates
- **Bounded loop**
  - re-run until `until:` or `iterations`
  - else → `Err("agent_net exhausted")`

</div>
</div>

---

<!-- _class: tight -->

# `ai.network` — agents discovered at runtime

> The programmatic sibling of `agent_net`: build the agent set **at run time**, route by plain text.

<div class="columns">
<div class="column">

```rust
var net = ai.network(max_rounds: 10)
net.agent(name: "geologist",     system: geo_prompt)
net.agent(name: "risk_assessor", system: risk_prompt)
net.agent(name: "reporter",      system: rep_prompt)

let r = net.run(
  entry:   "geologist",
  message: "Analyse today's M4.5+ events.",
  until:   "DONE",                 // stop sentinel
)
```

</div>
<div class="column compact">

- **Built at run time**
  - agents into a `var`
  - often loaded from a **folder of prompts**
- **Text-based routing**
  - `>>NAME:` prefix = **hand-off**
  - else round-robin
  - stops at the `until` sentinel
- **`agent_net` vs `ai.network`**
  - typed + schema-checked  vs  free-form text
  - **stable set**  vs  **runtime discovery**

</div>
</div>

---

<!-- _class: divider -->

<p class="eyebrow">Section 7</p>

# The theory behind Aeris

> Now the *why*: the design tension it navigates, the paradigm shift it answers, and what it does — and does not — promise.

---

<!-- _class: tight -->

# The trilemma

> Three goals pull against each other. A language that goes all-in on one **pays for it on the other two**.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 560 420" role="img" aria-label="The design trilemma — verifiability, readability, expressiveness — with Aeris at the centroid">
<g font-family="Inter, system-ui, sans-serif">
<polygon points="280,80 80,360 480,360" fill="#FBEFE6" stroke="#1C2035" stroke-width="2.5"/>
<line x1="280" y1="80" x2="280" y2="267" stroke="#B4B0AC" stroke-width="1" stroke-dasharray="4 5"/>
<line x1="80" y1="360" x2="280" y2="267" stroke="#B4B0AC" stroke-width="1" stroke-dasharray="4 5"/>
<line x1="480" y1="360" x2="280" y2="267" stroke="#B4B0AC" stroke-width="1" stroke-dasharray="4 5"/>
<text x="280" y="44" text-anchor="middle" font-size="23" font-weight="700" fill="#0E1020">verifiability</text>
<text x="280" y="68" text-anchor="middle" font-size="15" fill="#5F6470">static</text>
<text x="84" y="392" text-anchor="middle" font-size="20" font-weight="700" fill="#0E1020">readability</text>
<text x="470" y="392" text-anchor="middle" font-size="20" font-weight="700" fill="#0E1020">expressiveness</text>
<circle cx="280" cy="267" r="13" fill="#FF7E51" stroke="#1C2035" stroke-width="2.5"/>
<text x="280" y="305" text-anchor="middle" font-size="22" font-weight="800" fill="#0E1020">AERIS</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

**Current status:**

- **Verifiable** — *by construction*
  - `cap` · `intent` · contracts · `policy`
  - **compiler** catches stray effects

- **Readable** — *one way to say each thing*
  - reserved keywords + `aeris fmt`
  - signature **tells the truth**

- **Expressive** — *at the level of intentions*
  - `saga` · `agent` · `model` · `policy`
  - each construct = **whole intention**

> The trick: **stay small**. One construct per concept.

</div>
</div>

---

# Designed for LLMs — *what*, not *how*

> An LLM has **no mental model** — just the next token.

**The design question shifts**

- From *"how do I build this?"*
- To *"**what** do I want built?"*
  - `saga` · `agent` · `intent` · `policy`
  - **whole intentions**, not mechanisms

**Why high abstraction (not low)**

- LLMs get code right in proportion to
  - **resemblance to training data**
  - **fewness of valid completions**
- High abstraction wins on both
  - **fewer decisions → fewer failures**
  - **more signal per token**

---

# Why-as-grammar

> Traditional split: **what** in code, **why** in commits/tickets/PRs.
> The model **never sees the second half**.

**The cost in the agentic era**

- Agent must **reverse-engineer purpose**
  - every guess = fresh **non-determinism**
- The *why* is **re-derived every run**

***why* in the grammar**

- `intent` · `requires:` / `ensures:` · `policy`
  - **not comments**
  - **enforced** · **traced** · **machine-readable**
- Runtime **rejects the omission**

> Not a language humans write better — one **agents run with more certainty**.

---

# Three sources of non-determinism

> Aeris tries to make each source **explicit, contained, reproducible**.

- **The model** — *same prompt → different output*
  - **Capture, don't control**
  - every `ai.*` taped → `aeris replay` is **bit-identical**

- **The grammar** — *ambiguity makes the model guess*
  - **Reduce the choices**
  - reserved keywords · one form · `cap` as a value

- **The world** — *networks drop, DBs mutate*
  - **Isolate & declare**
  - `cap` bounds reach · `model@vN` validates · `policy` halts

> **Honest limits:** first LLM call unpredictable · logic *inside* `cap` not verified · `undo` cascade **best-effort**.

---

<!-- _class: tight -->

<style scoped>
  section > ul > li { font-size: 34px; line-height: 1.32; margin: 10px 0; }
  section > ul > li > ul { padding-left: 24px; margin-top: 4px; }
  section > ul > li > ul > li { font-size: 26px; margin: 4px 0; color: #1C2035; }
</style>

# Why ?

- **vs Python**
  - skip the `langchain` + `pydantic` + `openai` + `structlog` stack — these are **language built-ins**
  - script feel under `enforce = "off"`, ramps to **static effect + schema checks** under `"strict"`
  - native **JSONL trace + `aeris replay`** — no `logging` + OpenTelemetry plumbing to wire
  - `model@vN` + `where` at every boundary — no Pydantic decorators sprinkled across the codebase

- **vs Java**
  - **no JVM**, no Spring, no app server — single **8 MB binary** deployed with `scp`
  - `agent` · `saga` · `policy` · `intent` are **language keywords**, not annotations or Spring beans
  - `result<T>` + `?` + `catch` instead of **checked-exception ceremony** and `try/catch/throws`
  - cold start **&lt; 50 ms** — no JIT warmup, no fat WAR / uberjar

- **vs Rust / Go**
  - **interpreted, fast iteration** — no borrow checker, no `go build` / `cargo build` cycle
  - `saga` · `agent_net` · `model@vN` are **first-class** — not patterns to reimplement per project
  - `cap` tracks **external effects** (HTTP, FS, K8s, LLM) — Rust's idea applied to *side effects*, not memory aliasing
  - idempotency keys + reverse rollback come **with the saga keyword**, not as a Temporal/Conductor SDK

- **vs YAML pipelines** (Airflow, dbt, Terraform)
  - one `.aer` file replaces **YAML + Python + IaC** — pipeline, agents, governance in **one review surface**
  - **typed values, cap-checked effects, tracing built-in** — not string templating with sigils and macros
  - same source goes through `aeris check`, `aeris fmt`, `aeris replay` — no Jinja / Go-template indirection

---

# Next steps

> Two directions, both open.

- **Build something *with* it**
  - a deploy, a triage flow, a chatbot grounded on your docs — one `.aer` file each
  - `enforce = "off"` to prototype, ramp to `"strict"` when the shape stabilises

- **Build something *on* it**
  - bug fixing :)
  - new L2 libraries, new *ai* features, improvements
  - the spec lives in `docs/{thesis,language,project,plan}.md`; the runtime is one Rust crate

---

<!-- _class: divider -->

# Thank you

> *Questions, feedback and contributions are welcome.*
