# 01 — Chatbot

> **One sentence.** A KB-loaded HTTP chatbot in a single
> `ai.chat(system:, dir:, port:)` call, with no glue code in
> between.

## What it shows

| Construct | Where |
|---|---|
| `ai.chat(system:, dir:, port:)` — three-arg form that builds a knowledge base from the directory and binds the integrated HTTP server (M35) | `main.aer` |
| `"""…"""` triple-quoted strings for the system prompt | `main.aer` |
| `fn main(args)` receiving CLI arguments as `list<string>` (M34.T2) | `main.aer` |
| `strings.parse_int(s) catch err { default }` — inline error recovery on a single expression | `main.aer` |
| Markdown documents used directly as the chatbot's knowledge base | `./docs/` (loaded by `ai.chat`) |
| Static HTML frontend served by the same process | `./index.html` |

## How it works

`ai.chat(system, dir, port)` is the "one call, whole chatbot"
sugar. The runtime:

1. Reads every `.md` file in `dir` and pins their contents into
   the LLM's system prompt as the knowledge base.
2. Stores the system prompt verbatim so every `/api/chat` request
   carries the same KB context.
3. Binds a single-threaded HTTP server on `port` and serves four
   endpoints:

   | Method | Path           | Response |
   |---|---|---|
   | `GET`     | `/`            | `./index.html` (the bundled frontend) |
   | `POST`    | `/api/chat`    | `{ "message": "..." }` → `{ "response": "..." }` |
   | `GET`     | `/api/health`  | `{ "status": "ok", "docs": N }` |
   | `OPTIONS` | `*`            | `204` with CORS headers |

The server is single-threaded by design: one LLM call blocks the
accept loop until the model replies (same constraint as M31's
inline `spawn` fallback). Concurrent users will queue.

## How to run

```bash
cd demo/01-chatbot
aeris run ./main.aer            # default port 8080
aeris run ./main.aer 3000       # custom port via main(args)
```

Then either open `http://localhost:8080` in a browser, or use
`curl`:

```bash
curl -s -X POST http://localhost:8080/api/chat \
     -H 'content-type: application/json' \
     -d '{"message":"what is enforce mode?"}'
```

The `claude` CLI must be on `$PATH` — see
`aeris.toml [ai.backend]` for the exact invocation, or change
`kind = "cli"` to `kind = "mock"` to exercise the server flow
without an LLM (it will echo the prompt back).

## Project layout

```
demo/01-chatbot/
├── main.aer            # the four-line entry: ai.chat(system, dir, port)
├── docs/               # the markdown knowledge base
│   ├── 01-overview.md
│   ├── 02-capabilities.md
│   ├── 03-saga.md
│   └── 04-agent.md
├── index.html          # the static frontend served at GET /
├── aeris.toml          # enforce = "off", Claude CLI backend
└── README.md
```

## Notes

- Adding a document to the KB is a `cp` away — drop a new `.md`
  under `./docs/` and restart `aeris run`. The whole `dir` is
  re-read on boot.
- Token usage is visible after every request in the JSONL trace
  at `.aeris/traces/<id>.jsonl`; `ai.usage()` exposes it
  programmatically too.
