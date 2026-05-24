# 05-test-suite — assertion idioms in one demo

Three test bodies, each centred on a different assertion helper
from `language.md § 21.4`:

| Test | Helper | What it checks |
|---|---|---|
| `httpbin.org/get returns 200` | `assert_status` | the response status field matches the expected integer code |
| `httpbin.org/get echoes the requested URL` | `assert_json` | one named field of the decoded JSON equals an expected value |
| `static description matches a semantic criterion` | `assert_semantic` | the AI backend judges that a piece of text satisfies a natural-language criterion |
| `httpbin.org/uuid returns a UUID-shaped payload` | `assert_semantic` over HTTP | same judge, but over the raw body of a live HTTP response |

The point of the demo is the third row: `assert_semantic` lets a
test ask the model "does this *look right*?" instead of trying
to spell every acceptable answer in a regex. The first two rows
exist so you can compare the styles side by side.

## Running

The runtime needs:
- network reachability to `httpbin.org` (plain HTTP);
- the `claude` CLI in `$PATH` (`aeris.toml [ai.backend]` pins it).

```bash
cd demo/05-test-suite
aeris test                        # runs the whole `tests/` folder
aeris test ./tests/api.test.aer   # one specific file
```

## What you see at the console

```
ok    api::httpbin.org/get returns 200
ok    api::httpbin.org/get echoes the requested URL
ok    api::static description matches a semantic criterion
ok    api::httpbin.org/uuid returns a UUID-shaped payload
```

The trace under `.aeris/traces/<id>.jsonl` records every
`http_call`, every `ai_call`, and one `assert_semantic` per
semantic check — so a failed judgement can always be reviewed
and replayed with `aeris replay`.
