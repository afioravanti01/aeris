# M10 — agent_net golden traces

Reference event-kind sequences for the four `agent_net` scenarios
called out by the M10.T6 acceptance (§ 14.1). Each `.jsonl` lists
one `kind` per line; the `runtime::eval::tests::golden_net_*`
fixtures run the corresponding source program against the in-memory
tracer and assert equality.

| File | Scenario |
|---|---|
| `net_linear_chain.jsonl` | linear chain, edge type-validation on every hop |
| `net_parallel_fan_out.jsonl` | parallel fan-out across two branches |
| `net_type_driven_routing.jsonl` | type-driven routing among branches (an unmatched branch produces `edge_skip` with `type_mismatch`) |
| `net_composition.jsonl` | an `agent_net` used as a node inside another `agent_net` |

The `_print_net_kinds_*` `#[ignore]`d tests regenerate these files
from the in-memory tracer on demand (`cargo test ... -- --ignored
--nocapture`).
