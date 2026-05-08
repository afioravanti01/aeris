# M6 — saga golden traces

Reference event-kind sequences for the three saga outcomes (§ 12).
Each `.jsonl` lists one `kind` per line; the `runtime::eval::tests::
golden_saga_*` fixtures run the corresponding source program against
the in-memory tracer and assert equality.

The `aeris run` driver opens a saga with `intent_enter`, then
`saga_enter`, then runs `step_enter` / `step_exit` per step. On
failure the rollback path adds `rollback_enter` and `undo_enter` /
`undo_exit` for each completed step in reverse. Exhausted undo
retries surface a `partial_failure` event before `intent_exit`.
