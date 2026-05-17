# 07 — Crypto / hashing pipeline

A six-step content sealing pipeline:

1. `normalize_input` — trim, lowercase, strip ASCII control chars,
   re-serialise JSON with sorted keys (when the input parses).
2. `hash` — SHA-256 of the normalised content.
3. `sign` — HMAC-SHA256 of the hash, keyed by `AERIS_HMAC_KEY`.
4. `verify` — re-compute the HMAC and compare; mismatch aborts the
   pipeline.
5. `encode` — Base64-encode the payload and wrap it in a versioned
   envelope.
6. `emit` — write the result to a file or to stdout.

The crypto primitives are shell-outs to `openssl` / `base64` with
deterministic stubs as fallback, so the demo runs portably.

The v0.1 version was expressed with a `pipeline { steps: ...
on_failure: … }` block. v0.3 does not introduce that construct — it
collapses to a saga without `undo`, which the thesis explicitly
refuses. The same shape is here as a plain function chain with `?`,
`catch` and `defer` for the audit trail.

## Prerequisites

```sh
export AERIS_HMAC_KEY="any-non-empty-string"
```

Optional but useful: `openssl` and `base64` on `PATH`.

## Running

```sh
cd demo/07_crypto_pipeline
aeris run ./main.aer
```

The script runs three examples:

- A plain string sealed to stdout.
- A JSON object sealed to `./aeris_sealed.json`.
- A tampered-payload check that confirms `verify` flags HMAC
  mismatches.

## What it shows

- Step chain expressed as plain functions with `?` and `catch`
- `defer io.println(...)` at the top of `run_crypto` runs on every
  exit path (success, `?` propagation, contract violation)
- `shell.exec(["sh", "-c", "..."])` as the v0.3 replacement for v0.1's
  `shell.run(cmd: …)` string form
- `env.must_read("AERIS_HMAC_KEY")` for required secrets
