# 06-deploy-rollback — saga + automatic rollback

A three-step `saga` that mimics a Kubernetes deploy and lets you
watch the runtime drive the compensation path when something
goes wrong.

```
apply_config     →  kube.apply(ConfigMap)
apply_deployment →  kube.apply(Deployment)         (requires: apply_config.ok)
health_check     →  http.get(<service>/healthz)    (requires: apply_deployment.ok)
```

Every step pairs `do` with an explicit `undo`. The runtime walks
`undo`s in reverse order the moment any `do` raises — for `apply_*`
that means `kube.delete` of the resource the matching `apply`
created, plus an `audit.event` so the rollback is observable in
`.aeris/audit.jsonl` and in the JSONL trace.

## Running

```bash
cd demo/06-deploy-rollback
aeris run ./main.aer              # happy path
aeris run ./main.aer --fail       # forces the health step to fail
aeris run ./main.aer v=2.1.0      # custom version label
aeris run ./main.aer v=2.1.0 --fail
```

L2 backends default to mock — no real cluster required. To drive
a live `kubectl`, set:

```toml
[l2.kube]
backend    = "real"
kubeconfig = "~/.kube/config"
context    = "kind-aeris"
```

in `aeris.toml`.

## What you see (rollback path)

```
[deploy] target version : 1.0.0
[deploy] mode           : --fail (rollback demo)
[deploy] starting saga …

[deploy/apply_config] → kube.apply(ConfigMap aeris-demo-config v=1.0.0)
[deploy/apply_config] ✓ ConfigMap applied
[deploy/apply_deployment] → kube.apply(Deployment aeris-demo v=1.0.0, replicas=2)
[deploy/apply_deployment] ✓ Deployment applied
[deploy/health_check] ✗ simulated failure (--fail set)
[deploy/apply_deployment] ↩ rolling back Deployment
[deploy/apply_config] ↩ rolling back ConfigMap

[deploy] saga rolled back — propagating error to the caller
[deploy]   reason: saga `deploy` rolled back after step `health_check`
```

The trace JSONL records `kube_apply`, `kube_delete`,
`audit.event`, and a `saga_exit { outcome: "rolled_back" }`
event so the compensation can be reviewed post-mortem with
`aeris trace diff`.

## Layout

- `main.aer` — entry: arg parsing, banner, saga invocation, `catch`.
- `lib/saga.aer` — the three-step `saga deploy(...)`.
- `lib/manifests.aer` — ConfigMap and Deployment YAML templates.
