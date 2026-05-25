# Aeris v0.3 — Scaletta delle slide

Riferimento dell'outline usato per generare `presentation.md`. Taglio
tecnico, audience di **colleghi tecnici** (non marketing). Aeris è
descritta come un **toy language**, vehicle per un esperimento sul
design di un linguaggio per agentic coding.

## Vincoli

- Tema: `theme/aeris.css`
- Build: `npm install && npm run build` (HTML) o `npm run pdf` / `npm run pptx`.
- Lingua: English.
- **Tono**: peer-to-peer. Niente value-prop, niente "enterprise", niente "first defensive layer for X".
- **Densità**: poche frasi per slide, parole-chiave in **bold**, una sola quote di apertura e una di chiusura quando serve.
- **Riferimenti alla letteratura**: spiegati (origine, anno, idea), non solo citati. Tre slide dedicate: capabilities, sagas, content-addressing.
- **Font**: override del tema a `section { font-size: 40px; }` (default 36px).

## Scaletta — 22 slide

Il deck segue un filo logico in sei movimenti: motivazione → metodologia → design → letteratura applicata → il linguaggio → osservazioni e limiti. Niente sezione "philosophy" in coda: la filosofia è distribuita nei movimenti.

### Apertura (2 slide)
1. **Cover** — *"A small interpreted language. An experiment in designing for the era when code is written by models."*
2. **Agenda** — 6 voci.

### Motivazione (2 slide)
3. **Why a toy language** — onesto: Aeris è un toy, non un prodotto. La domanda: cosa cambia in un linguaggio se l'autore principale è un modello?
4. **What "agentic coding" means here** — definizione operativa: LLM = distribuzione su token → generazione stocastica + lettura shallow → due pressioni di design (meno ambiguità, più *why* nel sorgente).

### Metodologia (1 slide)
5. **How we worked — thesis → spec → plan → iterations** — il loop reale: `thesis.md` prima del codice, `language.md` derivato, `plan.md` con ~50 milestone + acceptance check. "The model proposed, the docs ruled, the checks verified."

### Design (2 slide)
6. **The design trilemma** — verifiability / readability / expressiveness; Aeris al centroide.
7. **Three sources of non-determinism** — tabella: modello (trace+replay) / grammatica (reserved keywords, forma canonica, `cap` come valore) / mondo (cap, intent, contracts, policy, model@vN).

### Letteratura applicata (4 slide, 1 divider)
8. *Divider* — **What we drew on**. Onestà: nessuna delle tre idee è nuova; la novità è la combinazione + l'autore.
9. **Capabilities as values** — Dennis & Van Horn 1966 (CACM) + E language (Miller ~2003) + Capsicum/Genode/Pony. Come Aeris usa: `cap` parametro, signature = authority graph.
10. **The SAGA pattern** — Garcia-Molina & Salem 1987 (SIGMOD) + Netflix/Temporal/Step Functions. Come Aeris usa: `do`/`undo` obbligatori, idempotency key auto-derivata.
11. **Content-addressed supply chain** — Nix (Dolstra 2006) + Cargo.lock + Go GOSUMDB. Come Aeris usa: blake3 nel manifest, ed25519 per L2.
12. **Why-as-grammar — the design move** — la claim load-bearing del progetto: `intent`, `requires:` / `ensures:`, `policy` come grammatica, non comment.

### Il linguaggio (4 slide)
13. **The four layers** — diagramma SVG + 4 layer spiegati in 1-2 righe; opt-in by depth.
14. **How the interpreter runs your program** — SVG dell'AST walk per `let x = add(2, 3)`: ordine di visita, statement (effetto su env) vs expression (ritorna Value).
15. **A concrete example** — saga deploy completa: model + agent + saga con intent + cap. Niente walkthrough costrutto-per-costrutto.
16. **Capture, not control — the honest promise** — il modello non si rende deterministico; Aeris cattura + `aeris replay` bit-identical; promise = riproducibilità *dopo* la prima esecuzione.

### Osservazioni · limiti · domande (4 slide, 1 divider)
17. *Divider* — **What we observed · what we refused · open questions**.
18. **What we observed while building this** — 4 osservazioni dichiarate come tali, *non* misure: meno alternative sintattiche, `intent` cambia il tipo di bug, trace+replay rende debug LLM trattabile, methodology tiene su ~50 milestone.
19. **Honest limits** — 4 limiti dichiarati: prima esecuzione non-deterministica, in-body correctness non verificata, cap broadening è process problem, methodology ~50 milestone (non sappiamo se scala).
20. **What we deliberately refused** — 4 rifiuti con costo dichiarato: no formal proofs, no cap inference, no mutable deps, no unsigned native plug-ins.
21. **Open questions** — 4 domande aperte: granularità `intent`, language vs methodology, generalizzazione a static typing, scalabilità oltre ~50 milestone.

### Chiusura (1 slide)
22. **Thanks** (divider) — repo + docs.

Totale: 19 slide informative + 3 divider espliciti = **22 slide**.

---

## Cambiamenti rispetto alle versioni precedenti

- **Da 41 a 22 slide.** Tagliate: 11 slide "Verifiability & Governance" (cap / allow-list / narrowing / enforce / intent / requires / policy / trace / replay / manifest), 5 slide "AI primitives", 13 slide "Why Aeris — Design philosophy" (assorbite in motivation + design + literature).
- **Tono.** Da "enterprise pitch" a "experiment between technical peers". Cover ridichiarata. Slide di motivazione esplicita "Aeris is a toy, not a product".
- **Riferimenti.** Letteratura citata con autore + anno + sede di pubblicazione, spiegata in 2-3 righe, poi mappata sull'uso in Aeris. Niente più "ispirato a" generico.
- **Honest limits + What we refused + Open questions** ora chiudono il deck come blocco distinto, non sparpagliati.

## Convenzioni di stile

- **Cover** (`<!-- _class: cover -->`): box scuro top, logo AERIS via CSS, sottotitolo dichiarativo.
- **Divider** (`<!-- _class: divider -->`): sfondo navy, titolo H1, sottotitolo come blockquote. Usato come sezione-marker (3 divider intermedi nel deck nuovo) e per Thanks.
- **Slide tight** (`<!-- _class: tight -->`): per le slide a due colonne dense (Trilemma, Four layers, AST walk) e per le slide di letteratura (3 sezioni in colonna).
- **Due colonne**: `<div class="columns"><div class="column">...` per "diagramma + bullet" o "codice + sintesi".
- **Compact**: `<div class="column compact">` per bullet list con line-height ridotto.
- **Code blocks**: ```rust come language hint (highlight.js fallback al default).
