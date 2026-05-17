# Aeris

A small interpreted language for ops, governance, pipelines and AI agents.

**Status:** `v0.3.0`. See `RELEASE.md` for the v0.2.0 baseline; v0.3
adds three-mode capability enforcement, script-friendly surface
(`loop`, `??`, top-level statements, untyped params), inline error
recovery (`catch`/`error`/`defer`), time-control sugar
(`every`/`retry`/`timeout`/`clock.sleep`), `model X@v2 extends X@v1`,
the AI builtin family (`session`/`decide`/`usage`/`chat(dir:)`),
test helpers (`assert_status`/`assert_json`/`assert_semantic`), and
kwargs on user-defined functions.

## Documents

- [`docs/thesis.md`](docs/thesis.md) — rationale (non-negotiable)
- [`docs/language.md`](docs/language.md) — language specification (authoritative for surface)
- [`docs/cheatsheet.md`](docs/cheatsheet.md) — tabular quick reference of every construct and API
- [`docs/project.md`](docs/project.md) — constraints
- [`docs/plan.md`](docs/plan.md) — implementation plan and progress

## Build

```sh
cargo build --workspace
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
