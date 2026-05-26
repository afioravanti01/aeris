# Aeris

## Description

> Aeris is a **general-purpose interpreted language** written in Rust, built around a very specific domain: **ops, governance, pipelines and AI agents**.

Target: LLMs
Reviewer: Human

The principal author of code is now an LLM. An LLM doesn't have a mental model — it has a probability distribution over the next token. Writing code is, for an LLM, an intrinsically stochastic process.

An LLM doesn't have a mental model — it has a probability distribution over the next token. Three layers of non-determinism affect any agentic program.

- Model — same prompt, different output. Tackled with temperature=0, never eliminated.
- Language semantics — ambiguous constructs force the LLM to infer. pure fn / deterministic fn close this.
- State of the world — code acts on networks, DBs, FS that change. This is what governance addresses.

- Runtime 
    - Single static binary `aeris`. No runtime requirements.
    - File extension `.aer` · execution via `aeris run <file> [args...]` / `aeris test <file>` 
- Libraries
    - Builtin · Native modules · External libraries
- LLM Integration
    - Pluggable LLM backend (HTTP or CLI)
- Other
    - Concurrency via `spawn { … }` OS threads
    - Tracing: JSONL · `X-Aeris-Trace-Id`


- **A single source of truth** for ops, AI and governance
- **Zero external dependencies**: download the binary and you have everything
- **Curly-brace syntax** as other languages but with constructs dedicated to the domain (`pipeline`, `agent`, `policy`, `model`).

## Libraries

- **Level 1**
    - Stdlib: 10 built-in modules
- **Level 2**
    - 6 native modules via C ABI (Application Binary Interface)
    - `aeris-ai`, `aeris-kube`, `aeris-mongodb`, `aeris-docker`, `aeris-minio`, `aeris-rabbitmq`
- **Level 3**
    - External libraries: `use "github.com/..." lib@v`
    - External libraries: `use ./lib/...`

```rust
use io, json, fs, http, shell                  // Layer 1 — built-in stdlib
use ai, kube, docker, mongodb, rabbitmq        // Layer 2 — native .so modules
use "./lib/utils.aer"                          // Layer 3a — local .aer file
use utils from "./lib/utils.aer"               // Layer 3b — namespaced
use "github.com/acmecorp/aeris-devops" deploy@"1.2.0" // Layer 3c — GitHub
```

| Layer | Lib | Location |
|---|---|---|
| **1. Built-in** | `io`, `fs`, `http`, `shell`, `env`, `strings`, `date`, `json`, `yaml`, `net` | Binary |
| **2. Native** | `ai`, `docker`, `kube`, `minio`, `mongodb`, `rabbitmq` | `cdylib` `.so`, ABI |
| **3. Aeris** | local `.aer` files or github repo | Cache  `.aeris/ext/<host>__<repo>/<version>/` |


## Constraints

- Modular rust implementation
- AI automation and operations as first-class citizens.
- Pipelines and suite + tests
- AI, shell, kubernetes, docker libraries loaded dynamically
- Libraries in three layers
- Single sources of truth:
    - project.md, language.md, thesis.md

## Language layers

L1 · AI-native syntax 
L2 · Verifiable semantics 
L3 · Reversible execution — long-running scripts (LLM-driven or not) fail unpredictably on the outside world. Per-step trace + scope rollback make recovery deterministic over non-deterministic execution.
L4 · Multi-agent orchestration language

Create plan, milestones table and tasks. Upon completion of each task, mark the row as completed
Do not write unrequested documentation.
