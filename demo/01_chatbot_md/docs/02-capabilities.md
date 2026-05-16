# Capability

Una capability è un valore passato per parametro a una funzione.
Una funzione *senza* parametro `cap` non può fare IO, rete o AI: il
parser rifiuta una `http.post(...)` in un body senza cap.

## Forma

```aeris
fn settle(
  batch: list<Invoice@v1>,
  cap: cap[
    http.post @ ["api.acme.com"],
    audit.event,
  ],
) -> result<unit>
```

La firma elenca *cosa* è raggiungibile e *dove* (allow-list per host,
path, model, ecc.). Inside the body, `http.post(...)` risolve la
chiamata contro il `cap` in scope — non esiste un namespace globale.

## Narrowing

`cap.subset[...]` deriva una cap più stretta da passare a un callee.
Può solo restringere, mai espandere. Un attacco che inietta una
chiamata a un host non in allow-list muore al parser.

## Strict vs prototype mode

- `[caps] required = true` → ogni funzione che chiama una cap op deve
  dichiarare `cap` in firma.
- `[caps] required = false` → la regola di body-resolution è
  rilassata; le allow-list runtime restano vincolanti.
