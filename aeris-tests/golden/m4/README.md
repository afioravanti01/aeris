# M4 — golden traces

Reference JSONL traces for the M4 L1 capability ops (`io`, `fs`,
`env`, `clock`, `random`). Each `.jsonl` file is the *kind sequence*
expected when the corresponding fixture runs — one event per line.

The trailing `\n`-terminated lines are checked by
`runtime::eval::tests::golden_*_kind_sequence`. Per-run fields
(`trace_id`, `ts`) vary across runs and are therefore omitted from
the golden files; only the `kind` of each event is asserted. A
future `aeris trace diff` (M13.T1) will refine the comparison to
include semantic fields (`path`, `len`, `hash`).
