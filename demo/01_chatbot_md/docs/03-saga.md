# Saga

Una `saga` è un'operazione multi-step in cui ogni `step` ha un blocco
`do` write-effectful e il suo `undo`. Se uno step fallisce, gli step
precedenti vengono compensati in ordine inverso.

## Forma

```aeris
saga rotate(secret: Secret, cap: cap[...]) {
  intent "rotate the production webhook secret"

  step issue {
    do   { http.post("https://vault/rotate", "\{\}")? }
    undo { http.post("https://vault/revoke", "\{\}")? }
  }

  step record {
    requires: issue.ok
    do   { audit.event("secret.rotated",         { id: secret.id }) }
    undo { audit.event("secret.rotation_failed", { id: secret.id }) }
  }
}
```

## Idempotency

Ogni step riceve automaticamente una chiave
`blake3(trace_id || step_name || invocation_index)`. La chiave viene
iniettata come header `Idempotency-Key` su HTTP, come annotation
`aeris.idempotency` su K8s, come `message-id` su AMQP, e come campo
sentinella su Mongo. I retry diventano sicuri.

## Esiti

Solo tre possibili:

- `ok` — tutti gli step completati.
- `rolled_back` — un step fallito, undo completato in ordine inverso.
- `PartialFailure` (exit 74) — un undo ha fallito anche dopo i retry:
  serve intervento umano.

Mai uno stato intermedio silenzioso.
