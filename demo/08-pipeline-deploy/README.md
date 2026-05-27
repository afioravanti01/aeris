# 08 — Pipeline deploy (forward-only automation, real ACR + cluster)

A nine-step deploy expressed as a `pipeline`: delete → pull →
acr-login → tag → push → apply → wait → exec → audit. The
forward-only sibling of the saga in
[`07-docker-deploy`](../07-docker-deploy/), wired against the real
Docker daemon, Azure Container Registry and Kubernetes cluster.

```
aeris run ./main.aer
```

Equivalent CLI sequence:

```
kubectl delete deployment nationalgw-alpine --ignore-not-found        # wipe any previous rollout
docker pull alpine:latest                                             # network reach to Docker Hub
az acr login --name mitramazsplnacr001.azurecr.io                     # ACR auth
docker tag alpine:latest mitramazsplnacr001.azurecr.io/nationalgw/alpine:1.0.0
docker push mitramazsplnacr001.azurecr.io/nationalgw/alpine:1.0.0
kubectl apply -f ./k8s/deployment.yaml
kubectl rollout status deployment/nationalgw-alpine --timeout=60s     # new RS fully available
POD=$(kubectl get pod -l app=nationalgw-alpine --sort-by=.metadata.creationTimestamp -o jsonpath='{.items[-1:].metadata.name}')
kubectl exec "$POD" -- echo Started                                   # in-pod sanity check
```

## Layout

```
.
├── aeris.toml          # real backends for docker + kube
├── main.aer            # the Deploy pipeline
└── k8s/
    └── deployment.yaml # Deployment that pulls the just-pushed image
```

`k8s/deployment.yaml` deploys `mitramazsplnacr001.azurecr.io/nationalgw/alpine:1.0.0`
with `command: ["sleep", "infinity"]` so the pod stays up for the
`exec` step to land on it. If the cluster does not have the ACR
attached via managed identity, uncomment the `imagePullSecrets` block
and create the secret out of band.

## Prerequisites

- `docker`, `kubectl`, `az` and `bash` in `PATH`
- a working kubeconfig pointing at the target cluster
- the cluster can pull from `mitramazsplnacr001.azurecr.io` (attached
  ACR or `imagePullSecrets`)
- network reach to Docker Hub and to the ACR
- `aeris.toml` set to real backends (already configured here)

To run the pipeline offline (trace only, no I/O), flip both backends
back to `"mock"` in `aeris.toml`.

## `saga` vs `pipeline`

Both are ordered, `intent`-gated, `cap`-checked and fully traced. They
differ in one thing: **what happens on failure.**

| | `saga` (07) | `pipeline` (08) |
|---|---|---|
| step shape | `do` / `undo` pair | single forward expression |
| on failure | reverse-order `undo`, else `PartialFailure` | `on_failure` hook, then stop or `continue` |
| recovery model | roll **back** | roll **forward** + re-run |
| `intent` · `cap` · trace · idempotency key | yes | yes |
| compensation guarantee | **yes** | **no** (by design) |

Reach for a `saga` when a failed run must be undone; reach for a
`pipeline` when the recovery is *fix and re-run* (deploys, ops
automation). A `do`/`undo` block inside a pipeline step is a compile
error that points you back at `saga`.

## What to look at

- The single mandatory `intent` wraps every step — the *why* is in the
  grammar, not a comment.
- `docker.pull` / `docker.push` and `kube.apply` go through the typed
  L2 surface. Everything that needs a flag or pipe the L2 doesn't
  surface (`az acr login`, `docker tag`, `kubectl delete --ignore-not-found`,
  `kubectl rollout status`, `kubectl get … -o jsonpath`, `kubectl exec`)
  flows through two thin wrappers:

  ```
  sh(cmd: string) -> result<unit>            # fire-and-forget, fails on non-zero exit
  sh_out(cmd: string) -> result<string>      # captures trimmed stdout
  ```

  Both invoke `bash -c <cmd>` under the hood, so a step body reads as
  the same one-liner you would paste into a shell. The wrappers
  promote a non-zero exit code to an `Err` so a failed CLI step
  actually fails the pipeline (`shell.exec` itself always succeeds as
  long as the process spawns).

- The `exec` step is a **block expression** (`{ … } as pod`). Block
  expressions are first-class in step position — they let a step
  hold a few coordinated lines (capture a pod name, run a command
  inside it) and still expose a single value to later steps via
  `as <binder>`. The block's last expression (`pod`) is the value
  bound to `pod` for the `audit` step.

- `{...}` inside a regular string is interpolation, so the `jsonpath`
  expression is wrapped in a **raw string** `r"..."` to keep
  `{.items[-1:].metadata.name}` literal.

- `kube.apply` annotates the manifest with the step's idempotency key
  before sending it to `kubectl`, so re-running the pipeline against
  an already-applied manifest is a no-op at the apiserver level.

- Each step is taped: `pipeline_enter`, `step_enter` / `step_exit`,
  `pipeline_exit`, plus a per-step idempotency key. The `shell.exec`
  steps additionally emit `shell_exec` events with `argv0`, `exit`
  and stdout/stderr hashes. See the JSONL trace printed at startup
  (`[aeris] trace_id = …`).

- `Deploy.run(image: "alpine:latest")` — pass `on_error: "continue"`
  to roll past a failed step instead of stopping.
