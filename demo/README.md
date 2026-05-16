# Aeris v0.3 — Demo set

Mini-progetti Aeris autosufficienti (`main.aer` + `aeris.toml`),
pensati per essere mostrati live durante la presentazione.

| # | Scenario | Feature v0.3 evidenziate |
|---|---|---|
| 01 | `01_chatbot_md` | `enforce = "off"` + `loop` + `??` + `ai.chat(system, dir)` + `chat.ask` + `chat.kb_size` + backend CLI Claude headless |

## Come eseguire

Dal root del repo:

```bash
cargo build --release
cd demo/01_chatbot_md
aeris run main.aer
```

`aeris.toml` configura il backend (`[ai.backend] kind = "cli" cmd = "claude --print …"`). Per provare offline, basta cambiare temporaneamente `kind = "mock"` — la risposta diventa un echo deterministico del prompt.

## Anatomia

```aeris
fn main() {
  let chat = ai.chat(
    "Sei un assistente conciso. Rispondi solo dalla knowledge base.",
    "./docs",
  )
  io.println("loaded {chat.kb_size()} files")

  loop {
    io.print("you> ")
    let q = io.read_line() ?? ""
    if q == "" or q == "quit" { break }
    io.println("bot> " + chat.ask(q))
  }
}
```

Niente `cap`, niente `intent`, niente `load_corpus` a mano. Per i
dettagli di ciascuna feature vedi `docs/language.md` §§ 2.3 / 2.6 /
6.1 / 8.4.1 / 22 / 23 e l'Appendice D.

## Promozione a `loose` o `strict`

Per gradi:

```toml
# enforce = "off"   → script mode, niente check
# enforce = "loose" → manifest allow-list enforced (fs.allow_read, ai.models)
[caps]
enforce       = "loose"
fs.allow_read = ["./docs/**"]
ai.models     = ["claude-sonnet-4-6"]
```

In `strict` ogni funzione effettuale deve dichiarare `cap[...]`;
`aeris fmt --narrow-caps` deriva la firma minima dal body.
