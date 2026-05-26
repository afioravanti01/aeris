# 07-docker-deploy — deploy saga over typed `docker.*` + AI postmortem

Rolls out two containers one after another with a per-service `saga`
that mirrors the real deploy process — **build → rm → run → gate** —
against a live docker daemon, expressed entirely in typed `docker.*`
capabilities (no shelling out). When one container fails the gate, the
saga compensates (stops the container and removes the image) and
`ai.complete` writes the root-cause postmortem from the stopped
container's real evidence.

```
build  →  docker.build(context, image)         undo: docker.rmi(image)
run    →  docker.rm(name)  (if present)         undo: docker.stop(name)
          docker.run(image, name)  (detached)
verify →  docker.inspect(name)  (deep gate)     undo: audit only
```

`docker.build(ctx, tag)` tags the image; `docker.run(image, name)`
starts a detached, named container (1-arg `docker.run` is the
ephemeral `run --rm`). The failed container is stopped with
`docker.stop`, **not removed** — so `docker ps -a`, `docker.logs`, and
`docker.inspect` still show exactly what happened. The `docker.rm` at
the start of `run` clears whatever a previous run left behind.

## The two services

| Service | Outcome |
|---|---|
| `checkout-web:1.8.0` | production build → passes the gate, left running (live) |
| `payments-api:2.3.1` | builds, runs, logs `status: ok`, but **built with `NODE_ENV=development`** → gate rejects it, container stopped, image removed |

The second failure is deliberately hard to spot. The container
**builds**, **runs**, and logs `{"service":"payments-api","status":"ok"}`
— every shallow signal is green. The defect is baked into the image,
not its behaviour: it was tagged as a release but built with the
development profile. Only the deep `docker.inspect` gate, reading
`Config.Env`, finds `NODE_ENV=development`.

## The `docker.*` surface this uses

The whole lifecycle is typed capabilities, recorded as `docker_*`
trace events and gated by the `docker.*` cap paths:

| Op | Maps to |
|---|---|
| `docker.build(ctx, tag)` | `docker build -t <tag> <ctx>` |
| `docker.rm(name)` | `docker rm -f <name>` (container) |
| `docker.run(image, name)` | `docker run -d --name <name> <image>` |
| `docker.inspect(name)` | `docker inspect <name>` |
| `docker.logs(name)` | `docker logs <name>` |
| `docker.stop(name)` | `docker stop <name>` |
| `docker.rmi(image)` | `docker rmi -f <image>` |

(These ops — `logs` / `stop` / `rm` / `rmi`, plus the 2-arg
`run`/`build` forms — were added to the runtime to make this saga
expressible without `shell.exec`; see `docs/language.md § 23`.)

## Running

Real docker is required (`[l2.docker] backend = "real"` in
`aeris.toml`).

```bash
cd demo/07-docker-deploy
aeris run ./main.aer
```

The saga builds and tags the images itself; the first run pulls
`alpine:3.20`. The AI postmortem needs a configured backend — see
`[ai.backend]` (defaults to the local `claude` CLI, like demos 02/03).

After a run:

```
$ docker ps -a --filter name=checkout-web --filter name=payments-api
checkout-web   Up                 # live
payments-api   Exited (0)         # stopped, inspectable
$ docker logs payments-api
{"service":"payments-api","status":"ok"}
$ docker inspect -f '{{.Config.Env}}' payments-api      # → NODE_ENV=development
```

## What you see (failure path)

```
[deploy] ── payments-api  (payments-api:2.3.1) ──
[deploy/build] → docker.build(./services/payments-api, payments-api:2.3.1)
[deploy/build] ✓ image payments-api:2.3.1 built
[deploy/run] → docker.rm(payments-api)  (remove if present)
[deploy/run] → docker.run(payments-api:2.3.1, payments-api)  (detached)
[deploy/run] ✓ container payments-api started
[deploy/verify] → docker.inspect(payments-api)  (deep gate)
[deploy/verify] ✗ payments-api was NOT built for production (NODE_ENV mismatch)
[deploy/run] ↩ compensation: docker.stop(payments-api) (left stopped for inspection)
[deploy/build] ↩ compensation: docker.rmi(payments-api:2.3.1)

[deploy] payments-api: saga rolled back — container stopped, image removed
[deploy]   reason: saga `deploy_service` rolled back after step `verify`: …
[deploy] payments-api: analysing the failure with AI …
[deploy] payments-api: postmortem written to ./reports/payments-api.md
```

The postmortem (validated as `FailureReport@v1`) lands in
`./reports/<service>.md` and is echoed to the console. It quotes the
real `docker.logs` and `docker.inspect` fields and pins the root cause
to the development profile baked into the release image.

## Trace

The JSONL trace records `docker_build`, `docker_rm`, `docker_run`,
`docker_inspect`, `docker_logs`, `docker_stop`, `docker_rmi`, the
`saga_enter` / `saga_exit` pair, `step_enter` / `step_exit`,
`rollback_enter`, the `undo_enter` / `undo_exit` for the two
compensations, the `audit_event` records, and the `ai_call` for the
postmortem — and **no `shell_exec`**: the deploy is fully typed.

## Layout

- `main.aer` — entry: rollout loop, per-service `catch`, live/rollback banners.
- `lib/saga.aer` — `saga deploy_service(spec)`: build → run → verify, typed `docker.*` with compensations.
- `lib/specs.aer` — the two `ServiceSpec@v1` values + rollout order.
- `lib/models.aer` — `ServiceSpec@v1`, `FailureReport@v1`.
- `lib/report.aer` — `analyse_failure`: pulls `docker.logs` + `docker.inspect`, prompts `ai.complete`, persists.
- `services/*/Dockerfile` — the two demo images (one prod, one mis-built).
