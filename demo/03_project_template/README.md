# 03 — Project template generator

Generate a project skeleton from a one-page markdown brief.

`template.md` describes the project (name, stack, layers, endpoints,
requirements); `agents/generator.md` is the system prompt that asks
the LLM to emit a JSON list of `{ path, content }` files. The script
parses the reply and writes every file under `./output/`.

A single multi-turn exchange via `ai.session` / `ai.session_ask`
keeps the conversation focused and the trace replayable.

## Prerequisites

CLI Claude backend (see `aeris.toml`). The default model is
`claude-sonnet-4-6`; the script also pins it in the `ai.session`
call so the backend choice is explicit.

## Running

```sh
cd demo/03_project_template
aeris run ./main.aer
```

After the run, inspect `./output/` for the generated files.

## What it shows

- `ai.session(system, model)` and `ai.session_ask(session, prompt)`
  (tuple return — the new session is in `result[0]`, the reply in
  `result[1]`)
- `shell.exec(["sh", "-c", "rm -rf ./output/*"])` for environment setup
- `string.split("/")` + `.slice(a, b)` + `.join("/")` to materialise
  nested directories from JSON paths
- `fs.mkdir` / `fs.write_text` for file emission
