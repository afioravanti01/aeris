# Aeris v0.3 — Demo set

Self-contained Aeris projects (`main.aer` + `aeris.toml`) ported from
the v0.1 scenario gallery. All eight run under `enforce = "off"` and
pass `aeris check`. Some depend on external services (MinIO,
Python upstreams, OpenStreetMap Overpass); those are noted in each
project's own files.

| # | Scenario | What it demonstrates |
|---|---|---|
| 01 | `01_chatbot_md` | `enforce = "off"` + `loop` + `??` + `ai.chat(system, dir)` + `chat.ask` + CLI Claude backend |
| 02 | `02_chatbot_http` | HTTP server + `ai.chat(dir:)` knowledge base + concurrent request handling via `spawn` |
| 03 | `03_project_template` | `ai.session` / `ai.session_ask` driving file generation from a markdown brief |
| 04 | `04_seismic_sentinel` | Periodic `every 5m` ingest + 4-agent `ai.network` + MinIO persistence + dashboard HTTP server |
| 05 | `05_open_city` | Multi-source aggregator (Open-Meteo, OSM Overpass) + 3-agent network + MinIO + dashboard |
| 06 | `06_reverse_engineering` | 4-agent doc generator + AI chat server (`net.http` + `chat.ask`) + dashboard + MinIO docs |
| 07 | `07_crypto_pipeline` | Multi-step crypto chain (normalise → hash → sign → verify → encode → emit) + `defer` audit + shell-out |
| 08 | `08_api_gateway` | YAML-driven reverse proxy + fixed-window rate limiter + Python upstreams + `test` suite |

## Running

From the repo root:

```bash
cargo build --release
cd demo/<scenario>
aeris run ./main.aer
```

Some scenarios need extra services:

- `04_seismic_sentinel`, `05_open_city`, `06_reverse_engineering`
  use the MinIO bucket stubs — they run as mock under the default
  `aeris-core` and trace every op. Pointing to a real MinIO endpoint
  is configured via `MINIO_*` env vars.
- `08_api_gateway` expects three Python uvicorn services on ports
  8001 / 8002 / 8003. Start them with `aeris run upstreams.aer`
  (in another terminal) before `aeris run main.aer`.

## Anatomy — the v0.3 surface in one screen

```aeris
fn main() {
  let chat = ai.chat(
    "You are concise. Answer from the knowledge base.",
    "./docs",
  )
  io.println("loaded {chat.kb_size()} files")

  loop {
    io.print("you> ")
    let q = io.read_line() ?? ""
    if q == "" or q == "quit" { break }
    io.println("bot> " + chat.ask(q))
  }
}
```

No `cap`, no `intent`, no manual KB loading — script mode runs the
v0.3 surface as a high-level interpreted language. Flipping
`enforce` from `"off"` to `"loose"` or `"strict"` (per
`docs/language.md` § 8.4.1) tightens the discipline without
changing the program's meaning.

## v0.3 features each scenario exercises

| Feature | 02 | 03 | 04 | 05 | 06 | 07 | 08 |
|---|---|---|---|---|---|---|---|
| String interpolation `{x}` | yes | yes | yes | yes | yes | yes | yes |
| `loop`, `??` | yes |  | yes | yes | yes |  | yes |
| `catch`, `defer`, `error()` | yes | yes | yes | yes | yes | yes | yes |
| `every`, `retry`, `timeout` |  |  | yes | yes |  |  |  |
| `ai.session` / `ai.session_ask` |  | yes |  |  |  |  |  |
| `ai.chat(dir:)` | yes |  |  |  | yes |  |  |
| `ai.network` |  |  | yes | yes | yes |  |  |
| `net.http(port:)` + `HttpServer`/`HttpReq` | yes |  | yes | yes | yes |  | yes |
| `minio.{get,put,mb,bucket_exists,list}` |  |  | yes | yes | yes |  |  |
| `shell.exec(["sh","-c", ...])` |  |  |  |  |  | yes | yes |
| `assert_status` / `assert_json` / `assert_semantic` |  |  |  |  |  |  | yes |
| `list.map(f)` |  |  | yes |  |  |  |  |
| `string.index_of` |  |  |  | yes |  |  |  |
| `http.post(url, body, content_type:)` |  |  |  | yes |  |  |  |

## Known v0.3 surface gaps surfaced during the port

These are not blockers — every scenario has a working workaround —
but they should be tracked as future polish:

1. **Triple-quoted strings `"""..."""`** are documented in
   `docs/language.md` § 2.4 but not implemented by the lexer.
   Workaround: concatenate single-line strings with `+`.
2. **Tuple destructuring in `let (a, b) = expr`** is rejected by the
   parser. Workaround: `let r = expr; let a = r[0]; let b = r[1]`.
3. **Empty record / map literal `{}`** is always parsed as an empty
   block expression. There is no syntax for an empty map.
4. **Auto-semicolon insertion** is missing in some block positions.
   A bare expression like `[]` on the line after a call is read as a
   subscript of the previous line. Workaround: insert an explicit `;`.
5. **Reserved keywords as record-literal field names** (`limit:`,
   `match:`, `when:`, …) are rejected. Workaround: rename to a
   non-keyword field.

If you hit any of these in another scenario, please flag them — they
are candidates for a follow-up milestone.
