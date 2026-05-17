# 06 — Reverse-engineering pipeline

Take an opaque codebase under `./src/` and produce four markdown
documents that explain it: high-level analysis, requirements,
architecture, suggested improvements.

The script:

1. Runs a four-agent network (orchestrator → analyzer → requirements →
   architecture → improvements) over `./src/`.
2. Saves each agent's reply as a markdown file under `./docs/`.
3. Uploads the documents to MinIO.
4. Spawns an AI chat server (port 8888) backed by `ai.chat(system,
   dir)` over the generated docs — so a follow-up question gets
   grounded in what the agents produced.
5. Starts the dashboard backend on `http://localhost:8080`.

The bundled `./src/` is a small three-service Python codebase
(frontend, ticket-service, user-service) used as the running
example.

## Prerequisites

- CLI Claude backend.
- Optional: a real MinIO endpoint (otherwise the runtime stubs
  apply).

## Running

```sh
cd demo/06_reverse_engineering
aeris run ./main.aer
```

After the analysis the script keeps two servers alive:

- `http://localhost:8888/chat` — `POST { message: "..." }` to ask the
  chat agent.
- `http://localhost:8080` — the dashboard (with a built-in chat box
  that proxies to the chat server).

## What it shows

- `ai.network(max_rounds: 25)` with five role agents producing four
  files (orchestrator is coordination-only)
- Skip-if-cached logic via `fs.exists` over every expected output
- AI chat *server* assembled from `net.http` + `chat.ask` (the v0.1
  `ai.chat(...).start(port:)` helper was intentionally not
  re-introduced; the explicit composition is the v0.3 idiom)
- `minio.list` / `minio.get` / `minio.mb` from a small library
  module
