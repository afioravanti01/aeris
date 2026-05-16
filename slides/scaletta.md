# Aeris v0.2 — Scaletta delle slide

Riferimento dell'outline usato per generare `presentation.md`. Taglio
tecnico, audience di sviluppatori interessati al progetto.

Tema: `theme/aeris.css` (ripreso dal deck v0.1 in `aeris/slides/`).
Build: `npm install && npm run build` (HTML) o `npm run pdf` / `npm run pptx`.

---

## Struttura — 6 atti, 22 slide

### ATTO I — Inquadramento (3 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 1 | Cover | Frase-tesi: capabilities-as-values, saga con compensation, supply chain content-addressed, replay bit-identical |
| 2 | Il problema tecnico | Tre fonti di non-determinismo (modello, semantica, mondo); perché sandboxing/type system/effect system tradizionali coprono solo un sottoinsieme |
| 3 | Cosa fa Aeris | Linguaggio interpretato in Rust; singolo binario < 8 MB; tree-walking interpreter; trace JSONL + `aeris replay` |

### ATTO II — Modello di esecuzione (4 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 4 | Divider "Modello di esecuzione" | — |
| 5 | I quattro layer | L1 sintassi → L2 semantica verificabile → L3 saga → L4 agent_net; opt-in by depth |
| 6 | Pipeline di esecuzione + exit code | Source → lexer → parser → check (M2) → eval; matrice exit code 0/64/65/66/67/68/69/70/71/74 |
| 7 | Determinismo e trace | Trace JSONL sempre attivo; `cap.clock` / `cap.random` / `cap.ai.*` registrati; replay bit-identical |
| 8 | Lockset come centro di gravità | `[deps]` blake3, `[caps]` allow-list, `[ai.backend]`, `[policies]`; `main` riceve cap sintetizzato |

### ATTO III — Sistema di capability (4 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 9 | Divider "Sistema di capability" | — |
| 10 | Capability come tipo first-class | `cap[op @ ["allow-list"]]`; parser rifiuta `http.post` senza `cap` in scope |
| 11 | Narrowing e propagazione | `cap.subset[...]`, mai broadening, regole di escape, `cap[*]` vietato nel codice utente |
| 12 | Body resolution | `http.post(...)` si lega al `cap` in scope; `use http` non abilita niente da solo |
| 13 | Surface lock (V3) | `.aeris/surface.lock`, diff come primo hunk in review, `aeris fmt --narrow-caps` |

### ATTO IV — Contracts, intent, model (3 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 14 | Divider "Contracts, intent, model" | — |
| 15 | Contracts runtime | `requires:` / `ensures:` con esempi; violazione → exit 64; nessun SMT |
| 16 | Intent obbligatorio (V2) | `intent "..."` block; ogni write-effectful deve stare dentro; exit 66 al parse; trace events |
| 17 | Model versionato `@vN` | Schema con `where`; validazione sui trust boundary; bare `Invoice` → exit 68; migrazione esplicita |

### ATTO V — Saga e agenti (4 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 18 | Divider "Saga e agenti" | — |
| 19 | Saga — anatomia | `step` con `do`/`undo`; `undo: noop` solo se `do` puro; esiti ok / rolled_back / PartialFailure (exit 74) |
| 20 | Idempotency key (N1) | `blake3(trace_id ‖ step_name ‖ idx)`; iniettata in HTTP/K8s/AMQP/Mongo; replay → no-op |
| 21 | Agent — single LLM unit | `llm`, `intent`, `prompt`, `accept`/`produce`, `retries`, `budget`; routing contract auto-iniettato |
| 22 | agent_net — dataflow tipato | DAG aciclico; fan-out type-driven; `until:`; net annidate |

### ATTO VI — Policy, refusal, limiti (4 slide)

| # | Titolo | Contenuto |
|---|---|---|
| 23 | Divider "Policy, refusal, limiti" | — |
| 24 | Policy come costrutto | `match`/`deny`/`require`/`limit`/`audit`/`when`; drift in replay → `policy_drift` event |
| 25 | Cose che il linguaggio rifiuta | No SMT, no tier system, no capability inference, no soft keyword, no import mutabili, no `.so` plugin |
| 26 | Limiti onesti del modello | Prima chiamata LLM non-deterministica; logica dentro cap legittima non verificata; cascading undo best-effort |
| 27 | Stato dell'implementazione | M0–M17 done, M18–M23 v0.3 pending; binario < 8 MB, zero deps runtime |

> **Conteggio reale**: 5 divider + 22 slide di contenuto = 27 slide.

---

## Convenzioni di stile (eredita da `aeris/slides/`)

- **Cover** (`<!-- _class: cover -->`): box scuro top, logo AERIS via CSS,
  frase-tesi come `h2`.
- **Divider** (`<!-- _class: divider -->`): sfondo navy, titolo H1 bianco,
  sottotitolo come blockquote. Uno per atto.
- **Slide tight** (`<!-- _class: tight -->`): per snippet di codice lunghi
  (riduce font del code block).
- **Due colonne**: `<div class="columns"><div class="column">...` —
  utile per "esempio negativo / esempio positivo" o "codice / spiegazione".
- **Callout**: `<div class="note">...</div>`, `<div class="tip">...</div>`.
- **Code blocks**: usare ```aeris come language hint (highlight.js cade
  in default e il tema applica i colori dei token).

## Filo conduttore

Usare l'esempio **`settle_invoice`** dall'attiI III in poi come ricorrenza:
- slide 10: la funzione `total` (pura) vs `settle` (con cap)
- slide 13: la sua surface in `.aeris/surface.lock`
- slide 15: i contratti di `pay`
- slide 19: la saga `settle` completa con 3 step
- slide 22: `invoice_pipeline` come agent_net che la consuma

Questo riduce il carico cognitivo: il lettore impara *un* dominio e lo vede
attraversare tutti i livelli del linguaggio.
