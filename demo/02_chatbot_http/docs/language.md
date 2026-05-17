# Aeris v0.2 — Language Reference

> *The realisation of `thesis.md` as a grammar.*
>
> Every construct in this document exists for one reason: to make a
> structural property from `thesis.md` § 8 mechanically checkable, or
> to expose a runtime fact from § 10 to the trace. If a construct does
> not serve one of those, it does not belong here.

This document is the language specification for **Aeris v0.2**. It is
authoritative for surface syntax, type system, capability rules,
contracts, sagas, agents, and tooling contracts. Where it conflicts
with `thesis.md`, the thesis wins; where it conflicts with `project.md`,
this document wins (`project.md` lists constraints, this lists
realisations).

---

## 0. Document conventions

- `monospace` is source code or a CLI invocation.
- `<placeholder>` is a syntactic non-terminal (see § 26).
- *Italics* mark emphasis. **Bold** marks load-bearing claims.
- Every code block is parseable Aeris unless prefixed with `// pseudo-code`.
- "MUST", "MUST NOT", "MAY" follow RFC 2119 conventions when capitalised.

---

## 1. Overview — the four layers in one example

The four layers of the thesis (§ 4) are visible in a single program.

```aeris
use io, json, http, fs                               // L1 — built-in stdlib
use ai, kube, audit                                  // L2 — native cap handlers
use deploy from "github.com/acmecorp/aeris-devops" deploy@"1.2.0"   // L3 — external

model Invoice@v1 {                                   // versioned schema
  id: uuid
  amount: decimal where amount > 0
  customer: string where len(customer) <= 64
}

policy production_egress {                           // L2 — runtime guardrail
  match:  http.*
  deny:   url.host not in ["api.acme.com"]
  audit:  { url, method }
}

fn total(items: list<Invoice@v1>) -> decimal {       // pure leaf (no cap)
  items.fold(0, fn(acc, it) { acc + it.amount })
}

agent classify {                                     // L4 — single LLM unit
  llm:     "claude-opus-4-7"
  prompt:  "Classify the invoice as { utilities, software, travel, other }."
  accept:  Invoice@v1
  produce: Category@v1
}

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
    do   { for it in batch { http.post("https://api.acme.com/charge", it)? } }
    undo { for it in batch { http.post("https://api.acme.com/refund", it)? } }
  }

  step ledger {
    requires: charge.ok
    do   { kube.apply(ledger_manifest(batch))? }
    undo { kube.delete(ledger_manifest(batch))? }
  }

  step record {
    requires: ledger.ok
    do   { audit.event("settle.complete",    { count: len(batch) }) }
    undo { audit.event("settle.rolled_back", { count: len(batch) }) }
  }
}

agent_net invoice_pipeline {                         // L4 — typed dataflow
  flow extract -> classify -> route
  until: classify.confidence > 0.95 || iterations >= 3
}
```

The reader sees, **without running anything**, that:

- `total` cannot do IO (no `cap` parameter).
- `settle` writes to `http`, `kube`, `audit` — and *only* those — and
  HTTP is reachable *only* on `api.acme.com`, K8s *only* on
  `prod-eu-1`. The signature is the truth.
- The body's `http.post(...)` and `kube.apply(...)` are not free-form
  calls to a global namespace: they resolve against the `cap`
  parameter listed in the signature (§ 8.2). A function without `cap`
  cannot even *parse* such a call.
- The *why* is on the screen (`intent "..."`), not in a commit message.
- Every external write has a paired `undo`.
- The supply chain pin `deploy@"1.2.0"` is reproducible because it is
  bound to a blake3 hash in `aeris.toml` (§ 24).

That is the whole language, in spirit, on one page.

---

## 2. Lexical structure

### 2.1 Source files

- Encoding: UTF-8, no BOM. Newlines are `\n`. Tabs are forbidden in
  source; `aeris fmt` rewrites them to four spaces.
- File extension: `.aer`.
- Maximum line width: 100 columns (formatter-enforced, not parser-enforced).

### 2.2 Identifiers

- Pattern: `[A-Za-z_][A-Za-z0-9_]*`.
- Idiomatic case: `snake_case` for values, functions, modules;
  `PascalCase` for types, models, enums, agents, sagas, policies,
  agent nets; `SCREAMING_SNAKE` for constants.
- Identifiers are case-sensitive. Aeris does not support Unicode in
  identifiers — names appear in trace JSONL and must round-trip
  through grep without locale concerns.

### 2.3 Reserved keywords (final, frozen)

```
agent      agent_net  and        as         await      break
cap        catch      const      continue   defer      deny
do         else       ensures    enum       every      extends
false      flow       fn         for        from       if
in         intent     is         let        limit      loop
match      model      not        or         policy     property
pub        raise      record     require    requires   retry
return     saga       spawn      step       test       timeout
true       type       undo       until      use        var
when       where      while      with
```

The v0.3 additions over v0.2 are `catch`, `defer`, `every`, `extends`,
`loop`, `retry`, `timeout`.

**No soft keywords.** A user variable named `step` is a syntax error;
the developer writes `q` or `s` instead. This is a deliberate cost —
see thesis § 9.4. Every keyword is greppable across the codebase.

The structural-block field markers (`llm:`, `prompt:`, `accept:`,
`produce:`, `match:`, `audit:`, `do`/`undo` inside `step` etc.) are
**not** global keywords: the parser recognises them only at the LHS
of a `key: value` form inside a specific declarative block (`agent
{ }`, `policy { }`, `step { }`). Outside those blocks they are
ordinary identifiers. The lexing is unaffected — this is structural
parsing, not the position-dependent lexing § 9.4 prohibits.

### 2.4 Literals

```aeris
42         42_000        0xff       0b1010       // integer
3.14       1.5e-3                                // float
true       false                                 // bool
"hello"    "with {name}"     "x = {f(g(1, 2))}"  // string, interpolation
b"raw"     b"\xff\x00"                           // bytes
'\n'                                             // char
[1, 2, 3]                                        // list
{ a: 1, b: 2 }                                   // record / map
("ok", 42)                                       // tuple
2026-05-07                                       // date literal
2026-05-07T08:30:00Z                             // timestamp literal
3s   500ms   2h   7d                             // duration literal
```

Date, timestamp, and duration are *literal* forms recognised by the
lexer (the pattern `\d{4}-\d{2}-\d{2}` is always a date, never an
arithmetic subtraction). This is not a soft keyword — the recognition
is purely lexical and unconditional, never position-dependent.

**String interpolation (M16).** Inside a double-quoted string literal,
a `{` introduces an interpolation segment that ends at the matching
`}` (braces nest, so `"x = {f(g(1, 2))}"` is one segment with body
`f(g(1, 2))`). The body is parsed as an expression and the runtime
concatenates the stringified result. A literal `{` or `}` is written
as `\{` or `\}`; there is no `{{`/`}}` doubling rule. An empty segment
`"{}"` is a lex error — escape it as `"\{\}"` when you need the empty
JSON object. The legacy `\(...)` form from v0.2.0-dev is removed in
v0.3; `aeris fmt --migrate-strings` does the one-shot rewrite.

### 2.5 Comments

```aeris
// line comment
/* block comment */
/// doc comment — attached to the following declaration
```

Doc comments are ingested by `aeris doc` and emitted as JSONL into the
project's documentation index. They are **not** part of the trace
(intent is — § 10).

### 2.6 Operators (precedence high → low)

```
.   ?               (field, error-propagate; postfix ? binds tightest)
unary -  not
*  /  %
+  -
<<  >>
&    |    ^
==  !=  <  <=  >  >=
is   as
and
??                  (null-coalesce — Ok(v)/Some(v)/value → v, Err/None/() → rhs)
or
..  ..=             (range, inclusive range)
=  +=  -=  *=  /=  %=
```

There are **no** ternary, comma operator, increment/decrement, or
overload-able operators. `aeris fmt` is total.

The `??` operator (v0.3) takes any value on the left and an
expression on the right. It returns the left value when it is
"present" — `Ok(v)`, `Some(v)`, or any non-wrapper value — and
evaluates the right when it is `Err(_)`, `None`, or `()`. Right-
associative: `a ?? b ?? c` is `a ?? (b ?? c)`. The right-hand side
is short-circuited like `or`.

---

## 3. Modules, files, projects

### 3.1 Project layout

```
my-pipeline/
├── aeris.toml            // dependency pins, cap allow-lists, surface
├── .aeris/                 // build cache, surface lock, traces
│   ├── ext/                // fetched external libraries by hash
│   ├── surface.lock        // public effect surface (V3)
│   └── traces/             // recorded JSONL traces
├── src/
│   ├── main.aer            // entry: must define `fn main(cap) -> ...`
│   ├── invoices.aer
│   └── ledger.aer
└── tests/
    └── invoices.test.aer
```

A *project* is the closure of `aeris.toml`. A *module* is a single
`.aer` file. There is no `package` keyword; the file path under `src/`
is the module path.

### 3.2 `use` — imports

```aeris
use io, json, fs, http                                // L1 stdlib (multiple, comma-sep)
use ai, kube                                          // L2 native handlers
use "./lib/utils.aer"                                 // L3 — local source
use utils from "./lib/utils.aer"                      // L3 — namespaced alias
use deploy from "github.com/acmecorp/aeris-devops" deploy@"1.2.0"   // L3 — external
use { rollout, status } from deploy                   // selective re-export
```

- Local and external imports MUST appear in `[deps]` of `aeris.toml`
  with a `blake3:...` hash. The runtime computes the hash of the
  resolved bytes; mismatch is a fatal error before execution.
- L1 and L2 names live in a frozen registry (`aeris-core`); they cannot
  be overridden by a user import.
- An `as` clause renames: `use http as net`.
- Cyclic imports are a parse-time error.

A `use` of a capability-bearing module (`http`, `fs`, `ai`, `kube`,
...) does **not** introduce a global function named `http.post`. It
makes the capability path *resolvable* in scopes that hold a `cap`
parameter (§ 8.2). A function without `cap` cannot use the imported
module's effectful operations even if the module is imported.

### 3.3 Visibility

By default, top-level declarations in a module are *private to the module*.
A leading `pub` makes them part of the module's public surface.

```aeris
pub fn settle(...) -> ... { ... }
pub model Invoice@v1 { ... }
pub policy production_egress { ... }
```

The public surface of a module is what `aeris lock surface` (V3) records
into `.aeris/surface.lock`.

### 3.4 Top-level statements (v0.3, M26)

A module may contain `let` bindings, expression-statements (function
calls), and `for` / `while` / `loop` / `if` blocks **outside** any
`fn`. These run in declaration order on module load, before `main`
(or as the program body when no `main` is declared):

```aeris
let CLAUDE_ARGS = "--print --model claude-sonnet-4-6"
env.set(key: "AERIS_LLM_CLI", value: "claude {CLAUDE_ARGS}")
fs.mkdir("./out")

fn main() { … }
```

A module with no `main` is a valid program: the file *is* the body,
top-level statements are its sequential execution. Module-level
`var` is still forbidden — only immutable `let` and `const` may
bind at module scope.

The runtime allow-list applies to top-level statements exactly as
it does inside `main`: the synthesised cap from `aeris.toml [caps]`
is in scope, and `enforce = "off"` bypasses it.

---

## 4. Types

### 4.1 Primitives

| Type | Range / shape |
|---|---|
| `bool` | `true`, `false` |
| `int` | platform-sized signed (≥ 64 bits) |
| `i8 i16 i32 i64` | fixed signed |
| `u8 u16 u32 u64` | fixed unsigned |
| `f32 f64` | IEEE 754 |
| `decimal` | arbitrary-precision fixed-point (12 fractional digits default) |
| `string` | UTF-8 |
| `bytes` | immutable byte sequence |
| `char` | one Unicode scalar value |
| `uuid` | 128-bit, RFC 9562 |
| `date` | civil date, no zone |
| `timestamp` | UTC instant, ms precision |
| `duration` | signed 64-bit nanoseconds |
| `unit` | the empty tuple `()` |

Numeric conversions are explicit: `x as i64`. There is no implicit
widening — § 9.4.

### 4.2 Collections

```aeris
list<T>           // ordered, growable
set<T>            // hash set; T must be hashable
map<K, V>         // hash map; K must be hashable
tuple<T1, T2>     // fixed arity
option<T>         // Some(T) | None
result<T>         // Ok(T) | Err(err) — the error type is fixed (§ 18)
channel<T>        // bounded MPMC; § 19
```

### 4.3 Records

```aeris
record User {
  id:    uuid
  name:  string
  age:   int  where age >= 0
}

let u = User { id: uuid_v7(), name: "Ada", age: 36 }
let v = User { ..u, age: 37 }            // structural update
```

Record fields are **public by default within the module** and exported
when the record is `pub`. Records are immutable; mutation happens only
through `var` bindings of records-as-values, and Aeris values are
**by-value**: every rebinding produces a fresh structural copy. There
is no aliasing of mutable references; the signature remains the truth.

### 4.4 Enums (sum types)

```aeris
enum Status {
  Pending
  Active(since: timestamp)
  Banned { reason: string, until: option<date> }
}

match s {
  Pending             -> ...,
  Active(t)           -> ...,
  Banned { reason }   -> ...,
}
```

Variants may be unit, positional, or named-record. `match` MUST be
exhaustive; `aeris check` proves exhaustiveness structurally
(see § 17.2 for the rule on guards).

### 4.5 Models — versioned trust-boundary schemas

A `model` is a record decorated with a **mandatory version tag** and
optional **field constraints**. It is the only type the runtime
validates at trust boundaries (LLM responses, network ingress, queue
deserialisation). See thesis § 7.3.

```aeris
model Invoice@v1 {
  id:       uuid
  amount:   decimal      where amount > 0 and amount < 1_000_000
  customer: string       where len(customer) <= 64
  issued:   date         where issued <= today()
  lines:    list<Line@v1> where len(lines) >= 1 and len(lines) <= 200
}
```

Versions are part of the type identity: `Invoice@v1` and `Invoice@v2`
are distinct types, with no implicit conversion. A migration is an
explicit function:

```aeris
fn migrate_v1_to_v2(old: Invoice@v1) -> Invoice@v2 { ... }
```

### 4.6 Type aliases and generics

```aeris
type Email  = string                      // pure rename, no validation
type UserId = uuid

fn first<T>(xs: list<T>) -> option<T> {
  if xs.empty() { None } else { Some(xs[0]) }
}
```

Aeris supports parametric polymorphism on records, enums, and
functions. There is no subtype polymorphism, no trait keyword in user
code, and no bounded generics in user code: all user generics are
parametric over any type. Stdlib generic containers (`list<T>`,
`map<K,V>`, `set<T>`) impose internal bounds (e.g. `K` hashable for
`map`, `set`); those bounds are checked at call sites without exposing
a trait-declaration syntax to user code.

Type aliases are **pure renames**; they introduce no validation. For
validated values that must be checked at trust boundaries, use
`model@vN` (§ 4.5) — refinement on aliases would conflict with the
explicit refusal of refinement types in thesis § 9.1.

---

## 5. Values, bindings, expressions

### 5.1 Bindings

```aeris
let  x = 1            // immutable binding (default)
var  y = 1            // mutable binding (rebindable, function-scope)
const PI = 3.14159    // module-level, constant-folded
```

`var` is **function-scope only**. Module-level `var` does not exist;
only `const` is allowed at module scope. This eliminates ambient
mutable state and makes "no `cap` parameter ⇔ pure" a structural
property (§ 7.2).

Annotation is optional but encouraged at API boundaries:

```aeris
let amount: decimal = 12.5
```

A `let` shadowing in a nested scope is allowed and idiomatic:

```aeris
let s = "  hello  "
let s = s.trim()
```

### 5.2 Expressions

Aeris is expression-oriented. Blocks are expressions:

```aeris
let n = if x > 0 { 1 } else { -1 }
let m = match s { Active(_) -> 1, _ -> 0 }
```

`return` is allowed but rarely needed; the last expression of a block
is its value.

### 5.3 String interpolation

```aeris
let s = "user {u.name} age {u.age}"
let q = "literal braces \{ \}"      // \{ and \} are the only escapes
let z = "nested {f(g(1, 2))} call"  // braces inside the body nest
```

The braces inside a double-quoted string introduce an interpolation
segment (§ 2.4). The body is any expression; the runtime converts
the result to a string and concatenates it. `\{` and `\}` produce
literal braces (no `{{`/`}}` doubling); an empty `{}` is a lex
error (escape it as `\{\}` to get the empty-object literal).

Format specifiers are **not** part of v0.3 — the legacy
`"\(amount:.2)"` form is removed. Callers format explicitly:

```aeris
"price = {round(amount, 2)}"        // numeric rounding
"ts    = {t.to_iso()}"              // timestamp formatting
```

The legacy `\(...)` form from v0.2.0-dev is rejected by the lexer.
`aeris fmt --migrate-strings` performs the one-shot rewrite to the
brace syntax.

### 5.4 Method-call syntax

`x.f(a)` resolves in this priority order:

1. **Module call**: when `x` is a bare identifier naming an L1/L2
   module (`io.println(...)`, `fs.read_text(...)`, `ai.complete(...)`),
   the call dispatches to the module's `f` operation. Cap-gating
   applies (§ 8.2).
2. **Built-in method on the value type**: the runtime ships methods
   on `list`, `string`, `map`, and on a handful of record kinds
   (`Chat`, `Session`). These bind tighter than user-defined
   functions. See § 22 for the full list.
3. **Record field as callable**: when `x` is a record value with a
   field `f` holding a closure, `x.f(a)` calls that closure with `a`.
4. **UFCS**: `f(x, a)` if `f` is a free function in scope.

There is no general method dispatch table for user-defined types;
the four rules above are total.

---

## 6. Statements and control flow

### 6.1 `if`, `match`, `while`, `loop`, `for`, `until`

```aeris
if cond { ... } else if cond2 { ... } else { ... }

match v { p1 -> e1, p2 if guard -> e2, _ -> default }

while cond { ... }
loop { ... }                                  // v0.3 — sugar for `while true`
for i in 0..10 { ... }
for (k, v) in map { ... }
for x in channel { ... }                      // see § 19

until: condition                              // declarative; only inside agent_net
```

`loop { body }` is parsed pre-check as `while true { body }`. Use
`break` to exit; `continue` re-enters from the top.

`break` and `continue` are unlabelled by default; labelled breaks use
`'name:`:

```aeris
'outer: for i in 0..n {
  for j in 0..m {
    if quit { break 'outer }
  }
}
```

v0.3 adds `loop { … }` as sugar over `while true { … }` so script
authors don't have to spell out the condition (§ 6.1, M24.T1).

### 6.2 Range types

`a..b` is half-open, `a..=b` is inclusive. Ranges are values of type
`range<T>` and iterable for orderable `T`.

### 6.3 Block scope

Every `{ }` introduces a scope. `let` shadowing is scope-local. There
is no block-level `var` to outer-scope hoisting.

### 6.4 Time-control sugar — `every`, `retry`, `timeout` (v0.3, M18)

Three block forms wrap common temporal patterns. The body is an
ordinary block; the keyword tags how the runtime schedules it.

```aeris
every 5s {                              // periodic loop
  io.println("tick")
  if should_stop() { break }
}

let r = retry 3, delay: 1s {            // bounded retry on Err
  http.get("https://api.acme.com/status")
}

let r = timeout 30s {                   // bounded wall-clock
  long_running_call()
}
```

- `every D { body }` — re-enters `body` every `D` after the previous
  iteration completes. `break` exits the loop; `continue` skips to
  the next tick. The first iteration runs immediately.
- `retry N, delay: D { body }` — evaluates `body`; if it returns
  `Err(_)`, sleeps `D` and retries up to `N-1` more times. The last
  result (success or error) is the value of the expression.
- `timeout D { body }` — evaluates `body`. The runtime checks the
  wall-clock at every cooperative cancel-point (§ 19.3) and produces
  `Err(err.user("timeout"))` if `D` is exceeded. The block is **not
  preempted** mid-statement; this is honest about the tree-walk
  runtime's limits.

`clock.sleep(D)` is the underlying primitive — a read-classified
operation that blocks the current OS thread for `D`. Under replay,
`clock.sleep` returns immediately and the recorded duration is
re-emitted to the trace; live runs honour the wall-clock.

---

## 7. Functions

### 7.1 Form

```aeris
fn name(arg: T1, arg2: T2, cap: cap[fs.read_file, http.get @ ["api.x.com"]]) -> R
  requires: pred1, pred2
  ensures:  pred_on_result
{
  body
}
```

- Parameters are positional. Callers MAY pass them by name:
  `name(arg2: y, arg: x)`. The order is not enforced when names are
  used; mixing is allowed for trailing parameters only.
- The return arrow `-> R` is optional only when `R = unit`.
- `requires:` and `ensures:` are runtime contracts (§ 9). They are
  *not* SMT-verified — see thesis § 9.1.
- The `cap` parameter is the only legal name for the capability. If
  absent, the function is statically pure (§ 7.2).

### 7.2 Purity is structural — no `pure` keyword

A function without a `cap` parameter is **statically pure**:

- The parser refuses any call from its body to a function that *does*
  declare `cap`.
- The body cannot write `fs.read_file(...)`, `http.get(...)`, etc. —
  the unprefixed call form (§ 8.2) requires a `cap` in scope to
  resolve. A pure function has none, so any such call is a parse
  error before reaching effect classification.

There is no `pure` keyword because there is nothing left for it to add:

- Module-level `var` does not exist (§ 5.1); only `const` is allowed
  at module scope. There is therefore no ambient mutable state to read.
- Closures inherit their parent's purity rules: a closure built in a
  pure context cannot call effectful code.

In short, **no `cap` parameter ⇔ replay-trivial**. The signature is
the declaration:

```aeris
fn add(a: int, b: int) -> int { a + b }                   // pure
fn fee(amount: decimal) -> decimal { amount * 0.029 }     // pure
fn now_iso(cap: cap[clock.now]) -> string                 // not pure
  intent "log timestamp" { clock.now().to_iso() }
```

### 7.3 Higher-order, closures

```aeris
fn map<T, U>(xs: list<T>, f: fn(T) -> U) -> list<U> {
  let var out = []
  for x in xs { out.push(f(x)) }
  out
}

let inc = fn(x: int) -> int { x + 1 }
let plus = fn(a: int) -> fn(int) -> int { fn(b: int) -> int { a + b } }
```

A closure that captures `cap` carries that capability into its call
site. The compiler refuses to call a closure whose declared capability
exceeds the caller's scope. **`cap` does not escape** (thesis § 8.1):
storing `cap` in a record field, in a global, or returning it from a
function whose signature does not name a `cap` return is a parse-time
error.

### 7.4 Default values, variadics, optional

Aeris does **not** support default parameter values, variadics, or
optional parameters. The cost (one construct per concept) is paid at
call sites via named arguments and `option<T>`.

### 7.5 Untyped parameters (v0.3)

Parameters may omit their type when authoring a script. `fn f(x, y)`
is admissible and resolves to the pseudo-type `any` (the dynamic
interpreter does not check parameter types). The strict mode still
encourages typed signatures: `aeris fmt --narrow-caps` does not
infer types, but `aeris check` reports the underlying expression
errors at the body site.

### 7.6 Named arguments (kwargs)

A call may pass arguments by name to match the parameter list of
the callee:

```aeris
fn greet(name: string, greeting: string) -> string { greeting + " " + name }
greet(name: "Bob", greeting: "hello")
greet(greeting: "ciao", name: "Alice")   // order-independent when named
greet("Alice", "ciao")                   // positional also accepted
greet("Alice", greeting: "ciao")         // positional first, then kwargs
```

Named arguments work identically against:
- user-defined functions and closures (matched against the
  closure's parameter list);
- L1 / L2 builtins (matched against a frozen kwarg table in the
  runtime — see § 22);
- record methods on `Chat`, `HttpReq`, `HttpServer`, `AiNetwork`.

Resolution rules (uniform across the three groups above):

- Positional arguments fill leading parameter slots.
- Named arguments fill by name and may appear in **any order** after
  the positional ones.
- A positional argument after a named one is a parse error.
- An unknown name is `EvalError::Type("unknown kwarg `<name>` for
  `<fn>`")`.
- A repeated name is `EvalError::Type("duplicate kwarg `<name>`")`.
- A missing slot (whether positional or via name) raises the
  existing arity error.

Reserved keywords (`match`, `until`, `policy`, `agent`, …) are
admissible as argument names — the v0.1 `network.run(until: "DONE")`
form parses as expected.

---

## 8. Capabilities — the full tree

### 8.1 The capability tree (frozen, two levels)

```
cap
├── io          { print, println, eprint, eprintln, read_line }
├── fs          { read_file, read_text, read_bytes,
│                 write_file, write_text, write_bytes,
│                 walk, stat, exists, mkdir, remove, rename }
├── http        { get, post, put, patch, delete }
├── shell       { exec, pipe }
├── env         { read }
├── clock       { now }
├── random      { next }
├── ai          { complete, chat, embed, tools }
├── kube        { apply, delete, get, watch }
├── docker      { run, build, push, pull, inspect }
├── mongodb     { read, write }
├── minio       { get, put }
├── rabbitmq    { publish, subscribe }
├── audit       { event }
└── trace       { (system; auto-injected, not user-callable) }
```

The tree has **exactly two levels**: `<module>.<operation>`. Each
leaf operation carries an internal **read | write | diagnostic**
classification used by the runtime to gate the V2 mandatory-`intent`
rule (§ 10.1):

| Class | Operations | Triggers V2 |
|---|---|---|
| **read** | `fs.{read_*, walk, stat, exists}`, `http.get`, `env.read`, `clock.now`, `random.next`, `kube.{get,watch}`, `docker.{pull,inspect}`, `mongodb.read`, `minio.get`, `rabbitmq.subscribe`, `io.read_line` | no |
| **write** | `fs.{write_*, mkdir, remove, rename}`, `http.{post,put,patch,delete}`, `shell.{exec,pipe}`, `ai.*`, `kube.{apply,delete}`, `docker.{run,build,push}`, `mongodb.write`, `minio.put`, `rabbitmq.publish`, `audit.event` | **yes** |
| **diagnostic** | `io.{print,println,eprint,eprintln}` | no |

`clock.now` and `random.next` are read-classified but **always
recorded** for replay (N2). `ai.*` is write-classified and
**tape-recorded** (N3). `audit.event` is write-classified — the
append-only log is durable external state.

### 8.2 Calling a capability from a body

A function body writes capability calls **without the `cap.` prefix**.
The parser resolves `<module>.<operation>(...)` against the `cap`
parameter in scope:

```aeris
fn rotate_cert(new_cert: bytes, cap: cap[fs.write_file, audit.event]) -> result<unit> {
  intent "rotate the leaked TLS cert" {
    fs.write_file(cert_path(), new_cert)?       // resolved to cap.fs.write_file
    audit.event("cert.rotated", { path: cert_path() })
  }
}
```

Resolution rules (parse-time, no runtime magic):

- `<module>.<operation>(...)` requires a parameter literally named
  `cap` in lexical scope. Absent → parse error: *"no capability in
  scope to resolve `<module>.<operation>`"*.
- The resolved capability's effect signature MUST contain
  `<module>.<operation>`. If `cap` is `cap[fs.read_file]` and the body
  calls `fs.write_file(...)` → parse error.
- The allow-list attached to the operation in the signature is
  enforced on the call's arguments at runtime (a host outside the
  list raises `PolicyViolation`).

What this notation **does not lose**:

- The signature still names the `cap` parameter and lists its effects
  (with allow-lists). The reviewer reads the signature for the truth
  — the body's unprefixed calls are *consistent with* the signature,
  never additive.
- A function without a `cap` parameter literally cannot type-check a
  call to `fs.write_file(...)`, `http.post(...)`, etc. There is **no
  global `fs` namespace**: `fs.read_file` is shorthand for "the
  `fs.read_file` of the in-scope `cap`", not a top-level function.
  This preserves thesis § 8.1 literally.
- Authority delegation is unchanged: `cap.subset[...]` (§ 8.4) is
  still the explicit way to construct a narrower cap to pass to a
  callee — the call form (`cap.subset[...]`) is a method on the cap
  *value*, not a body call, and retains the explicit `cap.` prefix.

Multiple caps in the same scope (rare):

```aeris
fn forward(
  req: Req,
  cap:        cap[http.get @ ["api.primary.com"]],
  backup_cap: cap[http.get @ ["api.backup.com"]],
) -> result<bytes> {
  http.get(primary_url(req))?              // resolves to `cap` (the param literally named `cap`)
  backup_cap.http.get(backup_url(req))?    // explicit prefix for the other one
}
```

The unprefixed form binds to the parameter literally named `cap`. Any
other capability-typed value MUST be addressed by its name.

### 8.3 Capability narrowing in signatures

```aeris
fn deploy(target: string, cap: cap[kube.apply, kube.get, audit.event]) -> result<unit>
```

The bracketed list is the **effect signature** of the function. Inside
the body the parser refuses any `<module>.<operation>(...)` whose
`<module>.<operation>` is not in the list. **There is no inference**;
the developer writes the set explicitly (§ 9.3 of the thesis).

A capability tree node *implies* its leaves: writing `cap[fs]` permits
every `fs.*` operation. Convention requires writing the narrowest
correct set; `aeris fmt --narrow-caps` rewrites broad declarations to
the minimum the body actually uses (V1).

#### 8.3.1 Allow-lists in the signature

For capabilities whose authority is not purely categorical — HTTP host,
filesystem path, K8s context, S3 bucket, queue name, LLM model — the
signature encodes the allow-list with `@`:

```aeris
fn settle(
  batch: list<Invoice@v1>,
  cap: cap[
    http.post  @ ["api.acme.com", "api.stripe.com"],
    kube.apply @ ["prod-eu-1"],
    audit.event,
  ],
) -> result<unit>
```

The allow-list is part of the **type** of `cap`, not a side property.
A reviewer reads the signature and learns:

- *which* external systems are touched (`http.post`, `kube.apply`),
- *which* concrete endpoints are reachable (`api.acme.com`, ...),
- *which* are categorically unbounded (`audit.event` — append-only
  audit log, no host concept).

Allow-list grammar by capability family:

| Family | Form | Example |
|---|---|---|
| `http.*` | `@ <host_list>` | `http.get @ ["api.acme.com"]` |
| `fs.*`   | `@ <glob_list>` | `fs.write_file @ ["./out/**"]` |
| `kube.*` | `@ <context_list>` | `kube.apply @ ["prod-eu-1"]` |
| `mongodb.*` | `@ <db.collection_list>` | `mongodb.write @ ["app.users"]` |
| `minio.*` | `@ <bucket_list>` | `minio.put @ ["releases"]` |
| `rabbitmq.*` | `@ <queue_list>` | `rabbitmq.publish @ ["events.v1"]` |
| `shell.exec` | `@ <argv0_list>` | `shell.exec @ ["kubectl", "git"]` |
| `ai.*` | `@ <model_list>` | `ai.complete @ ["claude-opus-4-7"]` |

Capabilities without a meaningful allow-list dimension (`clock.now`,
`random.next`, `env.read`, `audit.event`, `io.*`) are written without
`@`. A single-element allow-list MAY drop the brackets:
`http.get @ "api.acme.com"`.

#### 8.3.2 Allow-list intersection with the lockset

`aeris.toml [caps]` declares the **project-wide ceiling** for each
family. A function signature that requests an endpoint outside the
project ceiling is a parse-time error. A signature that requests a
strict subset is unified with the ceiling at construction time
(§ 8.4). Concretely:

- ceiling: `[caps] http.allow = ["api.acme.com", "api.stripe.com"]`
- a function asking for `http.post @ ["api.acme.com"]` — accepted, narrowed.
- a function asking for `http.post @ ["evil.com"]` — rejected at lock time.
- a function asking for `http.post` (no `@`) — accepted, inherits the
  project ceiling.

### 8.4 Capability construction at the entry point

`cap[*]` is **forbidden in user source code**. The only function that
receives a capability without writing its shape is `main`, and `main`'s
shape is **derived from `aeris.toml`**:

```aeris
fn main(cap) -> result<unit> {
  let invoices = load_batch(cap)?
  settle(invoices, cap)?
  Ok(())
}
```

At `aeris run`, the runtime constructs `cap` for `main` by composing
the entries of `aeris.toml [caps]` into a concrete cap value. The
effective signature is printed by `aeris check` and `aeris run` on
start-up:

```
$ aeris run src/main.aer
[aeris] effective main cap:
  http.{get,post}     @ ["api.acme.com", "api.stripe.com"]
  fs.read_file        @ ["/etc/aeris/**", "./data/**"]
  fs.write_file       @ ["./out/**", "./.aeris/**"]
  kube.{apply,get}    @ ["prod-eu-1"]
  ai.complete         @ ["claude-opus-4-7", "claude-haiku-4-5"]
  audit.event
  clock.now, random.next
[aeris] trace: .aeris/traces/01JFZ....jsonl
```

The single source of truth for the project's authority surface is
therefore `aeris.toml [caps]`, made executable through `main`'s
synthesised parameter type.

Inside `main`, capabilities propagate to callees via narrowing:

```aeris
let cap_io   = cap.subset[fs.read_file @ ["./data/**"], audit.event]
let cap_net  = cap.subset[http.post    @ ["api.acme.com"]]
load_users(cap_io)?
publish(cap_net)?
```

`cap.subset[...]` is the only way to derive a narrower cap. It is a
method on the cap *value* and retains the `cap.` prefix (it is not a
capability call; it constructs a new cap). It:

- restricts which sub-paths are reachable;
- narrows allow-lists to a subset of the parent's allow-list;
- never *broadens* — `cap.subset[http.post @ ["evil.com"]]` against a
  parent that did not contain `evil.com` is a parse-time error.

### 8.4.1 Enforcement modes — `off`, `loose`, `strict`

The capability system has **three** modes, selected by a project-wide
field in `aeris.toml [caps]`:

```toml
[caps]
enforce = "off"      # script mode — default for `aeris init`
# enforce = "loose"  # prototype mode — runtime allow-list enforced
# enforce = "strict" # production mode — full v0.2 discipline
```

| feature | `strict` | `loose` | `off` |
|---|---|---|---|
| `main(cap)` cap synthesised from `[caps]` | yes | yes | `cap[*]` |
| `NoCapInScope` (E65) on fn without `cap` | error | suppressed | suppressed |
| `OpNotInCapSignature` (E65) on declared cap | error | error | suppressed |
| `CapStarInUserCode` (E65) | error | error | suppressed |
| `MissingIntentForWriteCall` (E66) | error | error | suppressed |
| `SagaStepUndoNoopWithWriteDo` (E67) | error | error | suppressed |
| `BareModelWithoutVersion` (E68) | error | error | error |
| `AgentNetCycle` (E70) | error | error | error |
| `AllowListOutsideLockset` (E71) | error | error | suppressed |
| Runtime allow-list (`http`, `fs`, `ai.models`, …) | enforced | enforced | bypassed (`cap[*]`) |
| Trace, replay, `model@vN` validation, policies | on | on | on |

**`strict` mode.** The full v0.2 behaviour described in §§ 8.1–8.4.
Every function that calls a capability operation declares its
`cap[...]`. `intent` is mandatory on writes. Sagas need explicit
`undo` on every write step. The manifest's allow-list is the
project-wide ceiling.

**`loose` mode (alias `required = false`).** The body-resolution
rule is relaxed — a function *without* a `cap` parameter may freely
call any capability operation. Functions that *do* declare a `cap`
parameter are still checked normally: opting in to the discipline
restores its full strength. The runtime allow-list stays in force;
an unauthorised host or path raises `PolicyViolation` at the call
site.

**`off` mode (new in v0.3).** Script-friendly. The whole cap
discipline is relaxed: no `cap` parameters needed, no E65/E66/E67/E71,
no runtime allow-list — `main` receives `cap[*]`. The trace, replay,
`model@vN` validation, and `policy` blocks **stay** available as
voluntary annotations; the only thing turned off is *cap
enforcement*. Intended for single-author scripts and prototypes
where audit is not a concern.

**Back-compat.** The legacy `required = true | false` form is still
accepted: `true` → `strict`, `false` → `loose`. When both `enforce`
and `required` are present, `required` (read second) wins on the
boolean axis — projects already on `required = …` keep working.

**Migration ladder.** A script grows up in three steps:

1. *Script.* `enforce = "off"`. Write code like in v0.1; iterate fast.
2. *Prototype.* Flip to `enforce = "loose"`. The manifest now caps
   what the program can reach at runtime; the static check still
   does not force `cap` annotations.
3. *Production.* Flip to `enforce = "strict"`. Run
   `aeris fmt --narrow-caps` to derive each function's signature
   from its body. Apply the diff. `aeris check` flags every
   remaining gap.

The default for `aeris init` is `enforce = "off"`. The promoted
project moves up the ladder when audit, supply-chain, or PR-review
needs become real.

### 8.5 Capability minimisation (V1)

`aeris fmt --narrow-caps` analyses each function body and rewrites the
signature to the actually-used set, including allow-list narrowing.
The tool is a **linter**, not an inferencer: it never *removes* a
declared capability silently — it emits a diff the developer applies.
This preserves PR-diff visibility (§ 9.3 of the thesis).

The intended authoring pattern is **generation loose, fmt tight**: an
LLM (or a human) writes a coarse signature like `cap[http, kube,
audit.event]`; `aeris fmt --narrow-caps` derives the narrow form with
allow-lists from the body's actual calls. Verbose signatures are a
*derived* property of compiled code, not a generation cost.

### 8.6 Effect surface (V3)

`aeris lock surface` computes, for every `pub` function, its closed
effect set (the union of caps it transitively reaches), and writes it
to `.aeris/surface.lock`:

```toml
[surface."src/invoices.aer".settle]
caps       = ["http.post", "kube.apply", "audit.event"]
allow.http = ["api.acme.com", "api.stripe.com"]
allow.kube = ["prod-eu-1"]

[surface."src/invoices.aer".total]
caps = []
```

A PR that **broadens** any surface (adds a sub-cap or expands an
allow-list) MUST regenerate the lock. The diff is the first hunk in
the review (success criterion 6 of thesis § 13). Surface contractions
do not require relocking.

### 8.7 What `cap` cannot do

- `cap` cannot be stored in a record field.
- `cap` cannot be assigned to a `const` or to any module-level binding.
- (Module-level `var` does not exist — § 5.1.)
- `cap` cannot be returned from a function unless the return type is
  itself a `cap[...]` type.
- `cap` cannot be sent through a `channel<T>`.
- `cap` cannot escape into a `spawn { }` body that does not declare
  it as a capture (passed via `cap.subset[...]` at the spawn site).
- The literal `cap[*]` is rejected in any user source file. The only
  cap with full project authority is `main`'s synthesised parameter
  type, derived from `aeris.toml`.

These rules are checked by the parser; violations are not "warnings".

---

## 9. Contracts — `requires`, `ensures`, `where`

### 9.1 Where contracts live

```aeris
fn discount(amount: decimal, pct: decimal) -> decimal
  requires: amount >= 0
  requires: pct >= 0 and pct <= 1
  ensures:  result >= 0 and result <= amount
{
  amount * (1 - pct)
}
```

- `requires:` is checked at function entry, before any body code runs.
- `ensures:` is checked at function exit, on every return path. The
  identifier `result` refers to the returned value.
- `where` clauses on record/model fields are checked at construction.
- `where` clauses on `match` arms gate the arm.

### 9.2 Failure mode

A contract violation halts execution with `ContractViolation { kind,
fn_name, location, expr_source }`. It is never silenced. It is **not**
caught by `?` (§ 18). It is logged into the trace and the process
exits with code `64`.

### 9.3 What contracts do not do

- They do not constitute proofs (§ 9.1 of thesis).
- They do not narrow types: a `where amount > 0` on `decimal` does
  not produce a `positive_decimal` type. The check is at the boundary
  only.
- They do not propagate. A function that returns a value violating the
  caller's `requires:` is still entered — and the caller fails on its
  own `requires:`. No "contract widening".

---

## 10. Intent — the why-as-grammar

### 10.1 Where `intent` is mandatory

The parser refuses any **write-effectful call** (write-classified
operations, § 8.1) outside an enclosing `intent`. The check is
**lexical, not data-flow**: the nearest static ancestor in the AST
must be an `intent` block, or a saga / agent-shaped construct
declaring its own `intent`.

Diagnostic operations (`io.print*`) and read-classified operations do
**not** trigger the V2 rule. They may still appear inside an `intent`
block; they are not required to.

### 10.2 Forms

Block-level (the canonical form):

```aeris
intent "rotate the leaked TLS cert" {
  fs.write_file(cert_path, new_cert)?
  audit.event("cert.rotated", { path: cert_path })
}
```

Saga-level (always present, applies to every step):

```aeris
saga deploy_release(...) {
  intent "ship release {version} to production"
  step build { ... }
  step apply { ... }
}
```

Agent-level (applies to every prompt invocation):

```aeris
agent classify {
  intent: "Triage the incoming invoice into one of four categories"
  ...
}
```

There is no function-level decoration form; a function whose body is
entirely effectful uses an outer `intent { ... }` block:

```aeris
fn rotate_cert(new_cert: bytes, cap: cap[fs.write_file, audit.event]) -> result<unit> {
  intent "rotate the leaked TLS cert" {
    fs.write_file(cert_path(), new_cert)?
    audit.event("cert.rotated", { path: cert_path() })
  }
}
```

### 10.3 What `intent` does at runtime

The runtime emits, at the entry of the block/saga/agent:

```json
{ "ts": "...", "trace_id": "...", "kind": "intent_enter",
  "intent": "rotate the leaked TLS cert", "scope": "rotate_cert" }
```

Every trace event emitted **inside** the body inherits the active
intent string in an `"intent"` field. At exit:

```json
{ "ts": "...", "trace_id": "...", "kind": "intent_exit",
  "outcome": "ok|err|partial" }
```

### 10.4 What `intent` does **not** do

It does not verify that the body matches the string. The thesis is
explicit (§ 8.4): the construct makes the *omission* impossible, not
the dishonesty. A reviewer (human or LLM) seeing `intent "save user
preferences"` followed by `http.post("https://evil.com", ...)`
catches the lie because the string is present, not because it is
correct.

---

## 11. Sequencing — without a dedicated keyword

Aeris v0.2 has **no `pipeline` construct**. Sequencing is expressed
two ways:

- **Read-only sequencing** is a chain of `let` bindings inside a
  regular function. The trace records the outer function's
  `intent_enter` / `intent_exit` and the leaf capability calls. No
  further structure is needed.

  ```aeris
  fn ingest_users(source: string, cap: cap[fs.read_file @ ["./data/**"]]) -> result<list<User@v1>>
  {
    intent "ingest {source} users for offline analysis" {
      let bytes      = fs.read_file(source)?
      let parsed     = json.decode<list<User@v1>>(bytes)?
      let normalised = parsed.map(canonicalise)
      Ok(normalised)
    }
  }
  ```

- **Write-effectful sequencing** is a `saga` (§ 12). The thesis
  commitment § 8.2 makes paired `do`/`undo` mandatory on every step
  that touches a write capability — a "pipeline that writes without
  undo" is exactly what the language refuses by construction. Adding
  a separate `pipeline` keyword would either duplicate `saga` (with
  undo) or contradict the thesis (without undo); either way it would
  carry no expressive weight. So it is not in the language.

If a future revision introduces a stage-tracing sugar over read-only
sequences, it will reuse this section number; until then, plain
functions and `saga` cover the design space.

---

## 12. Sagas — do/undo with idempotency

### 12.1 Form

```aeris
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
    do   { for it in batch { http.post("https://api.acme.com/charge", it)? } }
    undo { for it in batch { http.post("https://api.acme.com/refund", it)? } }
  }

  step ledger {
    requires: charge.ok
    do   { kube.apply(ledger_manifest(batch))? }
    undo { kube.delete(ledger_manifest(batch))? }
  }

  step notify {
    requires: ledger.ok
    do   { audit.event("settle.complete",    { count: len(batch) }) }
    undo { audit.event("settle.rolled_back", { count: len(batch) }) }
  }
}
```

### 12.2 Compiler rules

- Each `step` MUST have `do` and `undo` (no implicit `noop`).
- `undo: noop` is **forbidden** when the step's `do` reaches a write
  capability (§ 8.1). The parser refuses it. (Thesis § 8.2.)
- `step.<name>.ok` is a boolean visible to subsequent steps' `requires:`.
  A failed step short-circuits to the rollback phase.
- A saga has at most one `intent` declaration and it is mandatory.

### 12.3 Idempotency keys (N1)

The runtime auto-derives an idempotency key for each step invocation:

```
key = blake3(trace_id || step_name || invocation_index)
```

The key is injected into write capabilities as follows:

- `http.{post,put,patch,delete}` add header `Idempotency-Key: <hex>`.
- `kube.apply` adds `metadata.annotations["aeris.idempotency"] = <hex>`.
- `mongodb.write` adds the key as a field of the document or as a
  unique index sentinel.
- `rabbitmq.publish` sets the AMQP `message-id` to the key.
- `audit.event` sets `idempotency_key` in the audit record.

Backends that already see the same key produce the same write — replays
of a partially-completed saga become safe.

### 12.4 Failure semantics

- A step failure triggers reverse-order `undo` of all completed steps.
- An `undo` that fails is retried with idempotency keys; after
  configurable retry exhaustion, the saga emits a `PartialFailure`
  event and exits with code `74`.
- A `PartialFailure` is **never** suppressed. It is the only honest
  story consistent with distributed-systems physics (thesis § 11.4).

### 12.5 Trace shape

```json
{ "kind": "saga_enter",  "saga": "settle", "intent": "...", "trace_id": "..." }
{ "kind": "step_enter",  "step": "charge", "idempotency": "..." }
{ "kind": "step_exit",   "step": "charge", "outcome": "ok" }
{ "kind": "step_enter",  "step": "ledger" }
{ "kind": "step_exit",   "step": "ledger", "outcome": "err", "err": { ... } }
{ "kind": "undo_enter",  "step": "charge" }
{ "kind": "undo_exit",   "step": "charge", "outcome": "ok" }
{ "kind": "saga_exit",   "saga": "settle", "outcome": "rolled_back" }
```

---

## 13. Agents — single LLM units

### 13.1 Form

```aeris
agent classify {
  llm:     "claude-opus-4-7"
  intent:  "Classify the invoice into one of four categories"
  prompt:  """
    You will receive an invoice JSON. Output one of:
    { "category": "utilities" | "software" | "travel" | "other",
      "confidence": 0..1 }
  """
  accept:  Invoice@v1
  produce: Category@v1
  policy:  pii_redact, model_budget
  retries: 3
  budget:  { tokens: 4_000, latency: 5s }
}
```

### 13.2 Properties

- `llm:` MUST be a literal string. It pins the LLM. Different `llm:`
  values are different agents from the runtime's view. (The field is
  named `llm` rather than `model` to avoid ambiguity with the
  schema-declaration keyword `model`.)
- `prompt:` is a triple-quoted string. The runtime auto-injects an
  appendix declaring the routing-protocol JSON contract (§ 14) and
  the `accept`/`produce` schemas. The user does not write the routing
  contract by hand — it is owned by the runtime (thesis § 4 / L4).
- `accept:` and `produce:` reference `model` types. The runtime
  validates inputs and outputs against the schema; an out-of-shape
  response is rejected with `SchemaViolation` and counted as a retry.
- `policy:` references one or more `policy` declarations (§ 15).
- `budget:` constrains the per-call resource envelope; exceeding it
  raises `BudgetExceeded`. Each retry has its own budget.
- Every invocation is recorded into the trace as an `ai_call` event
  with `(prompt, model, response, tokens, ts)` (N3, thesis § 8.5).

### 13.3 Calling an agent

```aeris
intent "triage today's invoices" {
  let inv: Invoice@v1 = ...
  let cat: Category@v1 = classify(inv, cap.subset[ai.complete @ ["claude-opus-4-7"]])?
}
```

Agents are called like functions. They require a `cap` subset
containing `ai.*`. The compiler verifies the caller's `cap` covers
the agent's needs, including the model allow-list.

---

## 14. `agent_net` — typed dataflow graph

### 14.1 Form

```aeris
agent_net invoice_pipeline {
  intent "extract → classify → route incoming invoices"

  flow extract -> classify -> route_or_alert
  flow route_or_alert -> { route, alert }
  flow route -> persist
  flow alert -> notify_human

  until: classify.confidence > 0.95 || iterations >= 3
}
```

- `flow` declares directed edges. Multiple `flow` lines are unioned.
- A node name refers either to an `agent` declaration or to another
  `agent_net` declaration (composition).
- A branch like `-> { a, b }` denotes parallel fan-out; consumers run
  concurrently. Routing among branches is *type-driven*: a branch
  whose head agent's `accept` does not match the produced shape is
  not entered.
- **Cycles are rejected at parse time.** Iteration is expressed via
  `until:` (thesis § 4 / L4); the runtime re-runs the entire DAG until
  the predicate holds or `iterations` reaches its bound.
- The runtime owns the routing protocol: every agent receives, in its
  system prompt, a JSON-fenced contract describing its inbox and
  expected output shape. The user does not write it.
- Schema validation runs at every edge crossing.

### 14.2 Composition

An `agent_net` is itself a node and may be referenced from another
`agent_net`. Composition is a substitution at the use site; recursion
is forbidden.

### 14.3 Failure semantics

- An agent failure consumes its `retries:` budget; after exhaustion the
  agent emits an `err.llm` (§ 18.1).
- An agent's `err` propagates to its consumers; a consumer agent that
  would receive an `err` instead receives nothing for that iteration.
- If at the end of an iteration no terminal node has produced a value
  *and* `until:` is not satisfied, the net retries up to `iterations`.
- If `iterations` is reached with no terminal output, the net returns
  `Err(err.user("agent_net exhausted"))`.

### 14.4 Trace shape

```json
{ "kind": "net_enter", "net": "invoice_pipeline", "iter": 0 }
{ "kind": "edge",      "from": "extract",  "to": "classify",        "schema": "Extracted@v1" }
{ "kind": "agent_call","agent": "classify","model": "claude-opus-4-7","tokens": 412 }
{ "kind": "edge",      "from": "classify", "to": "route_or_alert",  "schema": "Category@v1" }
{ "kind": "net_exit",  "net": "invoice_pipeline", "outcome": "ok",  "iters": 1 }
```

---

## 15. Policies — runtime guardrails

### 15.1 Form

```aeris
policy production_egress {
  match: http.*
  deny:  url.host not in ["api.acme.com", "api.stripe.com"]
  audit: { url, method }
}

policy model_budget {
  match: ai.*
  limit: tokens_per_minute = 60_000
  limit: usd_per_day = 50
}

policy pii_redact {
  match:   ai.*
  require: not contains_pii(prompt)
  deny:    contains_email(response) or contains_ssn(response)
  audit:   { redactions, pii_kinds }
}

policy production_writes {
  match:   kube.apply or kube.delete
  require: cluster.name in ["prod-eu-1", "prod-us-2"]
  require: enclosing_intent != ""
  audit:   { manifest_kind, name, namespace }
}
```

### 15.2 Clauses

| Clause | Meaning |
|---|---|
| `match:` | the capability path(s) this policy applies to |
| `deny:`  | a violation if true; raises `PolicyViolation` |
| `require:` | a violation if false; raises `PolicyViolation` |
| `limit:` | quota over a window (per minute / hour / day) |
| `audit:` | extra fields included in the trace event for matching calls |
| `when:`  | optional gate on environment (`when: env == "production"`) |

`match:` patterns reference capability paths (`http.*`, `ai.complete`,
`kube.apply`) — the same paths used in signatures and bodies. There
is no separate "policy DSL"; the path syntax is uniform across the
language.

### 15.3 Activation

- A policy declared in a module's source is active when the module is
  imported.
- A policy may be **scoped** to a function via attribute syntax:
  `#[policy(production_writes)] fn deploy(...) { ... }`
- A policy may be activated globally for a project from
  `aeris.toml [policies]`.

### 15.4 Determinism under replay

Policy evaluations are recorded into the trace. Replay re-evaluates
policies against the recorded request shape; an outcome divergence
between live and replay is itself a trace event (`policy_drift`).

---

## 16. Models in depth

### 16.1 Versioning

A model **must** carry an `@vN` tag. Bare `Invoice` is a parse error.
This forces deliberate evolution at trust boundaries.

### 16.2 Validation points

A `model@vN` value is validated:

1. on construction (`Invoice@v1 { ... }`),
2. on JSON decoding (`json.decode<Invoice@v1>(s)`),
3. on agent boundary crossing (`accept`/`produce`),
4. on queue/HTTP ingress when annotated `model` (`http.body<Invoice@v1>(req)`).

Validation failures raise `SchemaViolation { model, version, errors: list<error> }`.

### 16.3 Field clauses

```aeris
model Order@v2 {
  id: uuid
  total: decimal where total > 0
  ts: timestamp
  status: Status

  // record-level invariants:
  where: status == Cancelled implies total == 0
  where: ts <= now()
}
```

Record-level `where:` clauses run after all field validations.

### 16.4 Migration

```aeris
fn migrate_order_v1_to_v2(old: Order@v1) -> Order@v2 { ... }
```

Migrations are **explicit pure functions** (no `cap` parameter,
therefore pure by structure). There is no implicit upgrade. A consumer
that needs `@v2` from a producer emitting `@v1` calls the migration
explicitly; the runtime records the migration call in the trace.

### 16.5 Inheritance — `extends` (v0.3, M23)

A `@v2` schema may extend an earlier version of the same model:

```aeris
model User@v1 {
  id:    uuid
  name:  string  where len(name) > 0
}

model User@v2 extends User@v1 {
  email: string  where contains(email, "@")
}
```

The runtime merges the parent's fields and `where:` clauses into the
child at parse time. The child may add new fields and new clauses;
it may **not** remove or rename parent fields. Adding a `where:`
clause on a parent field is allowed and is appended to the parent's
predicate — both must hold.

`extends` is **not** an implicit migration: `User@v1` and `User@v2`
remain distinct types (§ 4.5). A consumer that needs `@v2` from a
producer emitting `@v1` still calls a migration function explicitly.
`extends` is the way to *describe* the v2 shape without retyping
the v1 fields.

---

## 17. Pattern matching

### 17.1 Patterns

```aeris
match v {
  0           -> "zero",
  n           -> if n > 0 { "positive" } else { "negative" },
}

match s {
  Pending                        -> ...,
  Active(t) if t < cutoff        -> ...,
  Active(_)                      -> ...,
  Banned { reason: "spam", .. }  -> ...,
  Banned { .. }                  -> ...,
}

match r {
  Ok(v)                          -> v,
  Err(net_timeout(after))        -> retry(after),
  Err(e)                         -> raise e,
}

match xs {
  []                             -> ...,
  [x]                            -> ...,
  [x, ..rest]                    -> ...,
  [first, .., last]              -> ...,
}
```

### 17.2 Exhaustiveness

`match` is exhaustive. The compiler computes the unmatched residue
**structurally** (no SMT, no constraint solving) and emits the missing
patterns as the error message. There is no implicit `default`; the
developer writes `_` if that is genuinely intended.

**Guards do not contribute to exhaustiveness.** A guarded arm
(`pat if cond -> ...`) covers a runtime-decided subset of `pat`'s
matches; the structural checker treats a guarded arm as if it might
miss. Therefore a `match` on a non-finite domain (e.g. `int`) whose
arms are all guarded MUST include a guard-free catch-all (`_` or a
plain binder `n`). This rule is the price of *decidable* exhaustiveness
checking — and the cost is paid in two extra characters per match.

### 17.3 `is` and `as`

```aeris
if r is Ok(v) { use(v) }                  // refinement check
let v = r as Ok                            // refinement coercion; raises on mismatch
```

Both forms are sugar over `match`.

---

## 18. Errors and `result`

### 18.1 The single error type

```aeris
enum err {
  io                { kind: io_kind, path: string }
  net               { kind: net_kind, host: string, after: option<duration> }
  schema            { model: string, version: string, problems: list<string> }
  contract          { fn_name: string, clause: string }
  policy            { name: string, fields: map<string, string> }
  budget            { kind: budget_kind, used: u64, cap: u64 }
  partial_failure   { saga: string, completed: list<string>, failed: string }
  llm               { model: string, code: int, message: string }
  user(string)
}
```

Aeris has **a single, closed error type — `err`**. Functions that may
fail return `result<T>`; there is no user-parameterisable `E`, no
exception class hierarchy, no orphan error types. Modules MAY produce
custom error shapes by populating the `user(string)` variant with a
structured payload (e.g. JSON-encoded), but the wire-level type is
always `err`. This makes "what shape can an error take?" a question
answerable from the language reference, not from a project's
sub-vocabulary.

### 18.2 The `?` operator

```aeris
let bytes = fs.read_file(p)?
```

If the expression is `Err(e)`, the surrounding function returns
`Err(e)`. If the function does not return `result<T>`, the use of
`?` is a parse error.

### 18.3 `raise`

```aeris
raise err.user("amount must be positive")
```

`raise e` is equivalent to `return Err(e)` in a function returning
`result<T>`. In a function without a `cap` parameter (i.e. a pure
function — § 7.2), `raise` is a parse error: pure functions are total.

### 18.4 What `?` does **not** catch

Contract violations and policy violations are **fatal**. They are not
`Err(...)` values; they cannot be propagated by `?`. They terminate
the program after the trace flush. This is by design — see thesis
§ 8.2 (mandatory undo) and § 8.4 (mandatory intent): hidden recovery
from a structural violation defeats the purpose.

### 18.5 Inline recovery — `catch`, `error()`, `defer` (v0.3, M17)

Three constructs for local error handling without leaving the
`result<T>` discipline:

```aeris
let bytes = fs.read_file(p) catch err {
  io.eprintln("read failed: {err.message}")
  b""                                     // fallback value
}

if amount < 0 { return Err(error("amount must be positive")) }

fn render(items, cap: cap[fs.write_file @ ["./out/**"]]) -> result<unit> {
  let tmp = fs.create_temp()?
  defer fs.remove(tmp)                    // runs LIFO on every exit
  intent "render report" {
    fs.write_file("./out/report.html", build(items, tmp))?
    Ok(())
  }
}
```

- `expr catch err { recovery }` — if `expr` is `Ok(v)` (or any
  non-error value) the value flows through. If it is `Err(e)`, `e`
  is bound to `err` inside the recovery block and the block's value
  becomes the expression's value. The recovery body itself may
  return an `Err`, propagate with `?`, or supply a fallback.
- `error(msg)` — constructor for `err.user(msg)`. Used inside
  `Err(error("..."))` or `raise error("...")`. It is *only* a
  shorthand; the wire-level error type is still `err`.
- `defer stmt` — schedules `stmt` to run when the enclosing function
  returns, in LIFO order. Deferred statements run on **every** exit
  path (normal return, `?` propagation, `raise`, contract violation
  shutdown). They cannot see the function's return value and cannot
  themselves return; they exist to release resources.

`catch` does **not** catch contract or policy violations (§ 18.4) —
those still terminate the program. `defer` *does* run before a
contract-violation shutdown, so it is safe to put resource cleanup
there.

---

## 19. Concurrency

### 19.1 `spawn` and `await`

```aeris
let h: handle<int> = spawn { compute(cap.subset[ai.complete @ ["claude-opus-4-7"]]) }
let r: int = await h
```

- `spawn { }` runs the block on an OS thread (project.md). The block
  receives a *copy* of any captured `let` binding and a *cloned,
  narrowed* capability subset constructed at the spawn site; sharing
  `cap` directly across threads is forbidden.
- A `spawn` block returns a `handle<T>` whose `await` yields the
  block's value or propagates an error.
- A panic inside `spawn` becomes an `Err(err.user(...))` on `await`.

Concurrent fan-out is expressed by spawning multiple handles and
awaiting them:

```aeris
let h_a = spawn { fetch_a(cap.subset[http.get @ ["api.x.com"]]) }
let h_b = spawn { fetch_b(cap.subset[http.get @ ["api.y.com"]]) }
let h_c = spawn { fetch_c(cap.subset[http.get @ ["api.z.com"]]) }
let (a, b, c) = (await h_a, await h_b, await h_c)
```

There is no `parallel { e1, e2, e3 }` keyword; the spawn-and-await
form expresses the same intent without a dedicated construct.

### 19.2 Channels

```aeris
let ch: channel<int> = channel(capacity: 16)

spawn {
  for x in 1..100 { ch.send(x)? }
  ch.close()
}

for x in ch { io.println("{x}") }    // iterates until close
```

- Channels are bounded MPMC. `send` on a full channel blocks; `recv`
  on empty blocks; both yield `Err(err.io)` on closed-with-pending.
- `channel<T>` requires `T` to be `Send`-compatible: not `cap`, not a
  closure capturing `cap`, not a `handle`.

### 19.3 Cancellation

```aeris
let h = spawn { long_running(cap) }
h.cancel()                  // cooperative; injects err.user("cancelled") at next await
```

Cancellation is **cooperative**. Aeris does not interrupt OS-blocking
calls. Cooperative cancel-points are: `await`, `?`, capability calls,
`for x in channel`.

---

## 20. Tracing and replay

### 20.1 Trace channel

Every Aeris run emits a JSONL stream into `.aeris/traces/<trace_id>.jsonl`.
Each line is a self-contained event. Schema:

```json
{
  "ts": "2026-05-07T08:30:00.123Z",
  "trace_id": "01JFE...",
  "kind": "http_call|fs_read|ai_call|saga_enter|step_enter|...",
  "intent": "<active intent string, if any>",
  "scope": "<saga.step | function | net.agent>",
  "fields": { ... event-specific ... }
}
```

The trace is also propagated across HTTP calls via the
`X-Aeris-Trace-Id: <trace_id>` header (project.md).

### 20.2 What is recorded (N3, N2)

| Source | Recorded fields |
|---|---|
| `ai.*`         | `prompt`, `model`, `response`, `tokens`, `latency` |
| `clock.now`    | `value` |
| `random.next`  | `value` |
| `http.*`       | `url`, `method`, `status`, `req_hash`, `resp_hash` |
| `fs.read_*`    | `path`, `len`, `hash` |
| `fs.write_*`   | `path`, `len`, `hash` |
| `shell.exec`   | `argv`, `env_pruned`, `exit`, `stdout_hash`, `stderr_hash` |
| ... | ... |

The recording is **always-on**. There is no opt-out switch in
production builds. HTTP request/response *bodies* are stored as hash
by default; `aeris run --full-record` opts into byte-level capture
for debugging at the cost of trace size and potential secret exposure.

### 20.3 Replay

```
aeris replay <trace_id>
```

- Re-runs the program against the recorded tape.
- `ai.*` returns the recorded response without contacting any LLM.
- `clock.now` and `random.next` emit the recorded values.
- `http.*` is rerun against a recorded fixture (read-only) when
  `--from-fixtures` (default); against the live network when `--live`.
- The replay is **bit-identical** for the deterministic subset of the
  program; the non-deterministic subset is fixed by the recording
  (§ 8.5 of thesis).

### 20.4 Diffing

```
aeris trace diff <trace_a> <trace_b>
```

Aligns events by `(scope, ordinal)` and reports diverging fields. Used
for regression bisects.

---

## 21. Tests and properties

### 21.1 Unit tests

```aeris
test "addition is commutative" {
  assert add(2, 3) == add(3, 2)
}
```

Tests are top-level declarations. They receive a *test capability*
containing only `cap.fixtures` (read-only access to `tests/fixtures/**`).

### 21.2 File-as-suite convention

There is no `suite { ... }` keyword. The grouping unit is the **file**:
every `test` declared in `tests/foo.test.aer` is run as a single suite
named `foo`. `aeris test foo` runs that file; `aeris test` runs the
whole `tests/**` tree. This makes one less concept to remember and
turns "where is the test for X?" into a filesystem question.

### 21.3 Property tests

```aeris
property "concat is associative" with (a: list<int>, b: list<int>, c: list<int>) {
  assert (a ++ b) ++ c == a ++ (b ++ c)
}
```

`with (...)` declares generators. The runtime samples `n=200` cases
(configurable). Counter-examples are recorded as `tests/fixtures/<id>.json`
and re-run on subsequent invocations (regression seed).

### 21.4 Specialised assertions (v0.3, M21)

Three helpers for the common LLM / HTTP test patterns:

```aeris
test "GET /health" {
  let resp = http_client_for_tests().get("http://localhost/health")
  assert_status(resp, 200)
}

test "agent emits well-formed JSON" {
  let raw = chat.ask("describe in JSON form")
  assert_json(raw, ["kind", "confidence"])    // required keys
}

test "summary is faithful" {
  let summary = summarise(doc)
  assert_semantic(
    actual:   summary,
    criteria: "faithful and complete to the original",
    judge:    "claude-haiku-4-5",
  )
}
```

- `assert_status(resp, code)` — pass iff the response's `status`
  field equals `code`; failure includes the actual status.
- `assert_json(text, required_keys)` — pass iff `text` parses as a
  JSON object containing every key in `required_keys`.
- `assert_semantic(actual, criteria, judge)` — uses the configured
  AI backend as a judge. Pass iff the judge replies that `actual`
  meets `criteria`. The judge call is recorded into the trace and
  re-played offline like every other `ai.*` call.

### 21.5 Fixture mode

```aeris
test "settle rolls back on ledger failure" with fixture: "settle.broken_ledger" {
  let cap = cap.test_subset[http.post @ ["api.acme.com"], audit.event]
  let r = settle(load_invoices(), cap)
  assert r.is_err()
  assert trace().has({ kind: "saga_exit", outcome: "rolled_back" })
}
```

Fixtures are recorded traces from a prior run. The test capability
replays them by default; the developer chooses `--live` to re-record.

---

## 22. Standard library — Layer 1

The L1 stdlib is bundled with `aeris-core`. The full list:

| Module | Public surface (abridged) |
|---|---|
| `io`      | `print`, `println`, `eprint`, `eprintln`, `read_line` |
| `fs`      | `read_file`, `read_text`, `read_bytes`, `write_file`, `write_text`, `write_bytes`, `walk`, `stat`, `exists`, `mkdir`, `remove`, `rename` |
| `http`    | `get`, `post`, `put`, `patch`, `delete`, `req`, `resp`, `header`, `query`, `body<T>` |
| `shell`   | `exec`, `pipe`, `args`, `quote` |
| `env`     | `read(key)`, `must_read(key)`, `set(key, value)` (v0.3) |
| `strings` | `trim`, `lower`, `upper`, `contains`, `starts_with`, `ends_with`, `split`, `join`, `replace`, `parse_int` (v0.3) |
| `date`    | `today() -> date`, `timestamp() -> int`, `now() -> timestamp`, `format(t, fmt)` (v0.3 — `%Y %m %d %H %M %S`) |
| `json`    | `decode<T>(s)`, `encode(v)`, `parse(s)`, `stringify(v)` ≡ `encode`, `pretty(v)` (v0.3 — natural JSON) |
| `yaml`    | `parse(s)`, `parse_file(path)` (v0.3 — v0.1-compatible subset) |
| `clock`   | `now`, `sleep(D)` (M18 — read-classified; recorded for replay) |
| `random`  | `next` |
| `net`     | `http(port: int) -> HttpServer` (v0.3, M20) |

**`net.http(port: int) -> HttpServer` (M20).** Bind a TCP listener
on the given port and return a server value. The server is
single-threaded blocking; concurrent request handling is the
caller's responsibility via `spawn { … }`:

```aeris
let server = net.http(port: 8080)
loop {
  let req = server.accept()
  spawn {
    if req.path == "/api/health" {
      req.reply_json(200, json.encode({ status: "ok" }))
    } else if req.path == "/" {
      req.reply(200, fs.read_text("./index.html"), "text/html; charset=utf-8")
    } else {
      req.reply(404, "not found")
    }
  }
}
```

The accepted request value is a record with keys `method`, `path`,
`query_raw`, `headers` (record), `body` (string), `remote_addr`.
TCP / UDP listeners and `net.resolve` are deferred to v0.4.

`json.encode` / `json.stringify` emit **natural JSON** (the v0.3
surface used everywhere a human or HTTP body needs to see it):
records become objects, lists become arrays, `Ok(v)`/`Some(v)`
unwrap to `v`, `Err(_)`/`None`/`()` become `null`. The internal
self-tagging encoder (used by the trace JSONL) is not exposed to
user code.

`json.parse` returns `result<record>` for natural JSON objects —
parse failure surfaces as `Err(string)` so callers can write
`json.parse(raw) ?? \{\}` for a default.

**Methods on built-in values (v0.3, no `use` needed):**

| Receiver | Methods |
|---|---|
| `list<T>` | `.len()`, `.empty()`, `.first()`, `.last()`, `.slice(a, b)`, `.contains(x)`, `.join(sep)`, `.map(f)` (M30). **Mutating (only on `var` bindings)**: `.push(x) -> int`, `.pop() -> option<T>` |
| `string` | `.len()`, `.trim()`, `.lower()`, `.upper()`, `.contains(p)`, `.starts_with(p)`, `.ends_with(p)`, `.split(sep)`, `.replace(from, to)`, `.index_of(needle, from?) -> option<int>` (M30) |
| `map<K, V>` | `.len()`, `.get(k) -> option<V>` |
| `record Chat` | `.ask(prompt) -> string`, `.kb_size() -> int` (see § 23) |
| `record HttpServer` | `.accept() -> HttpReq` (M20) |
| `record HttpReq` | `.reply(status, body, content_type?) -> unit`, `.reply_json(status, body) -> unit` (M20) |
| `record AiNetwork` | `.agent(name, system) -> unit` (mutating), `.run(entry, message, until?) -> { trace, rounds }` (M28) |

The global `len(x)` function accepts `list`, `set`, `tuple`, `map`,
`string`, and `bytes`. The display form (`io.println`, `+` string
concat) renders `Ok(v)`/`Some(v)` as `v`, records and lists as
natural JSON.

Every L1 module that can have side effects names a capability path
(e.g. `fs.write_file`, `http.post`). Pure helpers (`strings.trim`,
`json.decode`, `date.today`, the value methods above) take no `cap`
and are called as plain functions. The L1 surface is closed; there
are no "plugins" extending it (thesis § 9.6). Diagnostic helpers
(`io.print*`) bypass the V2 mandatory-intent rule (§ 8.1,
diagnostic class).

---

## 23. Native cap handlers — Layer 2

L2 modules are native cap handlers compiled into `aeris-core`. They
**are not** dynamically-loaded `.so` files (thesis § 9.6). Adding an
L2 module requires a release of `aeris-core`.

| Module      | Capability paths it implements |
|-------------|--------------------------------|
| `ai`        | `ai.complete`, `ai.chat`, `ai.embed`, `ai.tools` |
| `kube`      | `kube.apply`, `kube.delete`, `kube.get`, `kube.watch` |
| `docker`    | `docker.run`, `docker.build`, `docker.push`, `docker.pull`, `docker.inspect` |
| `mongodb`   | `mongodb.read`, `mongodb.write` |
| `minio`     | `minio.get`, `minio.put` |
| `rabbitmq`  | `rabbitmq.publish`, `rabbitmq.subscribe` |
| `audit`     | `audit.event` |

L2 modules expose pure helpers (manifest builders, query DSLs) that
take no `cap`. Effectful entry points always require the appropriate
capability path in the enclosing function's `cap` parameter.

**`ai.network(max_rounds: int) -> AiNetwork` (v0.3, M28).**
Programmatic multi-agent builder, parity with v0.1. Methods:

- `network.agent(name: string, system: string)` — register an
  agent in the network (mutates a `var` binding).
- `network.run(entry: string, message: string, until: string?) ->
  { trace: list<record>, rounds: int }` — drive the network from
  `entry` with `message`; iterate at most `max_rounds`; stop when
  any reply contains the `until` sentinel (default `"DONE"`).
  Hand-off is text-based: a reply prefixed `>>NAME:` forwards the
  rest to the named agent, otherwise round-robin advances.

```aeris
fn main() {
  var net = ai.network(max_rounds: 10)
  net.agent(name: "geologist",     system: fs.read_text("./agents/geo.md"))
  net.agent(name: "risk_assessor", system: fs.read_text("./agents/risk.md"))
  net.agent(name: "reporter",      system: fs.read_text("./agents/rep.md"))

  let r = net.run(
    entry:   "geologist",
    message: "Analyse today's M4.5+ events.",
    until:   "DONE",
  )
  io.println("{r.rounds} rounds")
}
```

`ai.network` is a *lower-overhead* sibling of `agent_net`. The
declarative `agent_net` validates each edge against `accept` /
`produce` `model@vN` schemas (§ 14); the programmatic builder
uses free-form text and text-based routing. Use `agent_net` when
the schemas are stable; use `ai.network` when the agent set is
discovered at runtime from a directory.

**`ai.session(system, model) -> Session` and `ai.session_ask(session,
prompt) -> (Session, string)` (v0.3, M19).** Multi-turn conversation
with auto-compaction. The session holds the rolling message history;
each `session_ask` appends the user message, calls `ai.complete`, and
appends the reply. When the history grows beyond 40 entries the
runtime trims it to the last 20 in-place, summarising older context
into a single system message. Each call is recorded into the trace as
an `ai_call` event and re-played offline.

```aeris
let s = ai.session(
  system: "You are a concise assistant.",
  model:  "claude-haiku-4-5",
)
let (s2, a) = ai.session_ask(s, "hello")
let (s3, b) = ai.session_ask(s2, "summarise the previous answer")
io.println(b)
```

**`ai.decide(prompt, choices, retries?) -> string` (v0.3, M19).**
Enum-style decision builtin. Calls the model with `prompt`, then
post-validates that the reply contains one of `choices`. On
mismatch it retries up to `retries` times (default `3`); after
exhaustion it returns `Err(err.llm(...))`. Use this in place of an
agent + `produce` enum when the agent set is trivial.

```aeris
let kind = ai.decide(
  prompt:  "Classify the invoice: utilities, software, travel, other.",
  choices: ["utilities", "software", "travel", "other"],
)?
```

**`ai.usage() -> { total_tokens: int, cost_usd: f64, calls: int }`
(v0.3, M19).** Returns the per-process running totals of LLM usage
since program start. Token counts come from the backend; cost is
computed from a static price table indexed by `model`. Always
available; it is a read-classified diagnostic on the in-memory
counter, not a network call.

**`ai.chat(system, dir) -> Chat` (v0.3, M19.T6).** Convenience
constructor that loads a directory of markdown / text files
(`*.md`, `*.txt`, `*.rst`, `*.adoc`, `*.yaml`, `*.yml`) into the
system prompt as a labelled knowledge base, then returns a `Chat`
record with two methods:

- `chat.ask(prompt) -> string` — send the prompt; reply is the
  string returned by the backend.
- `chat.kb_size() -> int` — number of files loaded.

```aeris
let chat = ai.chat("You are concise.", "./docs")
io.println("loaded {chat.kb_size()} files")
io.println(chat.ask("explain capabilities"))
```

The two-argument form coexists with the v0.2 `ai.chat(messages)`
list-of-messages API: when `args = (string, string)` the directory
loader fires; otherwise the message-list path runs. Both go
through the same backend (`[ai.backend]`).

The `ai` module is **pluggable on configuration**, not on linkage:
the LLM backend (HTTP API endpoint, CLI process, mock) is selected
through `aeris.toml [ai.backend]` and resolved by `aeris-core` at
start-up; no third-party native code is loaded.

---

## 24. External libraries — Layer 3

### 24.1 The manifest

```toml
# aeris.toml
[project]
name   = "settle-pipeline"
aeris  = "0.3.0"

[deps]
deploy = { source = "github.com/acmecorp/aeris-devops", version = "1.2.0",
           hash   = "blake3:7e2c...c1a4" }
utils  = { path   = "./lib/utils.aer",
           hash   = "blake3:9b18...ff02" }

[caps]
enforce         = "strict"                            # § 8.4.1 — off | loose | strict
http.allow      = ["api.acme.com", "api.stripe.com"]
fs.allow_read   = ["/etc/aeris/**", "./data/**"]
fs.allow_write  = ["./out/**", "./.aeris/**"]
kube.contexts   = ["prod-eu-1"]
ai.models       = ["claude-opus-4-7", "claude-haiku-4-5"]

[ai.backend]
kind  = "http"
url   = "https://api.anthropic.com"
auth  = "env:ANTHROPIC_API_KEY"

[policies]
active = ["production_egress", "model_budget"]
```

The file is named `aeris.toml` — it carries project metadata,
content-addressed deps, the cap ceiling, the AI backend and the
active policies, not just the locks. The crate refers to its
in-memory shape as `Manifest`; "lockset" survives as a synonym for
the deps section in older prose.

For scripts, the `[caps]` block is reduced to a single line:

```toml
[caps]
enforce = "off"   # cap[*] synthesised, no runtime allow-list
```

`enforce` is the v0.3 form; `required = true | false` from v0.2 is
still accepted as an alias (`true` → strict, `false` → loose).

### 24.2 Resolution

- A `use ... from "github.com/.../v"` resolves to a tarball whose bytes
  are blake3-hashed.
- The hash MUST match `[deps].<alias>.hash`. Mismatch is fatal *before*
  any code from the dependency executes.
- Resolved bytes are cached at `.aeris/ext/<host>__<repo>/<version>/`
  and treated as immutable.
- There is no `latest`, no `*`, no mutable git tag form (thesis § 9.5).

### 24.3 Effect surface for dependencies

A dependency's `surface.lock` is **also** locked into the consumer's
`aeris.toml [deps].<alias>.surface_hash`. A dependency upgrade that
broadens the surface forces a lockfile diff visible in PR review (V3
+ supply-chain integrity).

### 24.4 Local development

```aeris
use "./lib/utils.aer"               // path source; hash auto-computed
```

For path-source dependencies, `aeris lock` recomputes the hash on
every change and updates `aeris.toml`. CI rejects a PR whose
`aeris.toml` is stale relative to the source tree.

---

## 25. Tooling — the `aeris` CLI

### 25.1 Single binary

```
aeris run <file.aer> [args...]            # compile-and-run
aeris test <file_or_glob>                 # run tests
aeris fmt [--narrow-caps] <file_or_glob>  # format; total
aeris check <file_or_glob>                # type & cap-graph check, no run
aeris doc <file_or_glob>                  # extract /// doc comments → JSONL
aeris lock [surface]                      # write aeris.toml & .aeris/surface.lock
aeris replay <trace_id> [--live]          # re-run from recorded trace
aeris trace tail [<trace_id>]             # follow a trace
aeris trace diff <a> <b>                  # diff two traces
aeris init                                # scaffold a project skeleton
aeris version
```

### 25.2 `aeris fmt` is total

`aeris fmt` defines *the* canonical layout. It rewrites every parseable
file to a unique form. There are no formatter knobs. PR diffs do not
contain formatting noise.

`aeris fmt --narrow-caps` is the V1 patch: rewrites function signatures
to the minimum capability set the body uses, including allow-list
narrowing. The intended workflow is **generation loose, fmt tight**
(§ 8.5): write a coarse signature like `cap[http, kube, audit.event]`,
let the formatter derive the narrow form.

### 25.3 `aeris check` exit codes

| Code | Meaning |
|---|---|
| 0  | no errors |
| 64 | parse / type error |
| 65 | capability error (missing / over-broad / cap[*] in user code) |
| 66 | intent missing on write-effectful call |
| 67 | saga step lacks paired undo |
| 68 | model version conflict |
| 69 | lockfile drift (hash mismatch) |
| 70 | cycle in `agent_net` |
| 71 | allow-list violation (signature outside lockset ceiling) |

### 25.4 `aeris run` invariants

- Recording is on; the trace path is printed on stderr at start-up,
  along with the effective `main` capability shape (§ 8.4).
- A non-zero exit code flushes the trace before exiting.
- Signals `SIGINT` and `SIGTERM` trigger cooperative cancellation
  (§ 19.3) and an `exit_signal` trace event.

---

## 26. Grammar sketch (informative EBNF)

This is **not** the implementation grammar; the canonical grammar lives
in `aeris-core/parser/grammar.lalrpop`. The sketch fixes the surface.

```
File          ::= Use* TopDecl*
Use           ::= 'use' UseClause (',' UseClause)*
UseClause     ::= IdList | (Id 'from')? StringLit (Id '@' StringLit)?

TopDecl       ::= ('pub')? (FnDecl | RecordDecl | EnumDecl | ModelDecl
                 | TypeAlias | ConstDecl
                 | SagaDecl | AgentDecl | AgentNetDecl
                 | PolicyDecl | TestDecl | PropertyDecl)

FnDecl        ::= 'fn' Id Generics? '(' Params? ')' RetTy? Contracts? Block
Params        ::= Param (',' Param)*
Param         ::= Id ':' Ty                          // 'cap' is a reserved Param Id
RetTy         ::= '->' Ty
Contracts     ::= ('requires' ':' Expr)*  ('ensures' ':' Expr)*

CapTy         ::= 'cap' '[' CapEntry (',' CapEntry)* ']'
CapEntry      ::= CapPath ('@' AllowList)?
CapPath       ::= Id ('.' Id)?                       // 1 or 2 levels; never '*'
AllowList     ::= StringLit | '[' StringLit (',' StringLit)* ']'

CapCall       ::= CapPath '(' Args? ')'              // body-level; resolved against in-scope `cap`
CapNarrow    ::= 'cap' '.' ('subset' | 'test_subset') '[' CapEntry (',' CapEntry)* ']'

ModelDecl     ::= 'model' Id '@v' IntLit '{' ModelField* WhereBlock? '}'
ModelField    ::= Id ':' Ty ('where' Expr)?
WhereBlock    ::= ('where' ':' Expr)+

SagaDecl      ::= 'saga' Id '(' Params? ')' '{' IntentInline Step+ '}'
IntentInline  ::= 'intent' StringLit
Step          ::= 'step' Id '{' Contracts? 'do' Block 'undo' (Block | 'noop') '}'

AgentDecl     ::= 'agent' Id '{' AgentField+ '}'
AgentField    ::= Id ':' Expr                        // structural; field names enumerated by parser

AgentNetDecl  ::= 'agent_net' Id '{' IntentInline? Flow+ Until? '}'
Flow          ::= 'flow' FlowExpr
FlowExpr      ::= Id ('->' (Id | '{' Id (',' Id)* '}'))+
Until         ::= 'until' ':' Expr

PolicyDecl    ::= 'policy' Id '{' PolicyField+ '}'
PolicyField   ::= Id ':' Expr                        // structural; field names enumerated by parser

IntentBlock   ::= 'intent' StringLit Block

Stmt          ::= LetStmt | VarStmt | ExprStmt | ReturnStmt | RaiseStmt | LoopStmt | ...
Expr          ::= Literal | Path | Call | CapCall | CapNarrow | Binary | Unary | Match | If | Block | Spawn | Lambda | Try
Try           ::= Expr '?'
```

The complete grammar is appended in `aeris-core/parser/grammar.lalrpop`
once the parser is implemented. The sketch above is sufficient to
write any program in this document.

---

## 27. Glossary

- **agent** — a single LLM unit with a pinned `llm:` model, prompt
  template, accept/produce schemas, and bound policies.
- **agent_net** — an acyclic typed dataflow graph of agents.
- **allow-list** — the `@` clause in a capability type that names the
  concrete endpoints (hosts, paths, contexts, models) reachable through
  that operation.
- **cap** — the capability parameter; the *only* way to reach external
  effects. Body calls of the form `<module>.<op>(...)` resolve against
  the in-scope `cap` parameter (§ 8.2).
- **content-addressed** — every external library byte sequence is named
  by its blake3 hash; mismatch is fatal.
- **diagnostic** — capability operations (`io.print*`) that write to
  process-local streams; they do not trigger the V2 mandatory-intent
  rule.
- **effect surface** — the closed set of capability paths and
  allow-lists a public function reaches transitively.
- **intent** — a scope-level "why" string, mandatory on write-effectful
  calls; emitted into the trace.
- **lockset** — `aeris.toml` + `surface.lock`; the project's pinned
  view of the world. Also the source of `main`'s synthesised cap shape.
- **model@vN** — a versioned record schema validated at trust
  boundaries.
- **policy** — a runtime guardrail on capability calls (deny / require /
  limit / audit).
- **replay** — `aeris replay <trace_id>` re-runs a program against the
  recorded tape; deterministic in the deterministic subset.
- **saga** — a long-running multi-step external operation with paired
  do/undo and idempotency keys.
- **trace** — the JSONL event stream Aeris emits for every run.
- **why-as-grammar** — the principle that the *why* of code is a
  grammatical ancestor of every effect, not a comment or commit message.

---

## Appendix A — minimum viable program

```aeris
use io

fn main(cap) -> result<unit> {
  io.println("hello, aeris")
  Ok(())
}
```

## Appendix B — minimum viable saga

```aeris
use http, audit

saga rotate(cap: cap[http.post @ ["vault.acme.com"], audit.event]) {
  intent "rotate the production webhook secret"

  step issue {
    do   { http.post("https://vault.acme.com/rotate", "{}")? }
    undo { http.post("https://vault.acme.com/revoke",  "{}")? }
  }

  step record {
    do   { audit.event("secret.rotated",         { actor: "ops-bot" }) }
    undo { audit.event("secret.rotation_failed", { actor: "ops-bot" }) }
  }
}

fn main(cap) -> result<unit> {
  rotate(cap.subset[http.post @ ["vault.acme.com"], audit.event])
}
```

## Appendix C — minimum viable agent net

```aeris
use ai

model Doc@v1     { text: string where len(text) <= 50_000 }
model Summary@v1 { headline: string, bullets: list<string> where len(bullets) <= 5 }
model Critique@v1 { ok: bool, notes: string }

agent summarise {
  llm:     "claude-haiku-4-5"
  intent:  "Produce a 5-bullet summary of the document"
  prompt:  "Summarise the document in <=5 bullets."
  accept:  Doc@v1
  produce: Summary@v1
}

agent critique {
  llm:     "claude-opus-4-7"
  intent:  "Judge whether the summary is faithful"
  prompt:  "Return ok=true if the summary is faithful and complete."
  accept:  Summary@v1
  produce: Critique@v1
}

agent_net summarise_loop {
  intent "summarise → critique until acceptable"
  flow summarise -> critique
  until: critique.ok == true || iterations >= 3
}

fn main(d: Doc@v1, cap) -> result<Summary@v1> {
  summarise_loop(d, cap.subset[ai.complete @ ["claude-haiku-4-5", "claude-opus-4-7"]])
}
```

## Appendix D — script mode chatbot (v0.3)

A markdown-backed chatbot that wires a CLI-headless backend with no
`cap`, no `intent`, and no manual KB loading. The `enforce = "off"`
mode lets the script feel like the v0.1 spelling while keeping the
trace + replay + backend abstraction intact.

```toml
# aeris.toml
[project]
name  = "docs-chatbot"
aeris = "0.3.0"

[caps]
enforce = "off"

[ai.backend]
kind = "cli"
cmd  = "claude --print --dangerously-skip-permissions \
        --no-session-persistence --effort low \
        --output-format text --model claude-sonnet-4-6"
```

```aeris
fn main() {
  let chat = ai.chat(
    "Answer only from the FILE-tagged knowledge below.",
    "./docs",
  )
  io.println("loaded {chat.kb_size()} files")

  loop {
    io.print("you> ")
    let q = io.read_line() ?? ""
    if q == "" or q == "quit" { break }
    io.println("bot> " + chat.ask(q))
  }
}
```

Every v0.3 surface used here is opt-in:

- `enforce = "off"` removes the cap discipline,
- `loop { }` is the v0.3 sugar for `while true { }`,
- `?? ""` defaults a missing stdin line to an empty string,
- `ai.chat(system, dir)` + `.ask(prompt)` + `.kb_size()` are the
  M19.T6 KB builtins,
- `io.println` displays records and option/result values naturally.

Flipping `enforce` from `"off"` through `"loose"` to `"strict"`
forces increasingly more annotations onto the same code without
changing its meaning — see § 8.4.1.

---

*End of language reference. Companion documents: `thesis.md` (rationale),
`project.md` (constraints).*
