---
marp: true
theme: aeris
paginate: true
html: true
size: 16:10
title: "Aeris v0.2"
header: 'TECHNICAL PRESENTATION · v0.2'
footer: 'Aeris v0.2 · interpreted language for ops, AI and governance'
---


<!-- _class: cover -->

<p class="eyebrow">Technical presentation · v0.2</p>

## AERIS v0.2

Linguaggio interpretato per **ops, AI e automation** — pensato per essere scritto *e* letto da LLM. Capabilities come valori, compensation obbligatoria, supply chain content-addressed, replay bit-identical.

---

# Agenda

| # | Atto | Contenuto |
|---|---|---|
| **I** | Inquadramento | Problema, posizionamento, anatomia del linguaggio, perché LLM-friendly |
| **II** | Modello di esecuzione | Quattro layer, pipeline, trace, lockset |
| **III** | Sistema di capability | Tipo first-class, narrowing, body resolution, surface lock |
| **IV** | Contracts, intent, model | `requires`/`ensures`, `intent` obbligatorio, `@vN` |
| **V** | Saga e agenti | Compensation, idempotency, `agent`, `agent_net` |
| **VI** | Policy, refusal, limiti | Policy runtime, cosa il linguaggio rifiuta, stato |

---

# Il problema tecnico

> Codice prodotto e letto da LLM. Tre fonti di non-determinismo che si sommano e nessun layer tradizionale le copre insieme.

- **Modello** — stesso prompt, output diverso. `temperature=0` riduce, non elimina
- **Semantica del linguaggio** — costrutti ambigui forzano l'LLM a inferire
- **Stato del mondo** — rete, DB, FS mutano sotto i piedi del programma

I rimedi noti coprono solo un sottoinsieme:

- **Sandboxing** (Docker, gVisor) — controlla il processo, non sa quale funzione tocca cosa
- **Type systems** (TS, Java, Rust) — parlano di dati, non di **effetti**
- **Effect systems accademici** (F\*, Liquid Haskell, Koka) — rigore non pagabile su codice enterprise
- **DSL/framework** (Airflow, LangChain) — convenzione, non enforcement

---

# Cosa fa Aeris

<div class="columns">
<div class="column compact">

- Linguaggio **interpretato**, scritto in Rust
- **Singolo binario statico** `aeris` < 8 MB, zero dipendenze runtime
- Tree-walking interpreter (no bytecode, no JIT) — V0.2 sceglie semplicità sopra performance
- File `.aer`, esecuzione via `aeris run <file>` / `aeris test <file>`
- Ogni esecuzione produce un **trace JSONL** in `.aeris/traces/<trace_id>.jsonl`
- `aeris replay <trace> <source>` rigioca l'esecuzione bit-identical sul subset deterministico

</div>
<div class="column compact">

**Quattro layer di costrutti:**

- **L1** — sintassi (lessico, tipi, control flow)
- **L2** — semantica verificabile (capability, contracts, intent)
- **L3** — agentic loop (`saga` con `do`/`undo`)
- **L4** — multi-agent (`agent`, `agent_net`)

I layer sono *opt-in by depth*: uno script puro vive in L1; una pipeline self-recovering usa L1+L2+L3.

</div>
</div>

---

# Un linguaggio per ops, AI, automation

<div class="columns">
<div class="column compact">

**Per chi è**

- Platform / ops engineer che oggi cucina YAML + Python + shell + Terraform
- Autore di pipeline AI che oggi compone LangChain + prompt-string + retry manuali
- Team con vincoli di audit / compliance che non possono fidarsi di "un container che gira"

**Cosa rimpiazza, in un solo file**

- Lo script ops (`bash` / Python) → `.aer` con `intent` e capability esplicite
- Il manifesto pipeline (Airflow / Argo) → `saga` con `do`/`undo` obbligatori
- Il grafo agenti (LangChain / CrewAI) → `agent_net` tipato, validato edge-by-edge
- La policy egress (OPA / `policy.rego`) → `policy` valutata a runtime

</div>
<div class="column compact">

**Tre modalità d'uso**

- **Script** (v0.3, `enforce = "off"`) — top-level statements, no `cap`, no `main`. Per automation rapide, prototipi, demo
- **Pipeline** (`enforce = "loose"`) — funzioni con `cap`, manifest come ceiling. Per ops che salgono progressivamente
- **Mission-critical** (`enforce = "strict"`) — disciplina v0.2 piena. Per produzione, audit, compliance

**Il trace e il replay non si toccano mai.** Tutte e tre le modalità emettono JSONL e supportano `aeris replay` bit-identical sul subset deterministico — l'audit non è una feature opt-in.

> Lo stesso linguaggio scala da `aeris run triage.aer` allo stack di produzione regolamentata.

</div>
</div>

---

<!-- _class: tight -->

# Anatomia di un programma Aeris

<div class="columns">
<div class="column">

```aeris
// Script v0.3 — niente main, niente cap, niente boilerplate.
// Triage di log con un LLM headless (claude --print).

let session = ai.session(
  system: "Classify a log line: critical | warning | info.",
  model:  "claude-haiku-4-5",
)

let lines = fs.read_file("./error.log")
              .split("\n")

for line in lines.slice(0, 50) {
  let kind = ai.decide(
    prompt:  "Classify: {line}",
    choices: ["critical", "warning", "info"],
  )?

  if kind == "critical" {
    audit.event("triage.critical", { line: line })
  }
}

io.println("triage done — {ai.usage().calls} LLM calls")
```

</div>
<div class="column compact">

**Cosa è visibile a colpo d'occhio**

- **Top-level statements** (M26) — niente `fn main`, lo script gira
- **String interpolation** (M16) — `"{line}"`, `"{ai.usage().calls}"`
- **Named arguments** (v0.3) — `system:`, `model:`, `choices:` su builtin
- **AI di prima classe** — `ai.session`, `ai.decide`, `ai.usage` nella stdlib, non in libreria esterna
- **`?` propaga `err.llm`** quando il modello non rispetta `choices`
- **Trace JSONL** — ogni `ai.decide`, ogni `audit.event` finisce in `.aeris/traces/<id>.jsonl`, riproducibile con `aeris replay`

> Stessa identica grammatica scala fino al programma "settle" dell'Atto III: si aggiunge `fn`, `cap`, `intent`, `saga` — non si cambia linguaggio.

</div>
</div>

---

# LLM-friendly per costruzione

<div class="columns">
<div class="column compact">

**Grammatica densa, una sola forma legale**

- Tutti i keyword **riservati** — niente soft keyword, `grep step` è autorevole
- **Una forma canonica** per ogni costrutto: `aeris fmt` è totale, non parziale
- **Nessuna alternativa sintattica** (no `function`/`def`/`fn` insieme, solo `fn`)
- Densità: una `saga` di 10 step sta in mezza pagina, un `agent_net` in 6 righe

> *Fewer decisions to make → fewer points of failure.* Lo spazio dei completamenti validi è piccolo, l'LLM ha meno modo di sbagliare.

**Why-as-grammar**

- `intent "..."` — il *perché* del write è antenato grammaticale, non un commento
- `model X@vN` — schema versionato sui trust boundary, validato a runtime
- `policy` — guardrail come costrutto, non come prompt-string convention
- `requires:` / `ensures:` — pre/post-condizioni come parte della firma

</div>
<div class="column compact">

**AI built-in, non bolted-on**

- `ai.session` / `ai.session_ask` con auto-compaction 40→20
- `ai.decide(prompt, choices)` enum-style con retry su `SchemaViolation`
- `ai.chat(system, dir)` carica una knowledge base markdown in startup
- `ai.network(max_rounds)` programmatico + `agent` / `agent_net` dichiarativi
- Backend `http` *o* `cli` (`claude --print`, `ollama`, ...) — niente SDK linkati

**Reproducibility built-in**

- Ogni `ai.*` registrato come `ai_call` nel trace (`prompt`, `model`, `response`, `tokens`)
- `aeris replay <trace>` rigioca offline, bit-identical
- La **prima** esecuzione resta stocastica; ogni replay è deterministico
- Coerente con l'audience che conta: audit, debug, post-mortem, regression test

> Aeris non promette LLM deterministici. Promette **un linguaggio che li tiene visibili, registrati e replayabili**.

</div>
</div>

---

<!-- _class: divider -->

# Modello di esecuzione

> I quattro layer, la pipeline source → trace, la matrice degli exit code, il lockset come centro di gravità.

---

# I quattro layer

| Layer | Cosa aggiunge | Costrutti |
|---|---|---|
| **L1** AI-native syntax | Lessico denso, una forma canonica per costrutto, keyword tutti riservati | `fn`, `record`, `enum`, `match`, `if`, `for` |
| **L2** Verifiable semantics | Capability come valori, contratti runtime, intent obbligatorio | `cap[...]`, `requires:` / `ensures:`, `intent` |
| **L3** Agentic loop | Saga con `do`/`undo` obbligatori, idempotency automatica, esiti deterministici | `saga`, `step`, `do`, `undo` |
| **L4** Multi-agent | LLM unit tipato, dataflow aciclico, schema validation a ogni edge | `agent`, `agent_net`, `flow`, `until:` |

> Composizione *opt-in by depth*. Niente runtime tax per chi non sale di layer.

---

# Pipeline di esecuzione

```text
.aer source ─► lexer ─► parser ─► check (M2) ─► tree-walk eval ─► JSONL trace
                                       │                              │
                                       │                              └─► .aeris/traces/<id>.jsonl
                                       └─► exit non-zero al primo errore strutturale
```

**Exit code matrix** — un codice per categoria di violazione, scriptable in CI:

| Code | Causa |
|---|---|
| `0` | Esecuzione ok |
| `64` | Errore di parse / type / contratto |
| `65` | Capability mancante o `cap[*]` nel codice utente |
| `66` | Chiamata write-effectful fuori da un blocco `intent` (V2) |
| `67` | `saga step` con `do` write e `undo: noop` |
| `68` | `model` senza `@vN` su trust boundary |
| `69` | Lockset stale o byte-swap di una dipendenza |
| `70` | Ciclo dichiarato in `agent_net` |
| `71` | Allow-list di firma eccede il ceiling del lockset |
| `74` | Saga in `PartialFailure` (undo esaurito le retry) |

---

# Determinismo e trace

<div class="columns">
<div class="column">

Il **trace JSONL** è sempre attivo. Ogni interazione con il mondo non-deterministico finisce in una riga:

- `cap.ai.*` → `prompt`, `model`, `response`, `tokens`, `ts`
- `cap.clock.now()` → timestamp letto
- `cap.random.next()` → valore generato
- HTTP / shell → hash di request e response

**`aeris replay <trace> <source>`** rigioca il programma:

- Legge dal nastro invece di chiamare il mondo
- Bit-identical sul subset deterministico
- `--live` re-issue rete e LLM (per debug differenziale)

`aeris trace diff` allinea due trace per `(scope, ordinal)` e segnala le divergenze.

</div>
<div class="column">

```json
{"event":"intent_enter","intent":"settle invoice batch",
 "scope":"main.settle","ts":"2026-05-16T08:30:00Z"}
{"event":"ai_call","scope":"classify",
 "prompt":"Classify the invoice...",
 "model":"claude-opus-4-7",
 "response":"{\"kind\":\"utilities\"}",
 "tokens":142}
{"event":"http_request","scope":"main.settle.charge",
 "url":"https://api.acme.com/charge",
 "idempotency_key":"blake3:7a3f...",
 "req_hash":"...","resp_hash":"..."}
{"event":"intent_exit","outcome":"ok"}
```

</div>
</div>

---

# Lockset come centro di gravità

`aeris.toml` raccoglie *tutto* ciò che era sparso fra env var, manifest e config:

```toml
[project]
name  = "settle-pipeline"
aeris = "0.2.0"

[deps]
deploy = { source = "github.com/acmecorp/aeris-devops",
           version = "1.2.0", hash = "blake3:..." }

[caps]
required        = true                          # strict mode (M15)
http.allow      = ["api.acme.com"]
fs.allow_write  = ["./out/**"]
ai.models       = ["claude-opus-4-7", "claude-haiku-4-5"]

[ai.backend]
kind = "http"
url  = "https://api.anthropic.com"
auth = "env:ANTHROPIC_API_KEY"

[policies]
active = ["production_egress"]
```

- Hash mismatch su una dep → errore fatale **prima** dell'esecuzione
- `main` riceve il `cap` **sintetizzato dal lockset** — nessun altro modo per fabbricare un `cap` da zero

---

<!-- _class: divider -->

# Sistema di capability

> Capabilities come valori passati per parametro. La signature è il contratto. Niente namespace globale, niente effetti nascosti.

---

# Capability come tipo first-class

<div class="columns">
<div class="column">

```aeris
// Funzione pura: niente parametro cap → niente effetti, mai.
fn total(items: list<Invoice@v1>) -> decimal {
  items.fold(0, fn(acc, it) { acc + it.amount })
}

// Funzione effettuale: l'intera surface è in firma.
fn settle(
  items: list<Invoice@v1>,
  cap: cap[
    http.post @ ["api.acme.com"],
    audit.event,
  ],
) -> result<unit> {
  intent "settle invoice batch" {
    for it in items {
      http.post("https://api.acme.com/charge", it)?
    }
    audit.event("settle.complete", { count: len(items) })
  }
}
```

</div>
<div class="column compact">

- `cap[op @ ["allow-list"]]` — il tipo elenca le operazioni concesse e i loro vincoli
- Allow-list per host, path glob, model, bucket, queue, ...
- Una funzione **senza** parametro `cap` non può fare IO/rete/AI: il parser rifiuta `http.post(...)` se nessun `cap` in scope contiene `http.post`
- Il body resolution lega `<module>.<op>(...)` al `cap` ricevuto — non c'è namespace globale

> Object-capability security applicato agli effetti esterni, non all'aliasing di memoria.

</div>
</div>

---

# Narrowing e regole di escape

<div class="columns">
<div class="column">

```aeris
fn settle(items, cap: cap[
  http.post @ ["api.acme.com", "api.stripe.com"],
  fs.write_file @ ["./out/**"],
]) {
  // Restringo prima di passare a charge:
  // mai broadening, sempre subset.
  charge(items, cap.subset[
    http.post @ ["api.stripe.com"]
  ])
}

fn charge(items, cap: cap[http.post @ ["api.stripe.com"]]) {
  intent "charge cards" {
    for it in items { http.post(it.endpoint, it)? }
  }
}
```

</div>
<div class="column compact">

**Regole strutturali (verificate da M2):**

- `cap.subset[...]` ammette solo restringimenti — broadening rifiutato
- `cap` non può essere salvato in record, ritornato come tipo non-cap, inviato in un `channel`
- `cap[*]` proibito nel codice utente (E65)
- Solo `main` riceve il `cap` sintetizzato dal lockset

> Un cap che scende lungo l'albero delle chiamate può solo restringersi. Un'attacco che inietta una chiamata a `evil.com` muore al parser, non in produzione.

</div>
</div>

---

# Body resolution

`http.post(...)` **non** è una chiamata a un namespace globale. È risolta verso il `cap` in scope.

```aeris
use http                                          // rende il modulo visibile

fn ping_status() -> int {
  http.get("https://api.acme.com/health")?.status // E65: nessun cap in scope
}

fn ping_status_ok(cap: cap[http.get @ ["api.acme.com"]]) -> int {
  http.get("https://api.acme.com/health")?.status // ok
}
```

- Importare un modulo capability-bearing **non** introduce alcuna funzione globale `http.post`
- Aggiungere `use http` in cima al file non abilita niente; serve il `cap` nella firma
- Un LLM che genera codice e dimentica di dichiarare la capability fallisce **al compile time**, non in runtime

---

# Surface lock — V3

Per ogni funzione `pub`, la sua effect surface va in `.aeris/surface.lock`:

```toml
[settle]
caps = [
  "http.post @ [\"api.acme.com\"]",
  "audit.event",
]

[total]
caps = []
```

- Una PR che amplia una surface (aggiunge una sub-cap) **deve** rigenerare il lockfile
- Il diff del `surface.lock` appare come **primo hunk** nella review
- Le contrazioni non richiedono re-lock
- `aeris fmt --narrow-caps` (V1) propone restringimenti basati sull'uso reale del body

> Una PR LLM-generata che aggiunge una chiamata di rete a una funzione prima air-gapped è visibile a colpo d'occhio nella review.

---

<!-- _class: divider -->

# Contracts, intent, model

> Tre costrutti per portare il *perché* dentro la grammatica: pre/post-condizioni, intent obbligatorio sui write, schemi versionati ai trust boundary.

---

# Contracts — `requires:` / `ensures:`

```aeris
fn pay(
  amount: decimal,
  account: string,
  cap: cap[http.post @ ["api.stripe.com"]],
) -> result<Receipt@v1>
  requires: amount > 0
  requires: len(account) == 26
  ensures:  result.ok implies result.value.amount == amount
{
  intent "charge customer" {
    let resp = http.post("https://api.stripe.com/v1/charges",
                          { amount, account })?
    Ok(Receipt@v1 { amount, txn_id: resp.id })
  }
}
```

- Pre-condizioni controllate **all'ingresso**, post-condizioni a **ogni return path**
- Violazione → `ContractViolation`, flush della trace, exit 64
- **Non** catchabile da `?` — è un errore strutturale, non un `Err` di dominio
- Niente SMT, niente proof obligations: tutto runtime, errori actionable

---

# Intent obbligatorio — V2

Ogni chiamata **write-effectful** deve stare dentro un blocco `intent "..."`.

<div class="columns">
<div class="column">

```aeris
// E66 — parse-time error:
// "missing enclosing intent for write-effectful call"
fn rotate_cert(cap: cap[fs.write_file @ ["/etc/ssl/**"]]) {
  fs.write_file("/etc/ssl/new.pem", new_pem())?
}
```

```aeris
// OK — intent dichiara il perché.
fn rotate_cert(cap: cap[fs.write_file @ ["/etc/ssl/**"]]) {
  intent "rotate TLS cert before 30-day expiry" {
    fs.write_file("/etc/ssl/new.pem", new_pem())?
  }
}
```

</div>
<div class="column compact">

**Op write-effectful**: `cap.fs.write*`, `cap.http.{post,put,patch,delete}`, `cap.kube.apply`, `cap.audit.*`, `cap.ai.*`

**Trace events:**

- `intent_enter` — stringa, scope, ts
- `intent_exit` — outcome, durata
- ogni evento dentro il body porta `"intent": "..."` come campo

> Il *perché* diventa antenato grammaticale di ogni effetto. Una PR non può nascondere una write nel silenzio.

</div>
</div>

---

# Model versionato — `@vN`

```aeris
model Invoice@v1 {
  id:       uuid
  amount:   decimal where amount > 0
  customer: string  where len(customer) <= 64
  status:   InvoiceStatus

  where: status == Cancelled implies amount == 0   // invariante cross-field
}

// Bare `Invoice` senza @vN su un trust boundary → exit 68.
fn ingest(raw: string) -> result<Invoice@v1> {
  json.decode<Invoice@v1>(raw)        // validazione automatica al confine
}
```

- Validazione **automatica** sui trust boundary: HTTP ingress, `json.decode`, agent boundary
- `where` per campo (predicato) e `where:` record-level (invariante cross-field)
- Migrazione tra versioni = funzione pura **esplicita**, mai implicita
- Bare `Invoice` rifiutato al check (E68) — versioning forzato dove conta

---

<!-- _class: divider -->

# Saga e agenti

> Operazioni multi-step con compensation obbligatoria. LLM unit tipati. Dataflow aciclico fra agenti.

---

<!-- _class: tight -->

# Saga — anatomia

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

  step record {
    requires: ledger.ok
    do   { audit.event("settle.complete",    { count: len(batch) }) }
    undo { audit.event("settle.rolled_back", { count: len(batch) }) }
  }
}
```

- `do`/`undo` obbligatori. `undo: noop` ammesso **solo** se il `do` non è write-effectful (E67)
- Esiti deterministici: `ok` | `rolled_back` | `PartialFailure` (exit 74) — mai stato a metà

---

# Idempotency key — N1

Ogni capability di scrittura riceve automaticamente:

```text
key = blake3(trace_id ‖ step_name ‖ invocation_index)
```

| Capability | Iniezione |
|---|---|
| `http.post/put/patch` | Header `Idempotency-Key: <key>` |
| `kube.apply` | Annotation `aeris.dev/idempotency-key: <key>` |
| `rabbitmq.publish` | `message-id: <key>` |
| `mongodb.insert` | Sentinel field `_aeris_idem: <key>` |

- Replay di uno step già completato → **no-op** lato backend
- Cascading undo durante rollback è retry-driven con queste chiavi
- Un retry su un undo già parzialmente applicato non duplica l'effetto

> Riduce la frequenza pratica di `PartialFailure` senza pretendere di eliminarla.

---

# Agent — single LLM unit

<div class="columns">
<div class="column">

```aeris
model Invoice@v1  { id: uuid, amount: decimal where amount > 0 }
model Category@v1 { kind: string }

agent classify {
  llm:     "claude-haiku-4-5"
  intent:  "classify invoice into expense kind"
  prompt:  """
    Classify the invoice with amount {input.amount}.
    Return JSON with a single field `kind`.
  """
  accept:  Invoice@v1
  produce: Category@v1
  retries: 2
  budget:  { tokens: 2_000, latency: 3s }
}

fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]])
  -> result<Category@v1>
{
  intent "classify one invoice" {
    classify(Invoice@v1 { id: uuid_v7(), amount: 99.0 }, cap)
  }
}
```

</div>
<div class="column compact">

- `accept`/`produce` sono **`model@vN`** validati a ogni invocazione
- Routing contract auto-iniettato dal runtime nel system prompt — non scritto a mano
- `retries:` su `SchemaViolation` con backoff
- `budget:` per token e latency; sforamento → `BudgetExceeded`, exit 1
- Ogni invocazione registrata nel trace JSONL (tape recorder N3)

</div>
</div>

---

# agent_net — dataflow tipato

```aeris
agent_net invoice_pipeline {
  flow extract  -> classify  -> route
  flow classify -> { audit, archive }            // fan-out type-driven
  flow audit    -> notify_finance

  until: classify.confidence > 0.95 || iterations >= 3
}
```

- **DAG aciclico** — ciclo dichiarato → E70 al parse
- Routing risolto per **match `accept` ↔ `produce`**: il runtime sa quale ramo prendere dai tipi
- `until:` per loop di convergenza con bound massimo
- Una net può essere usata come **nodo** di un'altra net — composizione gerarchica
- Esiti: `ok(value)` o `Err("agent_net <name> exhausted")`

> Il protocollo di routing diventa parte del *programma*, non una stringa di prompt.

---

<!-- _class: divider -->

# Policy, refusal, limiti

> Policy come costrutto. Le scelte rifiutate per principio. I limiti onesti del modello.

---

# Policy come costrutto

```aeris
policy production_egress {
  match:  http.*
  deny:   url.host not in ["api.acme.com", "api.stripe.com"]
  audit:  { url, method }
  when:   env == "production"
}

policy ai_budget {
  match: ai.complete
  limit: tokens_per_minute = 60_000
  audit: { model, tokens }
}
```

- Attivata via **import del modulo**, **attributo** `#[policy(name)]`, o **lockset** `[policies] active`
- Valutata su ogni chiamata che matcha — non "ricordata" da un system prompt
- Violazione → `PolicyViolation`, trace event, exit 1
- **Drift in replay**: se la policy diverge fra live e replay, evento `policy_drift` nel trace

---

# Cose che il linguaggio rifiuta — e perché

| Refusal | Motivazione tecnica |
|---|---|
| **No SMT / refinement types** | Il verdetto del solver dipende dalla macchina → introduce non-determinismo *nel tooling*, peggio di quello che proviamo a controllare |
| **No tier system** (`draft/standard/verified`) | La semantica del boundary fra tier è inevitabilmente confusa: cosa succede se `standard` importa `draft`? Nessuna risposta ergonomica |
| **No capability inference** | Cambi interni a un callee modificherebbero silenziosamente la surface dei caller; il diff PR diventa ingannevole |
| **No soft keyword** | Lookahead position-dependent produce bug di parsing sottili (`time` token splittato silenziosamente). `grep step` deve trovare ogni `step` |
| **No import di riferimenti mutabili** | Niente `latest`, niente `*`, nessun tag git mutabile. Ogni `use X@vN.M.P` deve combaciare col lockset o fallisce alla risoluzione |
| **No `.so` plugin** | Un binario su disco introdurrebbe una effect surface invisibile al check M2. Romperebbe la verificabilità statica delle capability |

---

# Limiti onesti del modello

| Limite | Cosa significa |
|---|---|
| **Prima esecuzione LLM non-deterministica** | Il tape recorder rende `aeris replay` bit-identical *dopo* la prima run. Quella prima resta in balìa del modello: `temperature=0` riduce la varianza, non la elimina |
| **Logica dentro capability legittima non verificata** | Se una funzione tiene legittimamente `cap[audit.write]` e scrive l'attore sbagliato, Aeris non se ne accorge. Garantiamo visibilità (firma) e obbligo (intent, saga); la correttezza del body è responsabilità di test, review, RBAC |
| **Cascading undo best-effort** | L'`undo` di uno step può a sua volta fallire. Ritentiamo con idempotency key di N1; dopo retry, evento `PartialFailure` ed exit 74 → richiesta risoluzione umana. Limite noto del pattern SAGA, nessun linguaggio lo elimina |

> Aeris è la **prima linea difensiva**, non l'unica. Promettere di più sarebbe disonesto con l'audience che conta (compliance, audit, security).

---

# v0.3 — superficie ergonomica

<div class="columns">
<div class="column compact">

**Stringhe, controllo, errori**

- Interpolazione `"hi {name}"` con `\{` / `\}` escapes (M16)
- `loop { … }` sugar per `while true` (M24)
- `??` null-coalesce: `Ok/Some/value → v`, `Err/None/() → rhs` (M24)
- `expr catch err { … }`, `error("...")`, `defer stmt` (M17)
- `every 5s { }`, `retry 3, delay: 1s { }`, `timeout 30s { }`, `clock.sleep` (M18)

**Tipi e moduli**

- `model X@v2 extends X@v1 { … }` (M23) — fields + `where:` ereditati
- Top-level statements senza `main` (M26)
- Parametri non tipati per script (`fn f(x, y)`) (v0.3)
- `strings.*`, `date.*`, `json.pretty/parse`, `yaml.parse` (M24)

</div>
<div class="column compact">

**Toolkit AI**

- `ai.session(system, model)` + `ai.session_ask(s, p)` con auto-compaction 40→20 (M19)
- `ai.decide(p, choices, retries?)` enum-style (M19)
- `ai.usage() → { total_tokens, cost_usd, calls }` (M19)
- `ai.chat(system, dir)` + `chat.ask` + `chat.kb_size` (M19.T6)
- `ai.network(max_rounds)` builder programmatico (M28)
- Backend `cli` per `ai.complete` (M9.T1) — subprocess spawn

**Test helpers**

- `assert_status`, `assert_json`, `assert_semantic` (M21) — l'ultimo usa il backend AI come giudice

</div>
</div>

> Trace, replay, `model@vN` e `policy` restano attivi sopra **tutta** la superficie v0.3: nessuna ergonomia toglie l'audit.

---

# Rilassamento controllato del non-determinismo

La tesi v0.2 fissa la disciplina; la pratica v0.3 ammette che non tutti i progetti vogliono pagarla *da subito*. Tre modalità, una sola sorgente di verità.

| Modalità | Default `aeris init` | Effetto |
|---|---|---|
| `enforce = "off"` | **sì** (v0.3) | `cap[*]` sintetizzato a `main`; niente E65/E66/E67/E71; niente allow-list runtime |
| `enforce = "loose"` | — | manifest cap come ceiling runtime; le fn senza `cap` restano ammesse; le fn con `cap` sono check-ate normalmente |
| `enforce = "strict"` | — | piena disciplina v0.2 (`intent` obbligatorio, `cap[*]` rifiutato nel codice utente, surface lock) |

**Cosa NON cambia mai:**

- Trace JSONL sempre attivo
- `aeris replay` bit-identical sul subset deterministico
- Validazione `model@vN` ai trust boundary
- `policy` valutata su ogni chiamata che matcha

> Il rilassamento è solo del **vincolo statico**, non della **registrazione runtime**. Un progetto può salire la ladder (`off` → `loose` → `strict`) senza riscrivere codice — solo aggiungendo annotazioni.

---

# Stato dell'implementazione

| Tag | Milestone | Stato |
|---|---|---|
| **v0.1** | Prototipo esplorativo: AI builtins, L2 handlers, network listeners, inline errors — senza cap typing, intent, replay | legacy |
| **v0.2** | M0–M8 bootstrap → policy (parser, check, interp, trace, http, saga, manifest, model) | done |
| | M9–M15 AI + replay, agent_net, L2 handlers, test/fmt, diagnostics, packaging, prototype mode | done |
| **v0.3** | M15B `enforce = off \| loose \| strict` · M24 script surface (`loop`, `??`, `strings.*`, methods, natural JSON) | done |
| | M16 interpolazione `{x}` · M17 `catch`/`error`/`defer` · M18 `every`/`retry`/`timeout`/`clock.sleep` · M23 `model extends` | done |
| | M19 AI builtins (`session`, `decide`, `usage`, `ai.chat(dir:)`+`chat.ask`+`kb_size`) · M21 `assert_status`/`json`/`semantic` | partial |
| | M20 network listeners · M22 parità L2 estesa con v0.1 | deferred |

> Binario v0.2: < 8 MB stripped · tree-walk interpreter · zero dipendenze runtime.
> M20/M22 rinviati: richiedono runtime async / SDK esterni fuori dallo scope di v0.3.
