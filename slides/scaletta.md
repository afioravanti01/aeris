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

## Scaletta — 7 sezioni, 53 slide

Il deck è in due metà chiare: prima il linguaggio (sezioni 1–6), poi la
filosofia del progetto (sezione 7). La sezione 7 è il cuore — espone in
modo esplicito i razionali su LLM-as-author, riduzione del
non-determinismo e fondamenti teorici. Ogni slide della 7 usa bullet
list con i concetti chiave in **bold**.

### Apertura (2 slide)
1. Cover
2. Agenda — tabella delle 7 sezioni

### Sezione 1 — Aeris at a glance (2 slide)
3. What it is: runtime, libraries, LLM backend, what it replaces in one file
4. Hello world — script mode + main form

### Sezione 2 — How an interpreted language works (2 slide)
5. Lexer · parser · static check · interpreter — le 4 fasi mappate ai file Rust del runtime
6. AST walk — esempio `fn walk(node, env) -> Value` come la vecchia presentazione

### Sezione 3 — The four layers (1 slide)
7. Diagramma L1 / L2 / L3 / L4 + bullet "opt-in per profondità"

> "Why these four layers?" è stata spostata in sezione 7 — l'ordine
> nuovo è: prima si vede il linguaggio per intero, poi si spiega
> *perché* ha quella forma.

### Sezione 4 — Core language (15 slide)
Divider "Core language"
8. Language at a glance — let/var/const, kwargs, interpolation, closures
9. Control flow — if/match/loops, ranges, wildcards
10. Pattern matching — enums e destructuring
11. Models — record, enum, model@vN, extends, where
12. Errors & recovery — result, ?, ??, catch, defer
13. Time control — every, retry, timeout
14. Saga — flagship construct con do/undo
15. Idempotency key — blake3(trace_id ‖ step ‖ idx) + tabella iniezione
16. Concurrency — spawn, channel, cancellation cooperativa
17. Modules — tre layer, una keyword (use)
18. Standard library — general-purpose modules
19. Standard library — native domain handlers
20. A full HTTP server — net.http(port)
21. Tests — built into the language (assert, assert_status, assert_semantic)

### Sezione 5 — AI primitives (5 slide)
Divider "AI primitives"
22. ai.complete + ai.session (auto-compaction)
23. ai.decide + ai.usage
24. ai.chat(system, dir) + overload `port`
25. Multi-agent — agent_net declarativo vs ai.network programmatico

### Sezione 6 — Verifiability & Governance (12 slide)
Divider "Verifiability & Governance" — fonde la vecchia "Verifiability"
con la prima metà di "Governance & reasoning". Mostra in fila tutto
ciò che il linguaggio offre a livello di costrutti meccanicamente
verificabili e di governance dichiarativa.
26. cap — a permission carried as a value. **Prima volta che cap viene introdotto.**
27. Allow-list per family
28. Narrowing con cap.subset[…] + main(cap) sintetizzato
29. enforce = off | loose | strict
30. intent — executable documentation
31. requires: / ensures: come pre/post-conditions runtime
32. policy — declarative governance (deny/require/limit/audit/when)
33. Trace — cosa entra nel nastro JSONL
34. aeris replay + aeris trace diff
35. External libraries — content-addressed supply chain (blake3)
36. aeris.toml + surface.lock

### Sezione 7 — Why Aeris — design philosophy (13 slide)
Divider "Why Aeris — Design philosophy". Cuore concettuale del deck.
Tutte le slide qui sono **bullet list con keyword in bold**; nessun
code block.
37. Why these four layers? — i due requisiti (riduzione non-determinismo + verificabilità meccanica) e la mappatura sui quattro layer
38. The thesis — controlled non-determinism + tabella delle tre sorgenti (modello / grammatica / mondo)
39. Designed for LLMs (1/4) — Familiar carrier, domain inserts: graffe/match/named-args familiari + saga/agent/policy/intent/cap come inserti dominio
40. Designed for LLMs (2/4) — WHAT not HOW: l'LLM è autore principale, costrutti come intenzioni complete, non meccanismi
41. Designed for LLMs (3/4) — High abstraction not low: la tentazione opposta è sbagliata; due fattori (corpus + spazio dei completamenti)
42. Designed for LLMs (4/4) — Why-as-grammar: il *perché* portato dentro la sintassi (intent, requires/ensures, policy)
43. Reducing non-determinism (1/3) — The model: "capture, not control"; la promessa onesta è riproducibilità *dopo* la prima esecuzione
44. Reducing non-determinism (2/3) — The grammar: parole riservate, una forma canonica, cap come valore = la firma è la verità
45. Reducing non-determinism (3/3) — The world: intent / requires-ensures / cap / model@vN / policy come risposta allo stato esterno
46. Theoretical foundations — capability security, SAGA pattern, content-addressing in prosa operativa; "novelty è nell'assemblaggio"
47. Honest limits — cosa Aeris **non** promette (prima esecuzione, in-body correctness, cap over-broadening, cascading undo)
48. What Aeris refuses on principle — formal proofs automatici, inferenza di cap, dep mobili, plug-in native non firmati

### Chiusura (1 slide)
49. Thanks / Q&A (divider)

Totale: 49 slide informative + 4 divider espliciti = **53 slide**.

---

## Slide rimosse rispetto alla vecchia versione

- Vecchia slide 8 "Why these four layers?" → spostata in sezione 7 (slide 37).
- Vecchio divider "Governance & reasoning" → assorbito nel divider "Verifiability & Governance".
- Vecchie slide 35-37 "The thesis" + "Language for humans 1/2" + "(2/2)" → riformulate come slide 38, 40, 41, 42 in sezione 7 (più bullet, meno prosa, più bold).
- Vecchia sezione 8 "Putting it together" (3 slide: divider + SRE 1/2 + SRE 2/2) → rimossa.
- Vecchia slide 43 "Error model — layered exit codes" → rimossa (la tabella exit code resta documentata in `docs/language.md`).
- Vecchie slide 44-45 "Honest limits" + "What Aeris refuses on principle" → spostate nella sezione 7 finale come slide 47-48.

---

## Convenzioni di stile (eredita da `aeris/slides/`)

- **Cover** (`<!-- _class: cover -->`): box scuro top, logo AERIS via CSS, frase-tesi come `h2`.
- **Divider** (`<!-- _class: divider -->`): sfondo navy, titolo H1 bianco, sottotitolo come blockquote. Uno per ogni grande sezione (4, 5, 6, 7) + chiusura Thanks.
- **Slide tight** (`<!-- _class: tight -->`): per snippet di codice lunghi (riduce font del code block). Default usato sulle slide con code ≥ 12 righe.
- **Due colonne**: `<div class="columns"><div class="column">...` — utile per "codice / sintesi" o "lato sinistro narrativo / lato destro tabella".
- **Compact**: `<div class="column compact">` per bullet list con line-height ridotto, quando la colonna deve restare leggera.
- **Code blocks**: usare ```rust come language hint (highlight.js cade in default e il tema applica i colori dei token).

## Regola sulla nomenclatura "layer"

"L1 / L2 / L3 / L4" si riferiscono ai **quattro layer architetturali del linguaggio** (sintassi → multi-agent), introdotti nel diagramma e usati nei commenti degli esempi. Le slide della stdlib (sezione 4) si chiamano "Standard library — general-purpose modules" / "...native domain handlers", senza prefisso "Layer 1/2". External libraries (sezione 6) sono pinned per hash, non chiamarle "Layer 3" nelle slide.

## Regola su `cap`

`cap`, `cap.subset[...]`, `cap[http.post @ [...]]` compaiono **per la prima volta** nella sezione 6 (Verifiability & Governance, slide 26). Le slide pratiche (sezioni 1-5) mostrano programmi in modalità script — niente `cap` nelle firme. La saga pratica (slide 14) è senza `cap`; lo riceve implicitamente come `cap[*]` di `main` in modalità script.
