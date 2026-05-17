# 08 — API Gateway

A small reverse proxy with a fixed-window rate limiter, written in
Aeris and fronted by three Python upstream services.

`routes.yaml` declares:

```yaml
gateway:
  port: 8080
  rate_limit: 100        # requests
  rate_window: 60        # seconds

routes:
  - prefix: /api/users    # → http://localhost:8001
  - prefix: /api/products # → http://localhost:8002
  - prefix: /api/orders   # → http://localhost:8003
```

The gateway:

- Accepts every request on `:8080`.
- Counts requests per fixed `rate_window`; over the limit ⇒ HTTP 429
  with a JSON detail.
- Matches the path prefix and forwards to the upstream (stripping
  `/api`).
- Replies 404 when no route matches, 502 when the upstream is
  unreachable.

## Prerequisites

- [`uv`](https://docs.astral.sh/uv/) for the Python upstreams.
- Each upstream service has a `pyproject.toml`; run `uv sync` in
  `services/users`, `services/products`, `services/orders` once.

## Running

In one terminal — start the upstreams:

```sh
cd demo/08_api_gateway
aeris run ./upstreams.aer
```

In another terminal — start the gateway:

```sh
cd demo/08_api_gateway
aeris run ./main.aer
```

Try it:

```sh
curl http://localhost:8080/health
curl http://localhost:8080/api/users
curl -X POST http://localhost:8080/api/users \
     -H 'content-type: application/json' \
     -d '{"name":"Diana","email":"diana@example.com","role":"user"}'
```

## Tests

`tests/gateway.test.aer` contains eight integration tests
(`assert_status`, plain `assert`, `assert_semantic`). Run them
against the live gateway:

```sh
aeris test ./tests/gateway.test.aer
```

## What it shows

- `yaml.parse_file("routes.yaml")` for declarative configuration
- `net.http(port:)` + per-request `spawn { … }`
- `date.timestamp()` for the rolling window
- `http.{get,post,put,patch,delete}` as a forwarding proxy
- `assert_status` / `assert_json` / `assert_semantic` as v0.3 test
  helpers; `assert_semantic` uses the AI backend as a judge over the
  HTTP response body
