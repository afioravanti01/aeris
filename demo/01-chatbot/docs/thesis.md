# Aeris v0.2 — Thesis

> *"A small, dual-target language where the boundary between deterministic
> code and AI / IO / network code is a **value passed by parameter**, the
> rollback of every external write is **mandatory**, and the supply chain
> is **content-addressed by construction**."*

This document explains **why** Aeris exists, **what** thesis it commits to,
and **how** that thesis shapes every design decision. It is the foundational
document for the language: the language specification (`language.md`) and
the implementation plan (`plan.md`) are derivations of what is stated here.

---

## 1. The problem Aeris is built to solve

Modern software is increasingly written by Large Language Models. The
generation step is non-deterministic: same prompt, different output. The
runtime step is also non-deterministic: networks fluctuate, LLM responses
vary, clocks advance, file systems mutate. Enterprise customers — banks,
healthcare, public administration, regulated industry — cannot accept code
that is opaque about what it does and unpredictable about when it does it.

The naive responses fail in well-known ways:

- **Sandboxing** (Docker, gVisor, Firecracker) controls *what the process
  can do at the OS level*, but tells you nothing about which functions
  inside the program touch which resources. A helper deep in the call tree
  might silently call `http.get`, and only a code review will see it.
- **Static type systems** (TypeScript, Java, Rust) tell you about data
  shapes but not about effects. A function returning `String` can do
  anything to produce that string.
- **Effect systems with academic provenance** (Liquid Haskell, F*, Dafny,
  Koka) deliver the verification but at a cognitive cost that the average
  enterprise developer cannot pay. They are research languages.
- **Domain-specific frameworks** (Airflow, Temporal, LangChain) impose
  conventions but cannot enforce them. A function decorated as a "step"
  is still arbitrary code.

Aeris takes a fifth path: a small interpreted language with first-class
constructs for ops/AI/governance, where the *visibility* of every effect,
every compensation, and every supply-chain dependency is a **structural
property of the source code** — verified by construction at parse time,
captured by recording at runtime, and reproducible offline thereafter.

---

## 2. Aeris at a glance

Three operational properties are non-negotiable. They define the shape of
the deliverable before any feature is discussed.

- **A single source of truth for ops, AI and governance.** A `.aer` file
  holds the pipeline, the agents, the policies, the contracts, and the
  recorded traces — not split across YAML manifests, prompt files,
  Terraform modules, and `policy.rego`. A reviewer reading a single
  source tree sees the whole system.
- **Zero external dependencies at run time.** Aeris ships as a single
  static binary, < 8 MB stripped. No JVM, no Python runtime, no
  container needed. Download the binary, run a `.aer` file, get a
  trace. The deployment story is `scp` + `chmod +x`.
- **Curly-brace syntax with domain-dedicated constructs.** The surface
  is familiar to anyone fluent in Rust, Go, Swift, TypeScript, Kotlin
  — curly braces, named arguments, `match`, destructuring. What is
  *not* familiar are `saga`, `agent`, `policy`, `intent`, `model`,
  `cap`. The familiar carrier reduces the cognitive ramp; the unfamiliar
  inserts encode the domain in the grammar.

These properties are the floor, not the goal. Above them sit the four
layers of §4 and the five commitments of §8.

---

## 3. The trilemma

Three desiderata pull in different directions:

```
        verifiability (static)
              /\
             /  \
            /    \
           /      \
   readability ── expressiveness
```

A language strong on verifiability tends toward proof obligations,
refinement types, SMT solvers — all of which fail readability. A language
strong on readability tends toward defaults, inference, dynamism — all
of which fail verifiability. A language strong on expressiveness offers
many ways to do each thing — which fails readability *and* makes static
checking intractable.

**Aeris's commitment**: pick a design that lives at the *centroid* of
the triangle, not at any vertex. Concretely:

- Verifiability is **structural**, not semantic. We do not prove that a
  function computes the right answer; we make it impossible to *hide*
  what category of resources the function touches, and we make the
  rollback of every external write **mandatory**.
- Readability is **the constraint**, not the goal. The goal is enterprise
  acceptance; readability is the precondition for human review, which is
  the precondition for enterprise acceptance.
- Expressiveness is **deliberately limited**. One construct per concept,
  no soft keywords, no syntactic alternatives. A grep-able language.

The three together produce a small language. Smaller is the feature.

---

## 4. The four layers

Aeris stacks **four layers**. Each composes with the layer below: *Layer
4 needs Layer 3, Layer 3 expresses itself in Layer 1.* The layers are
the architectural rationale of the entire language; every construct
serves one of them.

### Layer 1 — AI-native syntax
Dense, unambiguous, low-token grammar — the way you write Aeris.
Designed for humans *and* LLMs. Every keyword is reserved (no
position-dependent meaning), every construct has a single canonical
form (`aeris fmt` is total, not partial), every identifier name has
one role.

### Layer 2 — Verifiable semantics
Capabilities are **values** passed by parameter. A function that does
not receive `cap` cannot perform IO, network, AI, or any external
side effect. The compiler refuses `cap.X.Y(...)` if `cap` is not in
scope. The signature is the contract. `requires:` / `ensures:` clauses
declare runtime invariants on inputs, outputs, and world state, with
violations halting the program at the boundary, before damage spreads.

### Layer 3 — Agentic loop
The `saga` construct expresses long-running operations on external
state. Each step declares both `do` and `undo`. The compiler refuses
a step whose `do` writes if `undo` is `noop`. Idempotency keys are
auto-derived from `(trace_id, step_name)` and injected into write
capabilities. Every saga is observable by construction: each step
emits a JSONL trace event. The recovery story is **logical
compensation**, not in-memory rollback — what the language calls
"reversible" matches what production engineers mean by it.

### Layer 4 — Multi-agent orchestration
`agent_net` is a typed dataflow graph of LLM agents. Each node
declares `accept` and `produce` schemas; the runtime validates
messages against these schemas at every edge crossing. Cycles are
rejected at parse time; iteration is expressed via `until:`. The
routing protocol is owned by the runtime (auto-injected into every
agent's system prompt as a JSON-fenced contract), not encoded by
hand in prompt strings.

The four layers are **opt-in by depth**: a throwaway script lives in
L1; a contract-checked utility uses L1+L2; a self-recovering ops
pipeline uses L1+L2+L3; a coordinated multi-agent system uses all
four. Each level the user opts into is paid for by them, not by the
project they're not building.

---

## 5. Why these four layers?

> Code is increasingly **generated** by LLMs and **read** by LLMs —
> for reasoning, debugging, modifying. That changes the design
> constraints of the language itself. The two requirements that
> matter now: **reduce non-determinism** and **make code mechanically
> verifiable.**

### The paradigm shift — WHAT, not HOW

The principal *author* of code is now an LLM. An LLM does not have
a mental model — it has a probability distribution over the next
token. Writing code is, for an LLM, an **intrinsically stochastic**
process.

So the question stops being *"how do I lay out the syntax to be
readable?"* and becomes *"what intentions can I let an agent express
directly, without encoding them as mechanism?"*

- A traditional language asks: *how do I build this?*
- An AI-native language asks: *what do I want built?*

In Aeris, `saga`, `agent`, `intent`, `policy` are not mechanisms —
they are **complete intentions** lifted to first-class constructs.

### Why high abstraction (and not low)

There is an opposite temptation: keep the language *as low as
possible*, close to the hardware, so the LLM has less room to fail.
**Wrong logic.**

An LLM generates correct code with probability proportional to:
- how much the code **resembles its training corpus**, and
- how **constrained** the space of valid completions is by the
  language itself.

High abstraction does both:
- **Fewer decisions** to make → fewer points of failure.
- **Higher signal-to-noise** per token generated.

`agent_net { flow extractor → normalizer }` communicates more
intent than 50 lines of Python — and the LLM that writes it has
less room for bugs because the syntactically valid completions are
fewer.

> *Every construct in the language should encode a complete
> intention, not a mechanism.*

### The four layers, as a response

- **L1 · AI-native syntax** — every token spent on style is a token
  an LLM can hallucinate. **Density + zero ambiguity** = fewer
  hallucinations.
- **L2 · Verifiable semantics** — capabilities-as-values make the
  LLM's intent **mechanically checkable**. The compiler — not the
  human reviewer — catches the hallucinated `http.get` in a
  function that never declared `cap.http`.
- **L3 · Agentic loop** — long-running LLM scripts fail
  unpredictably. Per-step trace + idempotent compensations make
  recovery **deterministic over non-deterministic execution**.
- **L4 · Multi-agent orchestration** — when 3+ agents coordinate,
  the routing protocol *is* the program. Lifting it to a typed
  dataflow graph eliminates coordination-as-prompt-string.

---

## 6. Why-as-grammar

Programming languages historically separated two things: **what the
code does** (semantics) and **why it does it** (documentation, commit
messages, PR descriptions). The separation was necessary for humans
— the machine does not need the *why*.

### The cost of that separation, in the agentic era

An LLM reading a `.aer` file *without* the *why* must reverse-engineer
purpose from mechanics. **Every inference is a point of
non-determinism.**

An agent *executing* code without knowing *why* cannot decide
autonomously whether to continue, stop, or escalate when something
looks off — it has no acceptance criterion against which to judge
unexpected state.

The old way pushes the *why* into commits and tickets — out-of-band
channels the agent never sees. So every run, the agent re-derives the
intent from scratch, with the same probability of being wrong each
time.

### The thesis — *why-as-grammar*

In Aeris the *why* is part of the grammar.

`intent`, `requires:` / `ensures:`, `policy` are *not comments* —
they are **traceable, structurally enforced constructs** that:

- shrink the space of valid interpretations the agent can adopt,
- make the program's purpose **machine-readable**, not just
  human-readable,
- propagate as structured data into the JSONL trace, where another
  agent can consume them.

The result: an agent does not have to *guess* what "right" looks
like — the grammar tells it. And the runtime enforces the omission:
an effectful call without an enclosing `intent` does not parse.

> *The goal is not a language humans write better — it is a language
> agents **execute with more certainty**.*

---

## 7. Three sources of non-determinism

An LLM does not have a mental model — it has a probability
distribution over the next token. Three layers of non-determinism
affect any agentic program. Aeris addresses each at a different
level of the design.

### 7.1 The model itself

Same prompt, different output. Tackled with `temperature=0`, never
eliminated. Aeris's response is **capture, not control**:

- Every `cap.ai.*` call is recorded into the JSONL trace
  (`prompt`, `model`, `response`, `tokens`, `ts`).
- `aeris replay <trace_id>` plays the recorded tape: no network,
  no LLM cost, deterministic.
- The first invocation is non-deterministic; every subsequent
  replay is bit-identical.

We do not promise "deterministic LLM code" — that is physically
false. We promise **reproducibility after the first run**, which
is what audit, debugging, regression testing, and post-mortem
actually need.

### 7.2 Language semantics

Ambiguous constructs force the LLM to *infer*. Aeris closes this:

- All keywords are reserved. No soft keywords, no
  position-dependent meaning. `grep saga` finds every saga.
- One construct per concept. No syntactic alternatives. `aeris fmt`
  is total, not partial.
- Capabilities-as-values, not effect modifiers. The signature
  is the truth about what the function can do; there is no
  hidden state to infer.

### 7.3 The state of the world

Code acts on networks, databases, file systems that change. *This
is what governance addresses.* Aeris's response:

- `intent` ties code to its purpose; mandatory on every effectful
  call, traced as structured data.
- `requires:` / `ensures:` declare conditions on inputs, outputs,
  and world state; violations halt the program at the boundary.
- Capabilities-as-values **isolate** what each function can touch;
  no hidden global access.
- Versioned `model` validates data crossing trust boundaries; an
  ill-shaped LLM response is rejected before it reaches business
  logic.
- `policy` enforces guardrails the model cannot forget — `deny`,
  `limit`, `require`, `audit` rules are evaluated on every matching
  call, not "remembered" by a system prompt.

Aeris does not eliminate non-determinism globally. It makes it
**explicit, isolated, and governable**.

---

## 8. The five commitments

These are the load-bearing thesis statements. Everything in
`language.md` is the realisation of these. Everything in `plan.md` is
the implementation order.

### 8.1 Capabilities are values, passed explicitly

The single most important commitment. The signature of every function
is the truth about what the function can do. There is no global `http`
namespace. There is no `import http`. There is `cap`, a parameter, and
a function that does not receive it cannot do HTTP. Higher-order does
not escape; method-on-value does not escape; method-on-captured does
not escape. Verified by construction: the parser refuses `cap.X.Y(...)`
if `cap` is not a parameter in scope.

Theoretical foundation: object-capability security
(Dennis & Van Horn, 1966; Mark Miller's E language; Caja; Genode OS).
Industrial precedent: Erlang process isolation, Pony's reference
capabilities. **Not novel research** — applied engineering of a
well-understood model.

**Enforcement is a project decision** (v0.3, `language.md` § 8.4.1).
A project that does not need audit may run with
`aeris.toml [caps] enforce = "off"`: the cap discipline is relaxed,
`main` receives `cap[*]`, and the script feels like an interpreted
language without capability ceremony. The structural invariants of
§§ 8.2 / 8.4 (sagas with `undo`, mandatory `intent` on writes) also
relax at this level — they are reciprocal to *audit* of authority,
and audit is what the project just opted out of. A project that
*does* need audit flips to `enforce = "strict"` (or the intermediate
`"loose"` middle gear) and recovers the full v0.2 surface
mechanically: `aeris fmt --narrow-caps` derives the per-function
signatures from the body. The commitment in this section is that
the *strict* form remains the canonical one — the off / loose modes
are escape hatches, not substitutes for the discipline they relax.

### 8.2 Side-effects are logically reversible or refused

A pipeline-style construct (`saga`) is the only place where multi-step
operations on external state are allowed. Each step has `do` and `undo`.
The compiler refuses `undo: noop` on a step whose `do` receives a write
capability. Cascading undo is best-effort (the implementation is honest
about this); idempotency keys reduce the practical failure rate.

Theoretical foundation: SAGA pattern (Garcia-Molina & Salem, 1987).
Industrial precedent: Temporal, AWS Step Functions, Cadence.
**Not novel** — applied engineering.

### 8.3 The supply chain is content-addressed

Imports use readable aliases bound to blake3 hashes in a committed
lockfile. The runtime refuses to load a library whose hash does not
match. Republishing under the same alias requires a new hash, which
appears in the lockfile diff, which appears in the PR review.

Industrial precedent: Cargo, Go modules with `GOSUMDB`, npm with
`integrity:`, Nix store paths, Unison. **Not novel** — applied
engineering.

### 8.4 The *why* is in the grammar, not in the trace

`intent "..."` is a scope-level construct. The runtime emits
`intent_enter` / `intent_exit` events to the JSONL trace, and every
trace event emitted inside the body carries the active intent string.
For functions or blocks that hold write capabilities (`cap.audit`,
`cap.kube.apply`, `cap.http.post`, `cap.fs.write`, `cap.ai.*`), an
enclosing `intent` is **mandatory**. The compiler refuses an effectful
call without one.

Why this matters: the *purpose* is now a grammatical ancestor of every
side effect, not a commit message. An LLM-generated PR cannot hide a
write behind silence: it must declare intent in the source.

This does not verify that body matches intent. It makes the omission
of intent impossible.

### 8.5 Runtime non-determinism is captured, not eliminated

The runtime records every `cap.ai.*` call, every `cap.clock` read,
every `cap.random` read, into the trace JSONL. `aeris replay <trace_id>`
re-runs the program against the recorded tape: same LLM responses,
same clock, same random. The replay is **bit-identical** for the
deterministic subset of the program, and the non-deterministic subset
is **fixed** by the recording.

This is the realistic enterprise commitment. We do not promise
"deterministic LLM code" — that is physically false. We promise
**reproducibility after the first run**. Audit, debugging, regression
testing, post-mortem all become offline operations.

---

## 9. What Aeris deliberately refuses

These are temptations rejected on principle. Each refusal is paid for
by a corresponding cost the user accepts.

### 9.1 No SMT, no refinement types

We considered (and rejected) refinement types backed by Z3. The
reasons for refusal:

- **Compiler determinism breaks**: SMT solvers may time out on one
  machine and not on another. The compiler verdict becomes machine-
  dependent. This is non-determinism *in the development tooling*,
  which is worse than the runtime non-determinism we are trying to
  control.
- **Error messages are unactionable**: every effort language with
  SMT (Liquid Haskell, F*, Dafny) struggles to render solver failures
  in human terms. Without a heroic engineering effort on the error
  renderer, the verified tier becomes an island for type-system
  specialists. Enterprise developers do not adopt it.
- **Marginal benefit on real code**: most enterprise predicates are
  range checks, length bounds, regex membership, enum membership.
  All are SMT-decidable but also expressible as runtime `where`
  clauses with negligible cost. The proof-time vs check-time
  tradeoff is small.

**The cost we accept**: contracts (`requires:` / `ensures:`) are
runtime-checked. A violation halts the program with a typed error.
We do not prove absence of violations; we make them loud.

### 9.2 No tier system

We considered a `draft / standard / verified` tier system with
escalating rigour. We rejected it because tier-boundary semantics
proved inherently messy: what happens when a `standard` file imports
a `draft` function? Three reasonable answers, none ergonomic. A single
rigour level is simpler.

**The cost we accept**: the rigour is uniform. A throwaway script
pays the same capability-passing tax as a production pipeline. We
mitigate with sensible defaults and minimal stdlib in `cap`.

### 9.3 No capability inference

We considered inferring capability sets from function bodies and
displaying them on hover. We rejected it because:

- An internal change to a callee silently changes the capability
  surface of every caller. Diff in PR review becomes deceptive.
- LLMs reading code rely on signatures. Hidden inference defeats
  the readability goal.
- The implementation requires effect-row unification with subtyping,
  which is technically heavy.

**The cost we accept**: signatures are verbose. A function that
needs three sub-capabilities lists three. We compensate with a
formatter that minimises capability declarations to the actual
usage (a linter, not an inference) — the developer sees the minimum
they wrote, not less.

### 9.4 No soft keywords

A soft keyword is a token usable as an identifier in unrelated
positions. They require position-dependent lookahead in the parser
and produce subtle bugs (e.g., a `time` literal silently failing to
parse because the lexer split it into two tokens).

**Aeris reserves all keywords.** A developer writes `q` instead of
`step` if they need a variable. The cost is small; the gain is
that `grep step` finds every step in the codebase, and an
LLM-generated identifier never collides with a keyword by surprise.

### 9.5 No `import` of mutable references

There is no `latest` version specifier. There is no `*`. There is no
mutable git tag form. Every `use X@vN.M.P` either matches a `[deps]`
entry in `aeris.toml` exactly or fails at resolution time.

**The cost we accept**: republishing a library requires a new alias
or a lockfile bump. The gain: "what version is in this build?" is
a textual question answerable from `aeris.toml` without running
anything.

### 9.6 No native shared-object plug-ins

Aeris does not load `.so` / `.dll` modules at runtime. User-land
libraries fetched through the manifest / registry are pure `.aer`
source; the bytes are blake3-hashed before they execute. Runtime
extensions that need native code (HTTP fetcher, `kubectl` /
`docker` subprocess wrappers, future TLS pinning) live as native
cap handlers in `aeris-core` and require a project release.

**The cost we accept**: a vendor that wants a custom `cap.X.Y`
must fork or PR `aeris-core`. The gain: static-cap-scope (§3.1)
remains a property the runtime can actually verify. A `.so`
plug-in mechanism would let any binary on disk introduce a fresh
effect surface that the M2 checks could not see, which would
silently break every guarantee in §8.

ADR-0002 (`docs/decisions/0002-ops-integrations-as-stdlib.md`)
records the corresponding positive commitment: Docker,
Kubernetes, and an extended shell surface are **Tier-1 stdlib**,
not deferred — they ship as native cap handlers in
`aeris-core`, which is exactly the place this refusal allows
them to live.

---

## 10. The five surgical patches that make Aeris enterprise-credible

These patches are commitments at the language level. They are the
operationalisation of §8.

### V1 — Capability minimisation

The compiler computes, for each function, the *actually used* sub-set
of `cap`. If the signature declares more, the compiler emits a fix-it:
*"narrow signature to `cap.ai_cached`"* with auto-fix via `aeris fmt
--narrow-caps`. Every signature is the **minimum** truth about what the
function touches.

### V2 — Mandatory `intent` on write-effectful calls

The parser refuses a function or block that calls
`cap.audit.*`, `cap.kube.apply`, `cap.http.post`, `cap.fs.write`,
or `cap.ai.*` without an enclosing `intent "..."`. The check is static.
The error is rejected at compile time, not at runtime.

### V3 — Effect surface in lockfile

Each public function's effect surface is recorded in
`.aeris/surface.lock`. A PR that broadens any surface (adds a sub-cap)
must regenerate the lockfile; the diff appears as the first hunk in
review. Surface contractions do not require re-locking.

### N1 — Idempotency key auto-derivation

Every write capability injects an idempotency key derived from
`hash(trace_id || step_name || invocation_index)`. HTTP calls add
`Idempotency-Key:` header. K8s API calls add a client token.
SQL writes can be wrapped in transactions keyed on it. Replay of a
saga step that already completed becomes a no-op at the backend.

### N2 — Deterministic clock and random under replay

`cap.clock.now()` and `cap.random.next()` derive from the trace ID
during replay; in live runs they use the wall clock and OS RNG. The
recording captures every reading. Replay is bit-identical for the
deterministic subset.

### N3 — LLM tape recorder by default

Every `cap.ai.*` call records `(prompt, model, response, tokens, ts)`
into the trace. `aeris replay <trace_id>` plays the tape: no network
required, no LLM cost, deterministic. Recording is **always-on**, not
opt-in.

### N4 — Network egress allowlist as capability parameter

`cap.http` is constructed from an allowlist declared in `aeris.toml`.
A `cap.http.get(url: "evil.com")` against an out-of-list domain raises
a runtime `PolicyViolation` and emits a trace event. Defense-in-depth
against adversarial LLM injections.

---

## 11. The honest limits

These are **not** addressed by Aeris and we say so up front, because
overpromising is the failure mode that erodes the credibility of the
language with the audience that matters (compliance, audit, security).

### 11.1 The first LLM call is non-deterministic

N3 (tape recorder) makes runs reproducible *after* the first execution.
The first execution is, by physics of the LLM, non-deterministic. No
language feature changes this. Replay-able, not deterministic.

### 11.2 Logic inside legitimate capability is not verified

If a function legitimately holds `cap.audit.write` and the body writes
the wrong actor for the wrong action, Aeris does not catch it. The
language gives you visibility (signature) and obligation (intent,
saga). Correctness of body logic is the responsibility of: tests,
property-based checks, code review, staging environment, RBAC at the
backend. Aeris is the first defensive layer, not the only one.

### 11.3 Capability over-broadening is a process problem

An LLM that requests `cap.http` when `cap.http.allowed(["api.x.com"])`
would suffice is not blocked by the compiler. The lockfile diff makes
the request visible. Catching it is human review or a CI policy
(`deny: cap.http unconstrained` for production branches). The language
provides the visibility primitive; the enforcement primitive lives in
CI and code review.

### 11.4 Cascading undo is best-effort

Saga step 4 fails; undo of step 1, 2, 3 is attempted in reverse.
The undo of step 2 may itself fail. We retry with idempotency keys.
After retries, we surface a `PartialFailure` event in the trace and
require human resolution. This is the SAGA pattern's known limit;
no language can paper over it without lying about distributed-systems
physics.

### 11.5 Compiler ergonomics is not free

The "errors translated into human language" promise — the difference
between `Z3 unsat` and *"step `classify` line 47: confidence not
provably in [0, 1]; missing clamp"* — is an engineering investment in
the compiler. Aeris plans for it (see `plan.md` M5). Without it the
language remains less ergonomic than its peers despite being more
correct.

---

## 12. Comparison to industrial alternatives

Aeris occupies a position adjacent to but distinct from existing
ecosystems. Useful for orientation:

| Tool | What it provides | Where Aeris differs |
|---|---|---|
| **Temporal / Cadence** | Workflows with retries and saga semantics | Temporal is a service that orchestrates external code; Aeris is a language whose source code *is* the orchestration. |
| **Airflow / Dagster** | DAG-based pipelines | Airflow steps are arbitrary Python; Aeris steps declare capabilities and undo in the syntax. |
| **LangChain / LlamaIndex** | LLM orchestration framework | LangChain is a Python library; Aeris makes LLM call structure a language primitive, with mandatory intent and tape recording. |
| **Nix / Guix** | Pure functional package manager with content-addressed store | Aeris borrows content-addressing for imports; it does not aspire to be a build system. |
| **Pony** | Reference capabilities for concurrency | Pony's caps describe aliasing for shared memory; Aeris's caps describe authority for external effects. Different domain, similar mechanism. |
| **Wasm Component Model** | Capability-based component composition | The closest cousin. Aeris can be seen as a high-level surface for Wasm components: every capability could in principle be a component import. |
| **Rust** | Ownership for memory safety | Rust's `&mut self` is the conceptual ancestor of Aeris's `cap`. We borrow the discipline; we do not borrow the borrow checker. |

The intellectual lineage is **object-capability security + SAGA + content
addressing + tape-and-replay**. None novel; all proven; the contribution
is the synthesis at a specific abstraction level (a small interpreted
language for ops/AI/governance).

---

## 13. Success criteria

Aeris is successful if and only if:

1. A compliance officer who has never seen the language can read a
   function signature and answer *"what external resources does this
   touch?"* in under 30 seconds, with high confidence, without reading
   the body.
2. Every effectful call site in an Aeris codebase has a declared *why*
   (the enclosing `intent`) propagated to the trace.
3. A failed run produces a JSONL trace from which the run can be
   replayed bit-identically offline (modulo physical first-time effects).
4. A saga whose middle step fails leaves the system in a defined state:
   either fully forward-completed, fully backward-compensated, or
   surfaced as `PartialFailure` for human resolution. No silent
   half-states.
5. A supply-chain attack that swaps a published library's bytes does
   not execute because the lockfile hash mismatch is fatal.
6. An LLM-generated PR that adds a network call to a previously
   air-gapped function fails review because the surface diff appears
   as the first hunk in the review.

If any of these is not achievable on the implementation, the language
has failed its thesis. The plan in `plan.md` is the path to all six
being achievable.

---

## 14. The frame

> The thesis of Aeris is *"a small DSL where the **visibility** of
> effects, side-effect compensations, supply-chain integrity, and
> intent are **structural properties of the source code** — verified
> by construction at parse time, captured by recording at runtime,
> and reproducible offline thereafter"*.
>
> We do not aim to *prove* code correct. We admit we cannot, and
> instead make the **rationally suspicious code physically incapable
> of hiding**. Less ambitious than a proof assistant. More
> deliverable than a research language. More enterprise than a
> framework.
>
> Single source of truth. Zero dependencies. Curly braces with
> domain-dedicated constructs. Capabilities as values. Sagas with
> mandatory compensation. Content-addressed supply chain. Why-as-
> grammar. The four layers compose. Every line that follows in
> `language.md` and `plan.md` is the realisation of this paragraph.