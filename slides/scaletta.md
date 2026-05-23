# Aeris v0.3 — Scaletta delle slide

Riferimento dell'outline usato per generare `presentation.md`. Taglio
tecnico, audience di sviluppatori interessati al progetto. Struttura
ispirata a `/Users/alessio/progetti/aeris/slides/presentation.md`
(versione v0.1.0-m5), adattata alla grammatica di v0.3.

## Vincoli

- Tema: `theme/aeris.css`
- Build: `npm install && npm run build` (HTML) o `npm run pdf` / `npm run pptx`.
- Lingua: English
- **Densità**: un concetto per slide. Bullet di una riga, parole chiave in **bold**. Tabelle ≤ 5 righe. Code block ≤ 15 righe. Quando un argomento eccede, *spezza la slide* (es. "Pattern matching (1/2)" + "(2/2)").
- **Font**: override del tema a `section { font-size: 44px; }` (default 36px).
- **Niente citazioni accademiche**: no SMT, F\*, Liquid Haskell, Pony, Dennis & Van Horn, Garcia-Molina, AMQP, MPMC, GOSUMDB, AST come jargon nudo. Quando serve giustificare una scelta, prosa operativa.
- **`cap` solo nella sezione Verifiability**. Le slide pratiche (Core language, AI primitives) sono in modalità script — niente `cap` nelle firme.
- File di accompagnamento alle slide `discorso-presentazione.md` -> spiegazione completa delle slide.

## Scaletta — 8 sezioni, 51 slide

### Apertura (2 slide)
1. Cover
2. Agenda — tabella delle 8 sezioni

### Sezione 1 — Aeris at a glance (2 slide)
3. What it is: runtime, libraries, LLM backend, what it replaces in one file
4. Hello world — script mode + main form

### Sezione 2 — How an interpreted language works (2 slide)
5. Lexer · parser · static check · interpreter — le 4 fasi mappate ai file Rust del runtime
6. AST walk — esempio `fn walk(node, env) -> Value` come la vecchia presentazione

### Sezione 3 — The four layers (2 slide)
7. Diagramma L1 / L2 / L3 / L4 + bullet "opt-in per profondità"
8. Why these four layers? — LLM authors + readers, requisiti di non-determinismo e verificabilità

### Sezione 4 — Core language (15 slide)
Divider "Core language"
9. Language at a glance — let/var/const, kwargs, interpolation, closures
10. Control flow — if/match/loops, ranges, wildcards
11. Pattern matching — enums e destructuring
12. Models — record, enum, model@vN, extends, where
13. Errors & recovery — result, ?, ??, catch, defer
14. Time control — every, retry, timeout
15. Saga — flagship construct con do/undo
16. Idempotency key — blake3(trace_id ‖ step ‖ idx) + tabella iniezione
17. Concurrency — spawn, channel, cancellation cooperativa
18. Modules — tre layer, una keyword (use)
19. Standard library — general-purpose modules
20. Standard library — native domain handlers
21. A full HTTP server — net.http(port)
22. Tests — built into the language (assert, assert_status, assert_semantic)

### Sezione 5 — AI primitives (5 slide)
Divider "AI primitives"
23. ai.complete + ai.session (auto-compaction)
24. ai.decide + ai.usage
25. ai.chat(system, dir) + overload `port`
26. Multi-agent — agent_net declarativo vs ai.network programmatico

### Sezione 6 — Verifiability (5 slide)
Divider "Verifiability"
27. cap — a permission carried as a value. **Prima volta che cap viene introdotto.**
28. Allow-list per family
29. Narrowing con cap.subset[…] + main(cap) sintetizzato
30. enforce = off | loose | strict

### Sezione 7 — Governance & reasoning (11 slide)
Divider "Governance & reasoning"
31. The thesis — controlled non-determinism + tre sorgenti
32. Language for humans → language for agents (1/2): WHAT not HOW + high abstraction
33. Language for humans → language for agents (2/2): why-as-grammar
34. intent — executable documentation
35. requires: / ensures: come pre/post-conditions runtime
36. policy — declarative governance (deny/require/limit/audit/when)
37. Trace — cosa entra nel nastro JSONL
38. aeris replay + aeris trace diff
39. External libraries — content-addressed supply chain (blake3)
40. aeris.toml + surface.lock

### Sezione 8 — Putting it together (3 slide)
Divider "Putting it together"
41. SRE triage (1/2) — model@vN + agent + agent_net
42. SRE triage (2/2) — policy + saga + every

### Wrap up (4 slide)
43. Error model — layered exit codes
44. Honest limits
45. What Aeris refuses on principle
46. Thanks / Q&A (divider)

Totale nel file: 46 slide informative + 5 divider espliciti = **51 slide**.

---

## Convenzioni di stile (eredita da `aeris/slides/`)

- **Cover** (`<!-- _class: cover -->`): box scuro top, logo AERIS via CSS, frase-tesi come `h2`.
- **Divider** (`<!-- _class: divider -->`): sfondo navy, titolo H1 bianco, sottotitolo come blockquote. Uno per ogni grande sezione (4, 5, 6, 7, 8) + chiusura Thanks.
- **Slide tight** (`<!-- _class: tight -->`): per snippet di codice lunghi (riduce font del code block). Default usato sulle slide con code ≥ 12 righe.
- **Due colonne**: `<div class="columns"><div class="column">...` — utile per "codice / sintesi" o "lato sinistro narrativo / lato destro tabella".
- **Compact**: `<div class="column compact">` per bullet list con line-height ridotto, quando la colonna deve restare leggera.
- **Code blocks**: usare ```rust come language hint (highlight.js cade in default e il tema applica i colori dei token).

## Regola sulla nomenclatura "layer"

"L1 / L2 / L3 / L4" si riferiscono ai **quattro layer architetturali del linguaggio** (sintassi → multi-agent), introdotti nel diagramma e usati nei commenti degli esempi. Le slide della stdlib (sezione 4) si chiamano "Standard library — general-purpose modules" / "...native domain handlers", senza prefisso "Layer 1/2". External libraries (sezione 7) sono pinned per hash, non chiamarle "Layer 3" nelle slide.

## Regola su `cap`

`cap`, `cap.subset[...]`, `cap[http.post @ [...]]` compaiono **per la prima volta** nella sezione 6 (Verifiability, slide 27). Le slide pratiche (sezioni 1-5) mostrano programmi in modalità script — niente `cap` nelle firme. Le saghe pratiche (slide 16 e 41) sono senza `cap`; lo ricevono implicitamente come `cap[*]` di `main` in modalità script.
