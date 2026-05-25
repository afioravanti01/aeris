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
  /* Theme default is 36px; bumped a little so the slimmer slides
     fill the canvas. */
  section { font-size: 42px; }
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
| **3** | **L1 · The core language** | values, functions, pattern matching, models, libraries |
| **4** | **L2 · Verifiability & governance** | `cap`, `intent`, contracts, `policy`, supply chain |
| **5** | **L3 · The agentic loop** | `saga`, trace & replay |
| **6** | **L4 · AI & agents** | `ai.*`, `agent`, `agent_net` |
| **7** | **The theory behind Aeris** | trilemma, *what*-not-*how*, why-as-grammar |

---

<!-- _class: divider -->

<p class="eyebrow">Section 1</p>

# Aeris at a glance

> Why it exists, what it looks like, the four layers it stacks, and how it differs from tools you already know.

---

# Aeris

> A personal experiment to build a language using an **LLM**.

<div class="columns">
<div class="column">

**What it is**

- A small interpreted language for **AI, automation and operations**
  - one `.aer` file, one static binary under **8 MB** — no JVM, no Python, no container
  - reads like Rust / Go / Swift / TS — *plus* `saga`, `agent`, `policy`, `intent`, `model`, `cap`

**Where it came from**

- Started just as an experiment 
- Usage of **Agentic Coding** 
- Long discussion before generating the project

</div>
<div class="column">

**How it was built — docs before code**

- Key files definition:
  - **`thesis.md`** — the rationale, written before any code; non-negotiable
  - **`language.md`** — the language spec, **based on** the thesis
  - **`project.md`** — the practical constraints to respect
  - **`plan.md`** — **~50 milestones**, each with a pass/fail acceptance check
- Iterative approach
  - the LLM writes → the docs say what's allowed → the check confirms → mark done
</div>
</div>

---

# Change of perspective

> The author of code is now an **LLM** — no mental model, just the next token, reading only the source. That leaves **two problems** a language has to answer.

<div class="columns">
<div class="column">

**1 · Opacity** — *what does this code touch?*

- Effects aren't on the surface — a helper deep in the call tree can quietly hit the network, the disk, an API
- You'd have to **read every line** to be sure — which doesn't scale when a model writes the code

</div>
<div class="column">

**2 · Non-determinism** — *does it do the same thing twice?*

- **Generation** varies — the same prompt yields different code
- The **runtime** drifts — networks, clocks, files and LLM replies all move under the program

</div>
</div>

> Aeris's goal: make that code **legible** — you can see what it touches without running it — and **reproducible** — you can replay exactly what it did.

---

# What today's defences miss

> Back to the first problem — **opacity**. The tools you'd reach for each check something *else*, never *which function can touch which resource*.

| Approach | What it checks | What it misses |
|---|---|---|
| **Sandboxing** — Docker, gVisor | what the whole *process* may do | *which function* does it — a deep helper can still call `http.get` |
| **Static types** — TS, Java, Rust | the *shape* of data | what the code *does* with it — a `String` can be produced by anything |
| **Effect systems** — F\*, Dafny, Koka | effects, *with proofs* | easy everyday use — cryptic solver errors, hard to learn |
| **Frameworks** — Airflow, Temporal | the *structure* and order of steps | real *enforcement* — a "step" is still arbitrary code |

> Aeris answers it in the **signature**: a function lists every resource it can reach — no need to read the body.

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
<text x="40" y="215" font-size="22" font-weight="700" fill="#0E1020">L3 — Agentic loop</text>
<text x="40" y="242" font-size="16" fill="#5F6470">saga with do/undo, derived idempotency</text>
<line x1="300" y1="255" x2="300" y2="270" stroke="#1C2035" stroke-width="2.5" marker-end="url(#L4)"/>
<rect x="20" y="270" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="300" font-size="22" font-weight="700" fill="#0E1020">L4 — Multi-agent orchestration</text>
<text x="40" y="327" font-size="16" fill="#5F6470">typed agent_net, schema at every edge</text>
</g>
</svg>
</figure>

</div>
<div class="column compact">

- **L1 · syntax** (§3) — values, functions, `model`, `use`
  - dense and unambiguous → **fewer wrong guesses**
- **L2 · semantics** (§4) — `cap`, `intent`, contracts, `policy`
  - the **compiler** catches a stray effect, not the reviewer
- **L3 · agentic loop** (§5) — `saga`, trace & replay
  - **clean recovery** from a failed run
- **L4 · multi-agent** (§6) — `ai.*`, `agent`, `agent_net`
  - routing as a **typed graph**, not prompt strings

> Each layer builds on the one below; **opt in by depth** — a script lives in L1, a production pipeline uses all four.

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

**The stages**

- **Lexer** → tokens
  - every keyword is reserved, so meaning never depends on position
- **Parser** → AST
  - the program's nested structure
  - Aeris's **static checks** run here
- **Evaluator** → effects
  - one recursive function over the tree
  - *what the source says is what it does, in order*

> Verification lives **at parse time**; the trace is produced **at walk time**.

</div>
</div>

---

<!-- _class: tight -->

# Tree-walking, concretely

> Each node is either a **statement** (changes the environment) or an **expression** (returns a value). Evaluation is a depth-first walk: children first, then the parent.

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

- A **function call** = a sub-walk over the callee's tree
  - push a scope, bind params, recurse
- `return` / `break` / `continue`
  - unwind the walk back to the right frame

**Why this matters for Aeris**

- The walk is where effects are **recorded**
  - every `cap.*`, `clock.now`, `random.next`, `ai.*`
- `aeris replay` re-walks the *same tree*
  - against the recorded tape → the same run, offline

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

> Expressions everywhere, immutable by default, **one construct per concept**. Every keyword is reserved, so `grep saga` finds every saga.

<div class="columns">
<div class="column">

```rust
use io, json, fs            // stdlib
use kube, docker            // native modules

let  x = 1            // immutable (default)
var  y = 1            // mutable, function-scope only
const PI = 3.14159    // module-level, folded

record User {
  id:   uuid
  name: string
  age:  int  where age >= 0   // checked at construction
}

enum Status {
  Pending
  Active(since: timestamp)
  Banned { reason: string }
}

let n = if x > 0 { 1 } else { -1 }   // blocks are values
```

</div>
<div class="column compact">

- **Primitives**
  - `decimal`, `uuid`, `date`, `timestamp`, `duration` are first-class literals (`2026-05-07`, `500ms`)
- **Records** — immutable values
  - copied **by value** on every rebinding; no shared mutable references
- **Enums** — sum types
  - `match` **must cover every case**, checked at parse time
- **Bindings** — `let` / `var` / `const`
  - `var` is **function-scope only**; module level allows only `const`
  - so "no `cap` means pure" is **guaranteed**, not just convention

</div>
</div>

---

<!-- _class: tight -->

# Functions, control flow, errors

> The signature tells the whole truth about a function. Errors are **values** — passed up with `?`, never hidden.

<div class="columns">
<div class="column">

```rust
use kube, docker            // native modules

fn discount(amount: decimal, pct: decimal) -> decimal
  requires: amount >= 0
  requires: pct >= 0 and pct <= 1
  ensures:  result >= 0 and result <= amount
{
  amount * (1 - pct)
}

fn restart(svc: string, cap: cap[kube.delete @ ["prod-eu-1"]])
  -> result<unit>
{
  intent "restart {svc}: delete its pods" {
    kube.delete("pods", selector: "app={svc}")?   // ? bubbles Err
  }
}
```

</div>
<div class="column compact">

- **Purity is structural**
  - `fn add(a, b)` with no `cap` ⇒ it can't perform any side effect
- **Errors are values**
  - `result<T>` = `Ok(T) | Err(err)`; **`?`** returns early on `Err`
  - one error type, structured for the trace
- **Control flow**
  - `if`, `match`, `for`, `while`, `loop`
  - `every`, `retry`, `timeout` cover the common timing patterns
- **Contracts** — `requires:` / `ensures:`
  - checked at runtime on the boundary, not proved by a solver (more in §4)

</div>
</div>

---

<!-- _class: tight -->

# Pattern matching & collections

> `match` is exhaustive and destructures; standard containers cover the rest. A missing case is a **parse error**, not a runtime surprise.

<div class="columns">
<div class="column">

```rust
let xs: list<int> = [1, 2, 3]
let m    = { "a": 1, "b": 2 }     // map
let pair = ("ok", 200)            // tuple

let label = match status {
  Pending           -> "waiting",
  Active(since)      -> "up since {since}",
  Banned { reason }  -> "blocked: {reason}",
}

let first = xs.first() ?? 0       // option, with a default
```

</div>
<div class="column compact">

- **`match`** — must be **exhaustive**; arms **destructure** and may carry an `if` guard
- **Collections** — `list`, `set`, `map`, `tuple`, plus `option<T>` and `result<T>`
- **`??`** — supplies a fallback for `None`, `Err`, or `()`
- **Errors** — `?` propagates, `catch` recovers inline, `raise` aborts with a typed error

</div>
</div>

---

<!-- _class: tight -->

# Models — versioned trust-boundary schemas

> A `model` is the **only** type the runtime checks where untrusted data enters: an LLM response, incoming network data, a queue message.

<div class="columns">
<div class="column">

```rust
model Invoice@v1 {
  id:       uuid
  amount:   decimal  where amount > 0 and amount < 1_000_000
  customer: string   where len(customer) <= 64
  issued:   date     where issued <= today()
  lines:    list<Line@v1> where len(lines) >= 1
}
```

</div>
<div class="column compact">

- **Version tag `@vN` is mandatory**
  - bare `Invoice` is a parse error
  - `Invoice@v1` and `@v2` are *distinct types*; migration is an explicit function
- **`where` clauses validated at every boundary**
  - construction, JSON decode, agent boundary, HTTP / queue ingress
- **A bad shape raises `SchemaViolation`**
  - rejected **before** it reaches your logic

> This is how an unpredictable LLM output is forced into a **known shape** at the door.

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
  - `io`, `fs`, `http`, `shell`, `env`, `clock`, `random`, `json`
  - compiled into the binary; a small, frozen registry
- **native modules** — signed binaries
  - `ai`, `kube`, `docker`, `mongodb`, `minio`, `rabbitmq`, `audit`
  - `.so` / `.dylib`, **pinned by blake3 and signed by the Aeris registry**
  - each declares its `cap` surface in a manifest the checker reads
- **external libraries** — third-party
  - `.aer` source, pinned by blake3 hash in `aeris.toml`
  - no `latest`, no `*`, no movable git tags

</div>
</div>

---

<!-- _class: divider -->

<p class="eyebrow">Section 4 · L2</p>

# Verifiability & governance

> The structural core: `cap` · `intent` · contracts · `policy` · supply chain. **Verified at parse time, enforced at run time.**

---

# Structural, not semantic

> Aeris does not try to prove your code is *correct*. It makes the **suspicious code unable to hide** — and the rules it must obey **explicit in the source**.

**Visible in the source, checked at parse time**

- **what** a function can touch → the `cap` in its signature
- **why** it touches it → an enclosing `intent` (mandatory on writes)

**Declared in the source, enforced at run time**

- **invariants** on inputs and outputs → `requires` / `ensures`
- **guardrails** the model can't forget → `policy` (`deny` / `limit` / `require`)

**What it deliberately does *not* do**

- it does not check the *logic* inside a legitimate `cap`
  - a function that holds `audit.write` can still log the wrong thing
  - that stays the job of tests, review, and backend RBAC

---

<!-- _class: tight -->

# Capabilities are values

> **Authority is a value you pass as a parameter, not something the whole process holds.** Hold the value and you can make the call; without it you can't — *the code won't even parse*.

<div class="columns">
<div class="column">

```rust
// pure: no cap ⇒ cannot do IO at all
fn total(items: list<Invoice@v1>) -> decimal {
  items.fold(0, fn(a, it) { a + it.amount })
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

- **The signature is the contract**
  - answer *"what does this touch?"* without reading the body
- **`cap` cannot escape**
  - not stored in a field, returned, or put on a channel
  - `cap[*]` is forbidden in user code
- **Enforcement is a project decision**
  - `off | loose | strict` in `aeris.toml`

**Lineage — object-capability security**

- **Dennis & Van Horn, 1966** (MIT) — coined it
- **Mark Miller's E**, ~2003 — first practical language to use it
- Capsicum, Genode, Pony follow — *applied engineering, not new research*

</div>
</div>

---

# `intent` — the *why* in the grammar

> Languages keep *what the code does* in the source and *why* in commits and tickets — places **the model never sees**. Aeris pulls the *why* into the grammar.

```rust
intent "rotate the leaked TLS cert" {
  fs.write_file(cert_path(), new_cert)?
  audit.event("cert.rotated", { path: cert_path() })
}
```

- **Required on every write call**
  - code that calls `fs.write_*`, `http.post`, `kube.apply`, `audit.event` or `ai.*` without an enclosing `intent` **won't parse**
- **It flows into the trace**
  - emits `intent_enter` / `intent_exit`
  - every event inside **carries that intent** → the purpose sits above every side effect in the code
- **What it does *not* do**
  - it does not check that the body matches the string
  - it makes **leaving it out impossible** — an LLM-written PR can't hide a write in silence

---

# Contracts — `requires` · `ensures` · `where`

> Runtime checks on inputs, outputs and world state. **Deliberately not** solver proofs.

- **Where they live**
  - **`requires:`** — checked at entry
  - **`ensures:`** — checked on every return path (`result` names the value)
  - **`where`** — on record/model fields and `match` arms
- **On violation**
  - raises `ContractViolation` — **never silenced, not catchable by `?`**
  - logged to the trace, exit code `64`

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
  - **`match:`** — the cap path(s) it applies to
  - **`deny:` / `require:`** — raise `PolicyViolation`
  - **`limit:`** — a quota over a window
  - **`audit:`** — extra fields in the trace
- **Activation**
  - on import, scoped to a `fn`, or project-wide in `aeris.toml`
- **Recorded** — a live-vs-replay divergence is a `policy_drift` event

> Extra protection against a malicious LLM injection — the egress allow-list is enforced at the call site.

</div>
</div>

---

# Content-addressed supply chain

> Every external dependency is identified by **the hash of its bytes**. Change the bytes and **none** of its code runs.

```toml
[deps]
deploy = { source = "github.com/acmecorp/aeris-devops",
           version = "1.2.0", hash = "blake3:7e2c...c1a4" }
```

- **Readable aliases bound to blake3 hashes**
  - pinned in a committed manifest
  - a hash mismatch **fails before any of the dependency's code runs**
- **No `latest`, no `*`, no movable git tags**
  - *"what version is in this build?"* is answerable from `aeris.toml` without running anything
- **Native modules** (`ai`, `kube`, `docker`, …)
  - also carry an **ed25519 registry signature**, checked before they are loaded

> **Lineage:** Nix store paths (Dolstra, PhD 2006), Cargo's `Cargo.lock`, Go's `GOSUMDB`. **Known-good, applied consistently** across the whole supply chain.

---

<!-- _class: divider -->

<p class="eyebrow">Section 5 · L3</p>

# The agentic loop

> The loop **one agent** runs while acting on the world — *do the work, record it, undo on failure*. Every step is **reversible**, every run **replayable**. *(Coordinating **many** agents is L4, next.)*

---

<!-- _class: tight -->

# What is the agentic loop?

> A single agent doing real work runs the same cycle on **every step**: do it, record it, and undo it if a later step fails.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 300" role="img" aria-label="The agentic loop: do a step, record it; on success move to the next step, on failure undo the completed steps">
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

**Why it needs the language**

- A long-running job on the outside world **will** fail somewhere — a dropped network, a bad reply, a half-applied change
- By hand, that leaves a **half-done mess**

**What the loop gives you**

- Every run ends **defined** — fully done, or fully rolled back
- The recording lets you **replay** it offline, exactly

> Two constructs make it real: **`saga`** (do/undo + idempotency) and **trace & replay** (record + reproduce).

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
  - a failed step **undoes the completed steps in reverse order**
  - if rollback can't finish, it reports an honest **`PartialFailure`** — never a silent half-state
- **Idempotency keys**
  - auto-derived `blake3(trace_id, step, index)`, injected into writes
  - replaying a half-done saga won't double-charge

**Lineage — the SAGA pattern**

- **Garcia-Molina & Salem, 1987** (SIGMOD)
  - for DBs that couldn't hold a long transaction; today **Temporal**, Step Functions
- Aeris's change: **compensation is required by the syntax**, not an optional add-on

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

- **Every run emits a JSONL trace**
  - one self-contained event per line, each carrying the **active `intent`**
- **Always-on recording** (N3 / N2)
  - `ai.*`, `clock.now`, `random.next` are always captured — not opt-in
- **`aeris replay` re-walks the tree against the tape**
  - **bit-for-bit identical** for the deterministic parts, fixed for the rest
- **All of it runs offline**
  - audit, debugging, regression, post-mortem

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
| `http.*` | url, method, status, request & response **hash** |
| `fs.read_*` · `fs.write_*` | path, length, content **hash** |
| `shell.exec` | argv, exit code, stdout / stderr **hash** |

- **Always on** — no opt-out in production; bodies are stored as **hashes** by default (`--full-record` keeps the bytes)
- **`aeris trace diff a b`** — aligns events by scope and reports what diverged → fast regression bisects

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

- **`ai.complete`** — one prompt → text; the base call
- **`ai.decide`** — the model picks from a fixed `choices` list, **retried until the reply is valid**
- **`ai.session` / `ai.session_ask`** — multi-turn history that **auto-compacts** past ~40 turns
- **`ai.embed`** — text → vector, for semantic search / RAG
- **`ai.tools`** — let the model **call your functions**, not just return text
- **`ai.usage()`** — running tokens / cost / calls, any time

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

- **`ai.chat(system, dir)`** — loads `*.md` / `*.txt` / `*.yaml` … as a labelled **knowledge base**
- **`chat.ask(p)?`** / **`chat.kb_size()`** — query it; count the files loaded
- **`ai.chat(system, dir, port)`** — the same KB as a **chat server** — a bot in one line
- Everything goes through the configured `[ai.backend]`; every turn is traced

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

- **`llm:`** — the exact model, pinned so the run can replay
- **`accept:` / `produce:`** — input and output **models**; the runtime validates both at the boundary, so a malformed reply never reaches your code
- **`prompt:`** — your instruction; the JSON output contract is appended automatically
- **`policy:` / `retries:`** — guardrails and bounded failure, as declared fields
- **Call it like a function** — `triage(ticket, cap)`, with a `cap` carrying `ai.*`; every call is **tape-recorded**

</div>
</div>

---

<!-- _class: tight -->

# `agent_net` — a typed dataflow of agents

> When several agents coordinate, **the routing between them *is* the program**. Aeris makes it a typed graph instead of a tangle of prompt strings.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 250" role="img" aria-label="agent_net support_desk: triage fans out to draft_reply and escalate; draft_reply feeds review; review loops back to draft_reply until approved">
<defs>
<marker id="an" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
<marker id="anf" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#FF7E51"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="14" y="98" width="104" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="66" y="125" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">triage</text>
<rect x="180" y="40" width="128" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="244" y="67" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">draft_reply</text>
<rect x="180" y="156" width="128" height="44" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="244" y="183" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">escalate</text>
<rect x="356" y="40" width="110" height="44" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="411" y="67" text-anchor="middle" font-size="18" font-weight="700" fill="#0E1020">review</text>
<line x1="118" y1="114" x2="180" y2="70" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<line x1="118" y1="126" x2="180" y2="174" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<line x1="308" y1="62" x2="356" y2="62" stroke="#1C2035" stroke-width="2" marker-end="url(#an)"/>
<path d="M411,84 C411,138 244,140 244,86" fill="none" stroke="#FF7E51" stroke-width="2" stroke-dasharray="5 5" marker-end="url(#anf)"/>
<text x="328" y="132" text-anchor="middle" font-size="13" font-style="italic" fill="#D14600">until approved</text>
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

- **Typed edges** — `triage` produces `Triage@v1`, exactly what `draft_reply` & `escalate` accept; a mismatch won't route
- **`until:`** bounds the loop; **cycles are rejected** at parse time — no runaway agents

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

- **Composition** — a whole `agent_net` is itself a node; reuse it inside another net (no recursion)
- **Every edge is traced** — each crossing records the `model@vN` schema that passed
- **Failure is bounded** — an agent burns its `retries:`, then emits `err.llm`, which **propagates to its consumers**
- **The loop is bounded** — re-runs until `until:` holds or `iterations` is hit; otherwise `Err("agent_net exhausted")`

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

- **Built at run time** — agents registered into a `var`, often loaded from a **folder** of prompt files
- **Text-based routing** — a reply prefixed `>>NAME:` hands off; otherwise round-robin; stops at the `until` sentinel
- **Which to use?**
  - **`agent_net`** — typed, schema-checked edges; the agent set is **stable**
  - **`ai.network`** — free-form text, **discovered at runtime**; lower overhead

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

**Why Aeris reaches all three at once**

- **Verifiable** — *by construction, not by proof*
  - the signature names every effect (`cap`); `intent`, contracts and `policy` are declared and checked at parse time → the **compiler** catches a stray effect, not a reviewer
- **Readable** — *one way to say each thing*
  - reserved keywords, one canonical form (`aeris fmt`), a familiar curly-brace surface → a signature tells you what a function does **without reading the body**
- **Expressive** — *at the level of intentions*
  - `saga`, `agent`, `model`, `policy` say in a few tokens what would take pages → each construct is a **whole intention**, not a mechanism

> The trick is staying **small**: one construct per concept is what lets all three hold together.

</div>
</div>

---

# Designed for LLMs — *what*, not *how*

> An LLM has no mental model of your program. It only has a **probability distribution over the next token** — so writing code is, for it, *inherently random*.

**So the design question changes**

- From *"how do I build this?"* to *"**what** do I want built?"*
  - `saga`, `agent`, `intent`, `policy` are not mechanisms
  - they are **whole intentions**, turned into first-class constructs

**Why high abstraction, not low**

- An LLM gets code right in proportion to two things
  - how much the code **resembles its training data**
  - how **few valid completions** the language allows
- High abstraction helps with both
  - **fewer decisions → fewer failure points**
  - **more signal per token** — `agent_net { flow a -> b }` > 50 lines of Python

---

# Why-as-grammar

> Programs traditionally split **what the code does** (in the source) from **why it does it** (in commits, tickets, PRs). The model never sees the second half.

**The cost in the agentic era**

- An agent reading code without the *why* has to **reverse-engineer the purpose**
  - every inference it makes is a fresh point of **non-determinism**
- The *why* lives out-of-band, so the agent **re-derives it on every run** — wrong as often as right

**Aeris's move — put the *why* in the grammar**

- `intent`, `requires:` / `ensures:`, `policy` are **not comments**: they are enforced, traced, and machine-readable
- the agent stops **guessing** what "right" means — the grammar states it, and the runtime rejects the omission

> The goal is not a language humans write better — it is one **agents run with more certainty**.

---

# Three sources of non-determinism

> Aeris does not *remove* non-determinism — it meets each source at a different level to make it **explicit, contained and reproducible**.

- **The model** — *same prompt, different output*
  - **Capture, don't control** — every `ai.*` call is taped (prompt, model, response, tokens); `aeris replay` re-runs from the tape, no network, **bit-identical**. Reproducible after the first run.
- **The grammar** — *ambiguity makes the model guess*
  - **Reduce the choices** — reserved keywords, one construct per concept, one canonical form (`aeris fmt`), `cap` as a value → fewer valid completions, so fewer wrong guesses.
- **The world** — *networks drop, DBs mutate, files change*
  - **Isolate & declare** — `cap` bounds what a function can touch, `model@vN` validates data at the boundary, `requires`/`ensures` and `policy` halt on a bad state → the blast radius is small and visible.

> Honest limits: the **first** LLM call is still unpredictable, logic inside a legitimate `cap` isn't verified, and cascading `undo` is **best-effort**.

---

<!-- _class: divider -->

# Thank you

> **Aeris is open source** — questions, feedback and contributions welcome.
