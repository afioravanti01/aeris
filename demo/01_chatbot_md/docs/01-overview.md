# Aeris v0.2 — overview

Aeris è un linguaggio interpretato scritto in Rust, distribuito come
singolo binario statico (< 8 MB stripped). I sorgenti hanno
estensione `.aer` e si eseguono con `aeris run <file>`.

## Quattro layer

- **L1** — sintassi base: lessico denso, tipi, control flow.
- **L2** — semantica verificabile: capability, contracts, intent.
- **L3** — agentic loop: saga con `do`/`undo` obbligatori.
- **L4** — multi-agent: `agent`, `agent_net`, `flow`, `until`.

I layer sono opt-in by depth: uno script puro vive in L1; una
pipeline self-recovering usa L1+L2+L3.

## Manifest

Il file `aeris.toml` raccoglie tutte le informazioni di progetto:
dipendenze content-addressed, allow-list delle capability, policy
attive, configurazione del backend AI. Sostituisce env var, manifest
e config sparsi nei tool tradizionali.

## Trace

Ogni esecuzione produce un file JSONL in `.aeris/traces/<id>.jsonl`.
`aeris replay <trace_id>` rigioca il programma bit-identical sul
subset deterministico.
