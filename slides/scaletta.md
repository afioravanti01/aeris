# Aeris v0.3 — Scaletta delle slide

Riferimento dell'outline usato per generare `presentation.md`. Taglio
tecnico, audience tecnica di professionisti (non marketing)

## Vincoli

- Tema: `theme/aeris.css`
- Build: `npm install && npm run build` (HTML) o `npm run pdf` / `npm run pptx`.
- Lingua: English.
- **Tono**: tecnico, enterprise, professionale
- **Densità**: poche frasi per slide, parole-chiave in **bold**, una sola quote di apertura e una di chiusura quando serve.
- **Riferimenti alla letteratura**: spiegati (origine, anno, idea), non solo citati. ù
- **Font**: override del tema a `section { font-size: 40px; }` (default 36px).
- Sorgenti:
    - ../docs/thesis.md
    - ../docs/language.md
    - ../docs/project.md
- Spiegare concetti in modo chiaro, semplice
- Gerarchie a due o tre livelli

## Scaletta
- Aeris at a glance (i primi tre capitoli di thesis.md)
    - Razionali di aeris
    - Differenze con linguaggi noti
- How an interpreted language works
- Core language
    - 3 slides sul core del linguaggio
- AI primitives
- Verifiability & Governance
    - cap, intent, contracts, policy, trace, supply chain
- Aeris — design philosophy
    - The thesis, designed for LLMs, reducing non-determinism
