---
marp: true
theme: aeris
paginate: true
html: true
size: 16:10
title: "Aeris v0.3"
header: 'Technical talk · v0.3'
footer: 'Aeris v0.3 · an experiment in designing for the era when code is written by models'
---


<style>
  /* Bumped from theme default (36px) so the slimmer slides fill the canvas. */
  section { font-size: 40px; }
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

<p class="eyebrow">Technical talk · v0.3</p>

## AERIS v0.3

A small interpreted language. **An experiment in designing for the era when code is written by models.**

---

# Agenda

| # | What we'll cover |
|---|---|
| **1** | Motivation — why a toy language, what "agentic coding" means here |
| **2** | Methodology — thesis · spec · plan · iterations |
| **3** | Design — the trilemma, three sources of non-determinism |
| **4** | What we drew on — capabilities, sagas, content addressing |
| **5** | The language — four layers, AST walk, one concrete example |
| **6** | What we observed, what we refused, open questions |

---

# Why a toy language

> Code is now written and read by **LLMs**. We wanted to see what a language *designed for that audience* looks like — so we built one.

**Be clear about what this is**

- Aeris is a **toy**. Not a product, not an enterprise pitch.
- A small interpreted language used as a **vehicle for an experiment**.
- ~6 KLOC of Rust core, tree-walk interpreter, single binary.

**The question being asked**

- What changes in a language if its **principal author is a model**, not a human?
- And: what does it take to build such a language **with an LLM doing the drafting**?

> The point is the **process and the design choices**, not the deliverable.

---

# What "agentic coding" means here

> An LLM has no mental model of a program. It has a **probability distribution over the next token**. Code generation is, for it, **intrinsically stochastic**.

**Two consequences**

- **Generation is stochastic.** Same prompt → different output. `temperature = 0` attenuates the variance, it does not remove it.
- **Reading is shallow.** The model only uses what is **in the source**. Anything in commits, tickets, PR descriptions is invisible to it on the next run.

**The two design pressures that follow**

- **Less ambiguity in the syntax** — every choice the grammar forces on the model is a roll of the dice.
- **More of the *why* inside the source** — anything the language doesn't encode, the model has to **re-infer each run**.

---

# How we worked — thesis → spec → plan → iterations

> A language with structural guarantees can be built only when the **rationale is committed before the code**.

- **`docs/thesis.md`** — written *before any code*. The design commitments. Non-negotiable.
- **`docs/language.md`** — the language surface, **derived from** the thesis. Mechanically constrained by it.
- **`docs/plan.md`** — the implementation, ordered into **~50 milestones**, each with an explicit *acceptance check* (a script that runs to green or red).
- **The inner loop** — model proposes a milestone implementation → spec rules what's allowed → acceptance check verifies → mark done.
- **No `// TODO`, no "we'll come back to it".** Incomplete is incomplete; the milestone stays open.

> **The model proposed, the docs ruled, the checks verified.** Every change traces back to a stated commitment in `thesis.md`.

---

<!-- _class: tight -->

# The design trilemma

<div class="columns">
<div class="column">

```text
        verifiability (static)
              /\
             /  \
            /    \
           /      \
   readability ── expressiveness
```

> *Three desiderata pull against each other. A language at any vertex pays on the other two.*

</div>
<div class="column compact">

**Aeris sits at the centroid**

- **Verifiability is structural, not semantic.** We don't prove a function computes the right answer. We make it **impossible to hide** which resources it touches.
- **Readability is the constraint, not the goal.** The goal is *reviewability* — by a human or by another agent.
- **Expressiveness is deliberately limited.** One construct per concept. No soft keywords. **A grep-able language.**

> Smaller is the feature.

</div>
</div>

---

# Three sources of non-determinism

> Aeris does not try to *eliminate* non-determinism. It makes it **explicit, isolated, governable** — each source addressed at a different level of the design.

| Source | Where it lives | How Aeris responds |
|---|---|---|
| **The model** | Same prompt, different output | **Capture every call** in the JSONL trace; `aeris replay` reproduces it bit-identical |
| **The grammar** | Ambiguity forces the model to infer | Reserved keywords, **one canonical form**, `cap` as a value |
| **The world** | Networks drop, DBs mutate, files change | `cap`, `intent`, `requires:` / `ensures:`, `policy`, `model@vN` |

> The honest promise: **reproducibility after the first run** — *not* deterministic LLM code, which is physically false.

---

<!-- _class: divider -->

# What we drew on

> Three pieces from the literature. **None of them is new.** What is new is putting them together inside a small language with an LLM as the principal author.

---

# Capabilities as values

> Authority is a **value passed by parameter**, not an ambient property of the process. Who holds the value can call; who doesn't, can't.

**Where the idea comes from**

- **Dennis & Van Horn, 1966** — *"Programming Semantics for Multiprogrammed Computations"* (CACM). The original capability paper at MIT Project MAC.
- **The E language** (Mark Miller, ~2003) — brought object-capability into a practical, statically-typed PL.
- **Modern descendants:** Capsicum (FreeBSD), Genode OS, Pony's reference capabilities.

**How Aeris uses it**

- A function declares `cap: cap[http.get @ ["api.acme.com"]]` in its signature. **Without `cap`, it cannot perform any external effect.**
- The signature **is the authority graph**. A reviewer (human or agent) reads it and knows what the function can touch, *without entering the body*.

> Not the first language to do this. The first one we know of doing it **for code written by an LLM**.

---

# The SAGA pattern

> A long-running operation is **a sequence of short operations**, each with a **compensating action** that rolls it back.

**Where the idea comes from**

- **Garcia-Molina & Salem, 1987** — *"Sagas"*. SIGMOD '87 paper. Originally for databases that could not hold a long transaction.
- **Microservices adoption:** Netflix Conductor, Uber Cadence → **Temporal**, AWS Step Functions. All are sagas with different surfaces.

**How Aeris uses it**

- Every `step` declares **both** `do` (the action) and `undo` (the compensation). `undo: noop` is allowed only when `do` does not write.
- If a step fails, the runtime runs the `undo`s of completed steps **in reverse order**.
- **Idempotency keys** are auto-derived as `blake3(trace_id, step_name, retry_idx)` and injected into write capabilities — so a replay does not double-charge.

> Aeris's contribution: making compensation **mandatory syntax**, not an optional decorator.

---

# Content-addressed supply chain

> Every external dependency is identified by **the hash of its bytes**. If the bytes change, no code from the dep runs.

**Where the idea comes from**

- **Nix** (Eelco Dolstra, PhD thesis 2006) — purely functional package management; every artifact has a deterministic hash-based store path.
- **Cargo + `Cargo.lock`** (Rust, 2014) and **Go modules + GOSUMDB** (Go, 2018) — same principle, transparency-logged in Go's case.

**How Aeris uses it**

- Every external `.aer` library is pinned by **blake3 hash** in `aeris.toml`.
- L2 native modules add **ed25519 signature** by the Aeris registry key on top of the hash.
- **No `latest`, no `*`, no movable Git tags.** The version answer is one line in the manifest.

> Known-good idea. Aeris just applies it consistently across the language's whole supply chain.

---

# Why-as-grammar — the design move

> Languages have separated *what the code does* (in the source) from *why it does it* (in commits, tickets, PR descriptions). **The agent never sees the second part.**

**The cost of that separation in the agentic era**

- Every read of the code without the *why* makes the agent **reverse-engineer purpose from mechanics**. Each inference is a fresh point of non-determinism.
- An agent executing code without a *why* has **no acceptance criterion** — no way to decide whether to continue, stop, or escalate when state looks off.

**Aeris lifts the *why* into the grammar**

- **`intent "..."`** — purpose attached to every effectful block. **Mandatory at parse**. Propagates into the trace.
- **`requires:` / `ensures:`** — runtime contracts at the boundary, not solver proofs over the body.
- **`policy`** — guardrails the runtime evaluates **on every matching call**. The model cannot forget them.

> This is the **load-bearing claim of the project**. We think the *why* must become machine-readable.

---

<!-- _class: tight -->

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

- **L1 — syntax.** Dense, low-token, no soft keywords. **Fewer choices → fewer hallucinations.**
- **L2 — semantics.** `cap`, `intent`, contracts. The **compiler** catches the rogue effect, not the reviewer.
- **L3 — agentic loop.** `saga` + `do`/`undo` + auto-idempotency keys. **Deterministic recovery over a non-deterministic execution.**
- **L4 — multi-agent.** `agent_net` is a typed dataflow graph; the routing protocol is **owned by the runtime**, not encoded in prompts.

> **Opt-in by depth.** A 30-line script uses L1 only.

</div>
</div>

---

<!-- _class: tight -->

# How the interpreter runs your program

> The AST **is** the program. Aeris walks the tree node by node — no bytecode, no intermediate representation.

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 540 400" role="img" aria-label="Tree-walking interpreter visiting an AST for let x = add(2, 3)">
<defs>
<marker id="ast-arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#5F6470"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">

<rect x="10" y="10" width="520" height="36" rx="4" fill="#F6F3F0" stroke="#1C2035" stroke-width="1"/>
<text x="22" y="34" font-family="Geist Mono, JetBrains Mono, monospace" font-size="18" fill="#0E1020">let x = add(2, 3)</text>

<rect x="180" y="70" width="180" height="46" rx="8" fill="#FFE9C4" stroke="#1C2035" stroke-width="2"/>
<text x="270" y="99" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Let("x", _)</text>
<circle cx="168" cy="93" r="14" fill="#1C2035"/>
<text x="168" y="98" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">1</text>
<text x="372" y="88" font-size="13" font-style="italic" fill="#5F6470">effect:</text>
<text x="372" y="107" font-family="Geist Mono, monospace" font-size="13" font-weight="700" fill="#0E1020">env { x: 5 }</text>

<line x1="270" y1="116" x2="270" y2="160" stroke="#5F6470" stroke-width="2" marker-end="url(#ast-arr)"/>

<rect x="180" y="165" width="180" height="46" rx="8" fill="#D6E5FF" stroke="#1C2035" stroke-width="2"/>
<text x="270" y="194" text-anchor="middle" font-size="17" font-weight="700" fill="#0E1020">Call("add", _)</text>
<circle cx="168" cy="188" r="14" fill="#1C2035"/>
<text x="168" y="193" text-anchor="middle" font-size="13" font-weight="700" fill="#F6F3F0">2</text>
<text x="372" y="183" font-size="13" font-style="italic" fill="#7C3AED">returns:</text>
<text x="372" y="202" font-family="Geist Mono, monospace" font-size="14" font-weight="700" fill="#7C3AED">Value(5)</text>

<line x1="225" y1="211" x2="135" y2="258" stroke="#5F6470" stroke-width="2" marker-end="url(#ast-arr)"/>
<line x1="315" y1="211" x2="405" y2="258" stroke="#5F6470" stroke-width="2" marker-end="url(#ast-arr)"/>

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

<rect x="20" y="362" width="14" height="14" rx="3" fill="#FFE9C4" stroke="#1C2035"/>
<text x="40" y="374" font-size="12" fill="#0E1020">statement · effect on env</text>
<rect x="240" y="362" width="14" height="14" rx="3" fill="#D6E5FF" stroke="#1C2035"/>
<text x="260" y="374" font-size="12" fill="#0E1020">expression · returns Value</text>
<circle cx="430" cy="369" r="9" fill="#1C2035"/>
<text x="430" y="373" text-anchor="middle" font-size="11" font-weight="700" fill="#F6F3F0">n</text>
<text x="445" y="374" font-size="12" fill="#0E1020">visit order</text>

</g>
</svg>
</figure>

</div>
<div class="column compact">

**The whole interpreter, in one sentence**

- One recursive function over typed AST nodes. **What the source says is what it does, in the order it says it.**

**The corners**

- A **function call** is a sub-walk over the callee's sub-tree: push scope, bind params, recurse.
- **Closures** capture their env by reference; `spawn { … }` keeps the scope chain alive.
- **`return` / `break` / `continue`** propagate as a typed unwind to the right frame.

</div>
</div>

---

<!-- _class: tight -->

# A concrete example

```rust
use ai, kube, audit

model Alert@v1     { service: string, message: string }
model Diagnosis@v1 { severity: string, kind: string }

agent classify {
  llm:     "claude-haiku-4-5"
  accept:  Alert@v1
  produce: Diagnosis@v1
  prompt:  "Classify {input.message} on {input.service}."
}

saga apply_fix(alert: Alert@v1, cap: cap[kube.apply, audit.event]) {
  intent "triage and apply fix for {alert.service}"
  step apply {
    do   { kube.apply(fix_for(classify(alert)?))? }
    undo { kube.delete("deployment/{alert.service}-fix")? }
  }
}
```

> Versioned schema · typed agent · saga with mandatory compensation · `cap` passed explicitly · `intent` attached. **One file, one grammar, no extra runtime.** The trace records every call; `aeris replay` rebuilds the run offline.

---

# Capture, not control — the honest promise

> An LLM cannot be made deterministic. That is a **physical property of the model**, not something a language can change.

**What Aeris does instead**

- Every `ai.*` call → **JSONL trace** with prompt, model, response, tokens, timestamp. **The full payload**, not a summary.
- `aeris replay <trace_id>` → **bit-identical** on the deterministic subset. No network, no model cost.
- `aeris trace diff <a> <b>` → field-level divergence aligned by `(scope, ordinal)`. The foundation for **regression bisect** of LLM behaviour.

**The promise, stated honestly**

- **Not** "deterministic LLM code" — physically false.
- **Yes** **reproducibility after the first execution** — which is what audit, debug, and post-mortem actually need.

> First run stochastic. **Every replay identical.**

---

<!-- _class: divider -->

# What we observed · what we refused · open questions

> Observations from one experiment. We do not claim they generalize.

---

# What we observed while building this

> Not measurements — **observations**, recorded in the trace.

- **Fewer syntactic alternatives → fewer drift bugs.** When the grammar has exactly one way to write a thing, the model converges on it; `aeris fmt` becomes a no-op rather than a source of churn.
- **Adding `intent` changed the *kind* of bugs we saw.** Fewer "what was this supposed to do?" — the intent string is in the trace, the purpose is recoverable from a single line.
- **Trace + replay made LLM debugging tractable.** Every regression becomes a reproducible test: same trace → same response → same outcome. The bug is in the code, not in the model's mood.
- **The thesis-driven loop held across ~50 milestones.** The model could draft implementations across a wide surface while the acceptance checks kept the language internally consistent.

> These say something about *this experiment*. **We don't claim they generalize.**

---

# Honest limits

> What this experiment does **not** show.

- **First run stays non-deterministic.** Replay is reproducibility *after* — never *instead of*. If the first run produced something wrong, you have a trace, not a fix.
- **In-body correctness is not verified.** `cap` tells you a function can reach `api.acme.com`. It does not tell you the function posts the right payload there. That's still **tests, property checks, backend RBAC**.
- **`cap` broadening is a process problem.** The `surface.lock` diff makes a widened authority visible in every PR. The *enforcement* lives in CI and review, not in the runtime.
- **The methodology might not scale.** ~50 milestones is small. We don't know if a 500-milestone language stays this coherent under the same loop.

> Aeris is the **first defensive layer**, not the only one. An experiment, not a complete system.

---

# What we deliberately refused

> Each refusal pays a declared cost. We chose to pay it.

- **No automatic formal proofs.** An SMT solver would prove more than `requires:` / `ensures:`. Its verdicts depend on the machine and on heuristics — **non-determinism in the tooling**, exactly what the language is trying to control.
- **No capability inference.** Inferring `cap` from the body would be convenient. It would also let a PR silently broaden authority while the diff looks innocent. **It would break code review.**
- **No mutable dependency references.** No `latest`, no `*`, no movable Git tags. The answer to *"what version is in this build?"* is always one line in `aeris.toml`.
- **No unsigned native plug-ins.** Only L2 modules signed by the Aeris registry key load. Cost: ecosystem grows slower; benefit: no opaque native authority.

> Each refusal is a deliberate trade — kept to preserve **what is in the source is the truth**.

---

# Open questions

> What we'd want to study next, with the honest disclaimer that we don't have answers yet.

- **What's the right granularity for `intent` blocks?** Too coarse: useless. Too fine: noise. We don't have a principled answer.
- **How much of the result comes from the language vs from the methodology?** We can't yet separate *"the grammar helps"* from *"the thesis-first process helps"*.
- **Does this generalize to a typed language?** Aeris is dynamically typed. The same shape with static types might be more verifiable — or might collapse under the cognitive cost we explicitly chose to avoid.
- **Does the loop scale beyond ~50 milestones?** Open. We don't know.

> A starting point, **not a conclusion**.

---

<!-- _class: divider -->

# Thanks

> An open project. Questions, feedback, contributions welcome.

> Sources: `docs/thesis.md`, `docs/language.md`, `docs/project.md`, `docs/plan.md`, `docs/cheatsheet.md`.
