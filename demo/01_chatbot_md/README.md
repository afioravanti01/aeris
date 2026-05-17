# 01 — Chatbot (markdown REPL)

A terminal chatbot backed by the Aeris documentation under `./docs`.
`ai.chat(system, dir)` loads every markdown file in the directory as
a labelled knowledge base; each `chat.ask(prompt)` calls the
configured LLM backend.

The whole program is ~25 lines and runs under `enforce = "off"` —
no `cap`, no `intent`, no manual corpus loading.

## Prerequisites

The `[ai.backend]` section in `aeris.toml` defaults to a CLI Claude
invocation. To work offline, change `kind = "cli"` to `kind =
"mock"` — the backend then echoes the prompt deterministically.

## Running

```sh
cd demo/01_chatbot_md
aeris run ./main.aer
```

Type a question, blank line or `quit` to exit.

## What it shows

- `enforce = "off"` script mode (top-level statements, no `main` wrapper would also work)
- `loop { … }` and `??` null-coalesce
- `ai.chat(system, dir)` + `chat.ask(prompt)` + `chat.kb_size()`
- `io.print` / `io.read_line` REPL (`io.print` flushes stdout so the prompt appears before input)
