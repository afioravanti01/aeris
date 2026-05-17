# 05 — Open City Intelligence

A multi-city comparison dashboard.

For each of the four cities (Rome, Berlin, Barcelona, Delhi) the
script fetches:

- **Weather** from Open-Meteo (current + 3-day forecast).
- **Air quality** from Open-Meteo Air Quality (PM2.5, PM10, NO₂,
  European AQI).
- **Amenity counts** from OpenStreetMap Overpass within a 5 km
  radius (hospitals, schools, restaurants, parks, bike parking, EV
  charging).

Then a three-agent network (orchestrator → urban_analyst →
comparator) scores each city on livability / sustainability /
infrastructure and writes a comparative report. The dashboard
payload is persisted to MinIO and served over HTTP on port 7778.

## Prerequisites

- CLI Claude backend.
- Internet access to `api.open-meteo.com`, `air-quality-api.open-meteo.com`,
  `overpass-api.de`.
- Optional: a real MinIO endpoint (otherwise the runtime stubs apply).

## Running

```sh
cd demo/05_open_city
aeris run ./main.aer
```

When the analysis finishes the script starts the dashboard backend on
`http://localhost:7778` (open it in a browser).

## What it shows

- Multi-source HTTP aggregator with `retry N, delay: D { … }` on every
  external call
- `http.post(url, body, content_type: "application/x-www-form-urlencoded")`
  for the Overpass query
- `ai.network(max_rounds: 8)` with three role-specialised agents
- `string.index_of(...)` + `.slice(start, stop)` to extract the
  ```json fenced score block out of the analyst's reply
- `{ ..cd, scores: … }` structural-update on a record value
