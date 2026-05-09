# Aeris v0.2.0 — Release Notes

The first publishable cut of Aeris: a single-binary DSL for ops,
governance, pipelines, and AI agents. Determinism is the integration
test; capabilities are values; sagas have explicit compensations;
LLM calls are taped and replayable.

This document maps each milestone in `docs/plan.md` to the artefacts
that prove it landed. Where a milestone produced a *golden trace*,
the path is given so the reader can `aeris replay` it bit-identically.

---

## Highlights

- **Single static binary** (`aeris`), < 8 MB stripped on Linux x86_64
  (M14.T1).
- **Five release targets**: Linux x86_64 / arm64 (musl), macOS x86_64
  / arm64, Windows x86_64 (M14.T2 / `.github/workflows/release.yml`).
- **Trace-first determinism**: `aeris replay <trace> <source>` is
  bit-identical for the deterministic subset (M9.T4, M9.T5).
- **Capabilities are values**: `cap[*]` rejected outside `main`'s
  synthesised cap; allow-lists intersect the lockset ceiling
  (M2.T5 / M2.T6).
- **Sagas have explicit compensations**: a write-effectful `do` with
  `undo noop` is a static error; rollback runs in reverse order with
  bounded retry → `PartialFailure` (M2.T8 / M6).
- **Models are versioned**: every reference carries `@vN`; bare model
  use is rejected (M2.T10).
- **Effect surface is enforced**: `aeris check` prints the surface
  diff as the first hunk when `.aeris/surface.lock` is stale
  (M2.T12 / M7.T5).
- **Human-grade diagnostics**: every error references its `language.md`
  section, quotes the source span with a `^^^^` underline, and adds a
  one-line "did you mean …?" hint (M13.T3 / T4 / T5). `aeris check
  --explain <code>` is manpage-style for codes 64–71 (M13.T6).

---

## Milestone-by-milestone artefacts

| Milestone | Output | Acceptance artefact |
|---|---|---|
| **M0** Bootstrap | workspace, CI, `aeris version` | `Cargo.toml`, `.github/workflows/ci.yml` |
| **M1** Lexer & parser | full `language.md` surface | 100+ round-trip fixtures (`syntax::fmt::tests::FIXTURES`) |
| **M2** Static analysis | `aeris check` exit codes 64 / 65 / 66 / 67 / 68 / 70 / 71 | 200 module-level idempotency fixtures + 30 negative-fixture diagnostic snapshots |
| **M3** Pure interpreter | `aeris run <pure_file>` | tree-walk evaluator over `runtime::eval` |
| **M4** Tracing + L1 | JSONL trace; `io`, `fs`, `env`, `clock` (N2), `random` (N2) | `aeris-tests/golden/m4/*.jsonl` (`io_println`, `fs_write_read`, `env_read`, `clock_random`) |
| **M5** http + shell + contracts | N4 allow-list runtime; `requires:` / `ensures:` | `runtime::http`; trace propagation tests |
| **M6** Sagas + idempotency | forward / rollback / `PartialFailure` | `aeris-tests/golden/m6/saga_success.jsonl`, `saga_rollback.jsonl`, `saga_partial_failure.jsonl` |
| **M7** Lockset + surface | blake3-shaped pinning, `main` cap, `surface.lock` | `lockset::lockset`, `lockset::surface` |
| **M8** Models + policies | `@vN` validation at trust boundaries; deny / require / limit / audit | `runtime::eval::apply_policies` |
| **M9** L2 `ai` + tape + replay | replay bit-identical | `runtime::replay::TapeHandle` + 8 replay fixtures |
| **M10** Agents + agent_net | typed dataflow with `until:` | parser / runner |
| **M11** L2 native handlers | audit / kube / docker / mongodb / minio / rabbitmq | `runtime::eval::lookup_builtin` per backend |
| **M12** Tests + properties + fmt + V1 narrow-caps | `aeris test` / `assert` / `property` / `aeris fmt` / `--narrow-caps` | 200 fmt fixtures, 10 property fixtures, 5 fixture-mode fixtures |
| **M13** Trace diff + `aeris doc` + diagnostics | `aeris trace diff`, `aeris doc`, human-grade errors | `runtime::trace_diff`, `syntax::doc`, `check::render` |
| **M14** Performance + packaging + release | static binary, cross-compile, benches, `aeris init` template | `tests/bench_*.rs`, `examples/`, `.github/workflows/release.yml` |

---

## Performance baselines (M14.T3 / T4 / T5)

Measured on the v0.2.0 dev workstation (macOS arm64, release build):

| Benchmark | Result | Acceptance budget |
|---|---|---|
| Pure-fn evaluator: `sum_to(50_000)` | ~30 ms | within 5× CPython |
| JSONL trace serialisation (200_000 events) | ~3.5 M ev/s | ≥ 100 k ev/s |
| Cold start (parse + check + module env) | < 1 ms | < 50 ms |

Reproduce with:

```sh
cargo test --release --test bench_evaluator -- --nocapture
cargo test --release --test bench_trace -- --nocapture
cargo test --release --test bench_cold_start -- --nocapture
```

---

## Examples (M14.T7)

The `examples/` tree ships three minimum-viable programs that mirror
`docs/language.md` Appendices A / B / C:

| Path | What it shows |
|---|---|
| `examples/hello/main.aer` | `fn main(cap)` + `io.println` (App. A) |
| `examples/saga/main.aer` | `saga` with `intent`, `do` / `undo`, `cap.subset[...]` (App. B) |
| `examples/agent_net/main.aer` | `model@vN`, `agent`, `agent_net` with `until:` (App. C) |

Each example carries its own `lockset.toml` so `aeris check` and
`aeris run` resolve `main`'s synthesised cap end-to-end.

---

## Breaking changes

None — this is the first published version.

---

## Verifying a release

```sh
shasum -a 256 -c aeris-v0.2.0-x86_64-unknown-linux-musl.tar.gz.sha256
gpg --verify aeris-v0.2.0-x86_64-unknown-linux-musl.tar.gz.asc
```

GPG signing is opt-in: the workflow signs only when the
`GPG_PRIVATE_KEY` and `GPG_PASSPHRASE` repo secrets are set.

---

## Six success criteria (`thesis.md` § 13)

1. **Compliance officer reads a saga signature in < 30 s** —
   `examples/saga/main.aer` lists every external resource on the
   first declaration line.
2. **Every effectful call has an enclosing `intent`** — enforced
   statically by M2.T7; `examples/saga` exhibits the pattern.
3. **Failed runs reproduce bit-identically** — see
   `aeris-tests/golden/m6/saga_rollback.jsonl` and
   `aeris-tests/golden/m4/clock_random.jsonl`. `aeris replay` keeps
   `clock` / `random` pinned to the recording.
4. **Mid-step saga failure leaves only `ok` / `rolled_back` /
   `PartialFailure` outcomes** —
   `aeris-tests/golden/m6/saga_partial_failure.jsonl`.
5. **Supply-chain dep-byte swap does not execute** — M7.T2's blake3
   hash check (`lockset::lockset::verify_local_deps`).
6. **LLM-generated PR adding a network call surfaces in review** —
   M2.T12: `aeris check` prints the `.aeris/surface.lock` diff as
   the first hunk before any other diagnostics.
