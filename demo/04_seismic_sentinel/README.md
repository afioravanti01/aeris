# 04 — Seismic Sentinel

A periodic global earthquake monitor.

Every 5 minutes the script:

1. Pulls fresh USGS GeoJSON for five regions (Pacific Ring, Americas,
   Europe-Mediterranean, Middle East-Asia, Africa) and tallies events
   above magnitude 4.5 over the last 24 hours.
2. Runs a four-agent `ai.network` (orchestrator → geologist →
   risk_assessor → reporter) and extracts the reporter's final
   summary.
3. Persists the dashboard payload (events + regional counts +
   report) to MinIO as `data.json`.
4. Serves the dashboard over HTTP on port 7777 with a small Leaflet
   frontend (`./web/index.html`).

## Prerequisites

- CLI Claude backend (configured in `aeris.toml`).
- Internet access to `earthquake.usgs.gov`.
- Optional: a real MinIO endpoint on `MINIO_ENDPOINT` — without one,
  the runtime stubs `minio.*` and trace-records every op.

## Running

```sh
cd demo/04_seismic_sentinel
aeris run ./main.aer
```

The script:

- Tries to load cached data from MinIO and starts the dashboard
  immediately if found.
- Always runs one fresh fetch + analysis cycle.
- Then re-runs the cycle every 5 minutes — `Ctrl+C` to stop.

Open `http://localhost:7777` in a browser.

## What it shows

- `every 5m { … }` periodic loop (M18)
- `retry 3, delay: 2s { … }` on `http.get` (M18)
- `ai.network(max_rounds: 10)` programmatic multi-agent builder
- `net.agent(name, system)` + `net.run(entry, message, until)`
- MinIO via `minio.bucket_exists` / `mb` / `put` / `get`
- `clock.sleep(1s)` to let `spawn { server.start(…) }` bind its port
