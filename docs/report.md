# Aeris v0.2 — Guida pratica e confronto con la versione precedente

Report generato per chi vuole iniziare a usare Aeris v0.2, con un confronto
onesto con la versione precedente del progetto (in
`/Users/alessio/progetti/aeris/`). La parte AI è approfondita.

---

## 1. Da dove partire (10 minuti)

```sh
cd /Users/alessio/progetti/aeris-v02
cargo build --release
mkdir -p ~/playground/aeris && cd ~/playground/aeris

# 1. Crea uno scheletro di progetto: aeris.toml + src/main.aer
/Users/alessio/progetti/aeris-v02/target/release/aeris init

# 2. Esegui l'hello-world
/Users/alessio/progetti/aeris-v02/target/release/aeris run src/main.aer

# 3. Studia gli esempi che ship con il repo
cat /Users/alessio/progetti/aeris-v02/examples/hello/main.aer
cat /Users/alessio/progetti/aeris-v02/examples/saga/main.aer
cat /Users/alessio/progetti/aeris-v02/examples/agent_net/main.aer
```

Ordine consigliato di lettura della spec `docs/language.md`:
§ 4 (tipi) → § 7 (funzioni) → § 8 (capabilities) → § 10 (intent) →
§ 13 (agent) → § 14 (agent_net) → § 23 (handler L2 `ai`) →
§ 16 (modelli `@vN`) → § 24 (lockset).

---

## 2. Cheatsheet costrutti — Aeris v0.2

### 2.1 Lessico e tipi

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| Commenti | linea, blocco, doc estraibile da `aeris doc` | `// ...`, `/* ... */`, `/// ...` |
| Letterali numerici | `int`, `float`, durata (`3s`, `2h`), range (`0..10`) | `42`, `3.14`, `0..=100` |
| Letterali stringa | UTF-8 con interpolazione `\(...)`; multilinea con `"""` | `"hello \(name)"` |
| Letterali data | `date`, `timestamp`, `duration` come token primari | `2026-05-16`, `2026-05-16T08:30:00Z` |
| Primitivi | `bool`, `int`, `i8..u64`, `f32/f64`, `decimal`, `string`, `bytes`, `char`, `uuid`, `date`, `timestamp`, `duration`, `unit` | — |
| Collezioni | `list<T>`, `set<T>`, `map<K,V>`, `tuple<...>`, `option<T>`, `result<T>` | `map<string, int>` |
| `record` | struct nominale, by-value, immutabile, update con spread | `record User { id: uuid, name: string }` |
| `enum` | sum type a varianti unit/posizionali/named | `enum Status { Pending, Banned { reason } }` |
| `type` | alias puro (no validazione) | `type Email = string` |
| Generics | parametri di tipo monomorfizzati al call site | `fn first<T>(xs: list<T>) -> option<T>` |

### 2.2 Dichiarazioni e funzioni

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `fn` | funzione; pura se non ha parametro `cap` | `fn add(a: int, b: int) -> int { a + b }` |
| `fn` con `cap` | funzione con capability (effetti permessi) | `fn settle(cap: cap[http.post]) { ... }` |
| `pub` | rende la firma visibile nella `surface.lock` | `pub fn settle(...)` |
| `requires:` / `ensures:` | contratti pre/post — fatali se violati (E64) | `fn pay(...) requires: amount > 0 ensures: result.ok` |
| `const` | costante module-level | `const PI = 3.14159` |
| `let` / `var` | binding immutabile / mutabile (`var` solo function-scope) | `let x = 1`, `var y = 2` |

### 2.3 Espressioni e control flow

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `if/else` | è un'espressione | `let n = if x > 0 { 1 } else { -1 }` |
| `match` | pattern matching, esaurienza strutturale | `match s { Pending -> 1, _ -> 0 }` |
| `while`, `for ... in` | loop classici; `break`/`continue` etichettabili | `for i in 0..10 { ... }` |
| Lambda | espressione funzione | `xs.map(|x| x + 1)` |
| `?` operator | propaga `Err(e)` | `let s = fs.read_file(p)?` |
| `raise` | sugar per `return Err(...)` | `raise err.user("invalid")` |
| `result<T>` | `Ok(T) \| Err(err)`; `err` chiuso a 9 varianti | `result<Invoice@v1>` |
| `is` / `as` | refinement / coercion strutturale | `if r is Ok(v) { use(v) }` |

### 2.4 Capability system (la vera novità)

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `cap[op, ...]` | tipo capability: lista esatta di operazioni concesse | `cap[fs.read_file, http.post @ ["api.x.com"]]` |
| `op @ ["..."]` | allow-list concreta (host, path glob, model, bucket, queue, ...) | `http.post @ ["api.acme.com"]` |
| `cap.subset[...]` | restringe il `cap` ricevuto e lo passa più stretto | `cap.subset[http.post @ ["api.acme.com"]]` |
| `cap[*]` | proibito nel codice utente; solo `main` sintetizza dal lockset | (parse error E65) |
| Body resolution | dentro la fn, `<module>.<op>(...)` si lega al `cap` in scope | `http.post(url, body)` |
| Prototype / strict | `[caps].required = false` in `aeris.toml` permette fn senza `cap` per E65; `true` rigoroso | (§ 8.4.1) |

### 2.5 Intent — la regola del "perché"

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `intent "..."` block | ogni chiamata write-effectful (V2) deve stare dentro un `intent` | `intent "rotate cert" { fs.write_file(p, data) }` |
| `intent:` in `saga` | dichiarato una volta per tutti gli step | `saga r(cap) { intent "..." step a { ... } }` |
| `intent:` in `agent` | campo dell'agente, propagato al trace | `agent c { intent: "classify invoice", ... }` |
| Eventi trace | `intent_enter` / `intent_exit` con stringa, scope, outcome | — |

### 2.6 Sagas

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `saga <name>(cap)` | operazione multi-step con rollback inverso garantito | `saga rotate(cap: cap[...]) { ... }` |
| `step <name>` | unità con `do { }` e `undo { }` obbligatori | `step apply { do { ... } undo { ... } }` |
| `undo noop` | ammesso solo se `do` non è write-effectful (E67) | — |
| Chiave idempotenza | `blake3(trace_id ‖ step ‖ idx)` iniettata in `Idempotency-Key`, annotation K8s, `message-id` AMQP, sentinel Mongo | automatica |
| Esito | `ok` / `rolled_back` / `PartialFailure` (exit 74 quando `undo` esaurisce le retry) | mai stato a metà |

### 2.7 Modelli versionati (`@vN`)

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `model X@vN` | schema versionato; validato sui confini di trust (HTTP ingress, `json.decode`, agent boundary) | `model Invoice@v1 { id: uuid, amount: decimal where amount > 0 }` |
| `where` su campo | predicato per validazione runtime | `amount: decimal where amount > 0` |
| `where:` record-level | invariante cross-field | `where: status == Cancelled implies total == 0` |
| Migrazione | funzione pura esplicita, mai implicita | `fn migrate_v1_to_v2(old: Invoice@v1) -> Invoice@v2` |

### 2.8 Agenti — la parte AI

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `agent <name>` | dichiarazione singolo LLM unit | (vedi sotto) |
| `llm:` | modello pinnato come stringa | `llm: "claude-opus-4-7"` |
| `intent:` | intent string propagata al trace e al prompt | `intent: "classify invoice"` |
| `prompt:` | prompt template; il runtime appende il contratto di routing | `prompt: """..."""` |
| `accept:` / `produce:` | schemi `model@vN` per input e output, validati a ogni call | `accept: Invoice@v1`, `produce: Category@v1` |
| `policy:` | elenco di `policy` da applicare | `policy: [pii_redact, model_budget]` |
| `retries:` | retry su `SchemaViolation` | `retries: 3` |
| `budget:` | tetto su token e latency; sforamento → `BudgetExceeded` exit 1 | `budget: { tokens: 4_000, latency: 5s }` |
| Chiamata | come funzione, richiede `cap` con `ai.complete @ [model]` | `classify(inv, cap.subset[ai.complete @ ["claude-opus-4-7"]])` |

Esempio minimo di agente:

```aeris
model Inv@v1 { id: uuid, amount: decimal where amount > 0 }
model Cat@v1 { kind: string }

agent classify {
  llm: "claude-haiku-4-5"
  intent: "classify invoice"
  prompt: """Classify {input.amount} into kind."""
  accept: Inv@v1
  produce: Cat@v1
  retries: 2
  budget: { tokens: 2_000, latency: 3s }
}

fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Cat@v1> {
  intent "classify" {
    classify(Inv@v1 { id: uuid_v7(), amount: 99.0 }, cap)
  }
}
```

### 2.9 `agent_net` — dataflow tipato fra agenti

| Costrutto | Cosa fa | Esempio |
|---|---|---|
| `agent_net <name>` | DAG aciclico di agenti (cicli → E70 al parse) | `agent_net pipe { flow a -> b -> c }` |
| `flow x -> y` | arco singolo | — |
| `flow x -> { y, z }` | fan-out parallelo type-driven | rami selezionati per match `accept` ↔ `produce` |
| `until:` | predicato di convergenza iterativa | `until: classify.confidence > 0.95 \|\| iterations >= 3` |
| Composizione | una net può essere usata come nodo di un'altra net | `flow inner_net -> tail` |
| Esiti | `ok(value)` o `Err("agent_net <name> exhausted")` | — |

### 2.10 Policy come costrutto del linguaggio

| Clausola | Cosa fa |
|---|---|
| `policy <name>` | dichiarazione |
| `match:` | percorso di capability su cui si attiva (`http.*`, `ai.complete`, ...) |
| `deny:` | violazione se vero |
| `require:` | violazione se falso |
| `limit:` | quota su finestra (`tokens_per_minute = 60_000`) |
| `audit:` | aggiunge campi al trace event |
| `when:` | gate ambientale (`env == "production"`) |
| Attivazione | per import del modulo, per attributo `#[policy(name)]`, o via `aeris.toml [policies] active = [...]` |
| Drift | quando replay e live divergono → evento `policy_drift` in trace |

### 2.11 Concorrenza, errori, tracing

| Costrutto | Cosa fa |
|---|---|
| `spawn { }` / `await` | OS thread con `handle<T>`; cattura solo `cap.subset[...]` |
| `channel<T>(capacity: N)` | MPMC bounded; `ch.send(x)?`, `for x in ch { }` |
| Trace JSONL | sempre attivo in `.aeris/traces/<trace_id>.jsonl`; header `X-Aeris-Trace-Id` propagato |
| Replay | `aeris replay <trace> <source>` bit-identical sul subset deterministico; `--live` re-issue rete/LLM |
| `aeris trace diff` | allinea due trace per `(scope, ordinal)` e segnala divergenze |

### 2.12 Stdlib L1 (sempre disponibile)

`io`, `fs`, `http`, `shell`, `env`, `clock`, `random`, `strings`, `date`,
`json`, `yaml`, `net`. Le operazioni write-effettive richiedono `intent`
(V2) e `cap` in firma.

### 2.13 Handler L2 (capability-gated)

`ai` (complete, chat, embed, tools), `audit.event`, `kube` (apply, delete,
get, watch), `docker` (run, build, push, pull, inspect), `mongodb`
(read, write), `minio` (get, put), `rabbitmq` (publish, subscribe).
Backend `ai` selezionabile da `aeris.toml [ai.backend]` con
`kind = mock | http | cli` (più `url`/`auth`/`cmd`).

### 2.14 Lockset (`aeris.toml`)

```toml
[project]
name  = "settle-pipeline"
aeris = "0.2.0"

[deps]
deploy = { source = "github.com/acmecorp/aeris-devops", version = "1.2.0", hash = "blake3:..." }

[caps]
required        = false                       # prototype mode (M15)
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

### 2.15 CLI

`aeris run`, `aeris check [--explain <code>]`,
`aeris fmt [--check] [--narrow-caps]`, `aeris test`,
`aeris lock [--check]`, `aeris replay [--live]`, `aeris trace diff`,
`aeris doc`, `aeris init`, `aeris version`.

Exit codes: `0` ok, `64` parse/type/contract, `65` capability, `66` intent
mancante (V2), `67` saga `undo` mancante, `68` model `@vN` mancante,
`69` lockset stale/byte-swap, `70` ciclo `agent_net`, `71` allow-list
eccede il ceiling del lockset, `74` `PartialFailure` su saga.

---

## 3. Confronto con la versione precedente (`/Users/alessio/progetti/aeris/`)

La direzione del riscritto è chiara: **v1 era una toolbox larga e
morbida, v0.2 è un core stretto e formale**.

### 3.1 Cosa c'è in v0.2 e NON c'era in v1

| Costrutto v0.2 | Perché conta |
|---|---|
| `cap[...]` come tipo first-class, con allow-list `@` e `cap.subset` | in v1 non esisteva il tipo capability; gli effetti erano impliciti |
| Regola V2: ogni write-effectful deve stare in `intent` (E66) | v1 non lo richiedeva |
| `saga` con `step.do/undo` obbligatori, idempotency key automatica, esiti `ok/rolled_back/PartialFailure` | v1 non aveva sagas (aveva `pipeline` con `on_failure`, semantica diversa) |
| `model X@vN` obbligatorio | v1 aveva `model v1 Name` e `extends v1`, ma non lo richiedeva sui confini di trust |
| `surface.lock` con diff in prima posizione (V3) | v1 non aveva |
| Replay bit-identical del trace JSONL (`aeris replay`) | v1 aveva solo `ai.record`/`ai.replay` limitato all'AI |
| `aeris trace diff`, `aeris check --explain`, `aeris fmt --narrow-caps` | nuovi |
| Exit code matrix (64–71, 74) | nuova |
| Lockset come centro di gravità (`[caps]`, `[ai.backend]`, `[policies]`, blake3 dep pinning) | v1 si basava su env var (`AERIS_LLM_ENDPOINT`, `AERIS_API_KEY`, ecc.) |
| Body-resolution della capability | v1 leggeva env var globali |

### 3.2 Cosa c'era in v1 e MANCA in v0.2

**Toolkit AI di v1 (la parte più ricca, ridotta in v0.2):**

| In v1 | In v0.2 |
|---|---|
| `ai.ask(p)`, `ai.ask_with(p, model)` | `ai.complete(prompt)` (semantica simile, ma deve essere in `intent`) |
| `ai.ask<Model>(p)` con augmentation prompt + validation | l'equivalente è `agent { accept, produce }` (più formale, meno inline) |
| `ai.session(system, model)` multi-turn con auto-compaction sopra 40 msg | **assente** — non c'è il concetto di sessione |
| `ai.chat(system, dir, model, port)` con REPL terminale o server HTTP | **assente** |
| `ai.decide(p, choices, retries)` enum-style | **assente** — si emula con un `agent` che `produce` un `enum` |
| `ai.extract(schema, from)`, `ai.generate(schema, count)` | **assente** — di nuovo via `agent` |
| `ai.ensemble(p, models, strategy)` (majority/unanimous/first) | **assente** |
| `ai.eval(output, criteria, scale, judge_model)` (LLM as judge) | **assente** |
| `ai.index()` + `.search()` (RAG keyword) | **assente** |
| `ai.guard`, `ai.budget`, `ai.cache` come call helper | budget esiste come campo dell'`agent`; guard/cache **assenti** come funzioni |
| `ai.usage()` | **assente** |
| `ai.record/replay` programmatico | rimpiazzato da `aeris replay` lato CLI (più potente, ma non in-process) |

**Pipelines e network:**

| In v1 | In v0.2 |
|---|---|
| `pipeline Name(arg) { steps: ... on_failure: ai.ask(...) }` con stato `last_step/last_error/last_output` | `agent_net` (più formale: tipi `accept`/`produce`, fan-out, `until:`, ma niente `on_failure` callback inline) |
| `ai.network(max_rounds: N)` hub-and-speak con JSON routing block | `agent_net` con type-driven routing (più rigoroso, niente hub-and-speak letterale) |

**Concorrenza e tempo:**

| In v1 | In v0.2 |
|---|---|
| `every "5m" { ... }` ciclo a intervallo | **assente** (puoi simularlo con `while` + `sleep`, ma `sleep` non è L1) |
| `retry N, delay: secs { ... }` blocco | **assente** come blocco; le retry esistono solo dentro `agent { retries: }` e nel rollback delle saghe |
| `timeout N { ... }` | **assente** come blocco; `budget.latency` esiste solo negli agenti |

**Errori:**

| In v1 | In v0.2 |
|---|---|
| `expr catch err { ... }` inline | **assente** — si usa `result<T>` + `?` + `match` |
| `error("msg")` per sollevare | `raise err.user("msg")` |
| `defer statement` esecuzione LIFO a fine fn | **assente** |

**Networking:**

| In v1 | In v0.2 |
|---|---|
| `net.http(port).accept()` HTTP server | **assente** — `http` è solo client |
| `net.listen` / `net.connect` TCP, `net.udp` | **assente** |
| `net.resolve` DNS | **assente** (c'è `net.dns` nella spec di v0.2 § 22 ma il runtime non lo implementa) |

**Testing:**

| In v1 | In v0.2 |
|---|---|
| `assert_status`, `assert_json`, `assert_semantic` (LLM judge) | solo `assert` generico nel test harness |
| `@example(args) -> expected` annotazione | **assente** |
| `suite "..." { setup { } }` | l'unità è il file (`tests/foo.test.aer`), niente `setup` |

**Sistema di tipi:**

| In v1 | In v0.2 |
|---|---|
| `pure fn` / `deterministic fn` come annotazioni di effetto | implicito: una funzione è pura se non ha parametro `cap` |
| `model v2 Name extends v1 Name { ... }` con compat check | `model X@vN` indipendenti + funzione di migrazione esplicita |
| Enum field shorthand `Enum["a", "b"]` | `enum Status { A, B }` standard |

**Layer 2 nativo (i backend di servizio):**

In v1 i moduli L2 erano molto più completi. In v0.2 sono al minimo
dell'acceptance:

| Modulo | v1 | v0.2 |
|---|---|---|
| `docker` | run, build, push, pull, inspect, stats, logs, exec, cp, network/volume/compose, prune, df, version | run, build, push, pull, inspect |
| `kube` | logs, get, apply, delete, describe, rollout, scale | apply, delete, get, watch |
| `mongodb` | find/find_one/insert/update/delete/count/aggregate/index/drop con connection e collection objects | read, write (stub) |
| `minio` | object + bucket + list + stat | get, put |
| `rabbitmq` | conn/channel, exchange/queue/binding/qos/publish/consume/ack/nack/reject | publish, subscribe (stub) |
| `audit` | non esisteva come modulo dedicato | `audit.event` con log append-only e idempotency |

**Plugin nativi (`.so`):**

v1 li supportava (auto-discovery via `$AERIS_MODULES_PATH`, ABI C).
**v0.2 li vieta esplicitamente** (rifiutati da `thesis.md` § 9.6, vedi
anche `docs/plan.md` § 9). Tutti gli handler L2 sono compilati dentro il
binario.

**REPL, sessioni, server HTTP** sono tutti assenti in v0.2.

### 3.3 Cosa cambia in forma ma c'è in entrambi

- **Modelli versionati**: in v1 era convenzione (`model v1 Name`), in v0.2
  è una proprietà sintattica (`Name@vN`).
- **Politiche**: in entrambi esistono come costrutto; v0.2 le rende più
  strette (`match`/`deny`/`require`/`limit`/`audit`/`when`) e le integra
  con replay drift.
- **Spawn**: in entrambi, ma v0.2 vieta che `cap` non-narrowed esca dal
  closure (deve passare per `cap.subset`).

### 3.4 Mappa di migrazione pratica per chi viene da v1

| Quello che facevi con… | …in v0.2 fai con… |
|---|---|
| `ai.ask("...")` libero | una funzione che riceve `cap: cap[ai.complete @ ["..."]]` e chiama dentro `intent "..." { ai.complete(prompt) }` |
| `ai.ask<Model>("...")` con validazione | un `agent` con `accept`/`produce` |
| `ai.session()` multi-turn | un `agent` chiamato in loop, passando la storia come parte di `accept` |
| `ai.decide(p, ["A", "B"])` | un `agent` che `produce` un `enum Choice { A, B }` |
| `ai.network(max_rounds: 3)` | un `agent_net` con `until: iterations >= 3` |
| `pipeline { steps: ... on_failure: ai.ask(...) }` | un `agent_net` con un agente terminale `fallback` instradato per type |
| `retry 3, delay: 1s { ... }` | per chiamate LLM: `agent { retries: 3 }`. Per chiamate di rete generiche: scrivi tu il `for` con conteggio |
| HTTP server | non c'è — esegui un servizio esterno (Docker, K8s) e parla con `http.get/post` |
