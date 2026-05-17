---
marp: true
theme: aeris
paginate: true
html: true
size: 16:10
title: "Aeris v0.3"
header: 'Presentazione tecnica · v0.3'
footer: 'Aeris v0.3 · linguaggio interpretato per operazioni, intelligenza artificiale e governance'
---


<script type="module">
  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';
  mermaid.initialize({ startOnLoad: true, theme: 'neutral', securityLevel: 'loose', fontFamily: 'Inter, system-ui, sans-serif' });
</script>
<style>
  .mermaid { background: transparent; margin: 0 auto; }
  .mermaid svg { max-width: 100%; height: auto; }
</style>

<!-- _class: cover -->

<p class="eyebrow">Presentazione tecnica · v0.3</p>

## AERIS v0.3

Linguaggio di programmazione interpretato, pensato per **operazioni di sistema, intelligenza artificiale e automazione**. La sua particolarità è essere scritto *e* letto da LLM (*large language model* — modelli linguistici): l'accesso a rete, file system e modelli è descritto nella firma delle funzioni, ogni scrittura esterna dichiara il proprio scopo, e ogni esecuzione viene registrata in modo da essere riprodotta byte per byte.

---

# Indice della presentazione

| # | Parte | Contenuto |
|---|---|---|
| **I** | Il contesto e il linguaggio | Il problema che Aeris risolve, a chi serve, come si presenta un programma, perché è adatto a essere scritto e letto da un modello linguistico |
| **II** | Come gira un programma | I quattro livelli del linguaggio, il flusso di esecuzione, la registrazione delle attività, il file di progetto |
| **III** | Le capabilities | Il permesso di compiere effetti esterni come valore di tipo, regole di restringimento, come il linguaggio lega le chiamate ai permessi |
| **IV** | Contratti, intenzioni, schemi | Condizioni prima e dopo una funzione, il blocco `intent` obbligatorio sulle scritture, schemi `model@vN` validati ai confini |
| **V** | Operazioni reversibili e agenti | `saga` con compensazione obbligatoria, chiavi di idempotenza automatiche, agenti AI singoli e in rete |
| **VI** | Regole di runtime, rifiuti, limiti | Le `policy`, le scelte che il linguaggio rifiuta per principio, i limiti onesti del modello |
| **VII** | Ergonomia di v0.3 e un esempio reale | Le novità di v0.3, un sistema di triage SRE end-to-end, lo stato di sviluppo |

---

# Il problema da risolvere

Oggi il codice viene generato dai modelli linguistici (LLM) e a sua volta letto da altri modelli per essere modificato o eseguito. Su un programma del genere si sommano tre sorgenti di comportamento imprevedibile, e nessuno strumento tradizionale le copre tutte insieme:

- **Il modello stesso.** A parità di prompt produce uscite diverse. Impostare `temperature = 0` riduce la varianza, non la elimina.
- **La grammatica del linguaggio.** Costrutti ambigui o con più scritture valide costringono il modello a indovinare la forma giusta.
- **Lo stato del mondo esterno.** La rete cade, il database cambia sotto i piedi, il file system viene modificato da altri processi.

Le difese che oggi sono in uso coprono solo una parte del problema:

- I container (Docker, gVisor) controllano cosa il processo *può fare* a livello di sistema operativo, ma non sanno quale singola funzione del programma tocca quale risorsa.
- I sistemi di tipi (TypeScript, Java, Rust) parlano della forma dei dati, non degli effetti che ogni funzione produce.
- I sistemi a effetti accademici (F\*, Liquid Haskell, Koka) offrono il rigore voluto, ma il costo cognitivo è troppo alto per un team aziendale medio.
- I framework di dominio (Airflow, LangChain) impongono convenzioni che il linguaggio non sa far rispettare.

---

# Cos'è Aeris, in concreto

<div class="columns">
<div class="column compact">

**Il prodotto**

- Linguaggio **interpretato**, scritto in Rust.
- Un singolo eseguibile statico chiamato `aeris`, sotto gli 8 MB, senza alcuna dipendenza esterna: si scarica e si esegue.
- I programmi sono file di testo con estensione `.aer`.
- Il modello di esecuzione è un *tree-walking interpreter*: il programma viene letto, trasformato in un albero sintattico, e il runtime visita l'albero nodo per nodo. Niente compilazione, niente codice intermedio.

**Cosa succede a ogni esecuzione**

- `aeris run programma.aer` esegue il programma.
- In parallelo, viene scritto un *file di traccia* nella cartella `.aeris/traces/`. Il formato è JSON Lines: un oggetto JSON per ogni riga, una riga per ogni chiamata significativa.
- `aeris replay <id-traccia>` rigioca offline l'intero programma a partire dalla traccia, ottenendo le stesse identiche risposte sulle parti deterministiche.

</div>
<div class="column">

**Il linguaggio è organizzato in quattro livelli**

<div class="mermaid">
flowchart TB
  L1["<b>Livello 1 — Sintassi</b><br/>variabili, tipi, controllo di flusso"]
  L2["<b>Livello 2 — Semantica verificabile</b><br/>capabilities, contratti, intent"]
  L3["<b>Livello 3 — Operazioni reversibili</b><br/>saga con do / undo obbligatori"]
  L4["<b>Livello 4 — Coordinamento di agenti AI</b><br/>agent, agent_net tipato"]
  L1 --> L2 --> L3 --> L4
  classDef base fill:#F6F3F0,stroke:#1C2035,stroke-width:1px,color:#0E1020;
  class L1,L2,L3,L4 base;
</div>

Ogni livello *si compone* con quello sotto. I livelli si attivano a richiesta: uno script da poche righe vive nel solo livello 1, una pipeline che si ripristina da sola usa i livelli 1, 2 e 3, un sistema multi-agente coordinato li usa tutti e quattro.

</div>
</div>

---

# A chi serve Aeris

<div class="columns">
<div class="column compact">

**Un linguaggio a uso generale**

Aeris è un linguaggio di programmazione *a uso generale*: con Aeris si scrivono programmi da riga di comando, validatori, parser, piccoli servizi e tutte le automazioni che oggi si scriverebbero in Python o in Go. La sintassi è familiare a chi conosce Rust, Swift, Kotlin o TypeScript.

Tre profili trovano nel linguaggio strumenti già pronti, perché Aeris porta dentro la grammatica esattamente le cose che oggi devono essere cucite a mano:

- **Ingegneri delle operazioni** (chi gestisce deploy e infrastruttura), che oggi mettono insieme file YAML, script Python, shell e Terraform.
- **Autori di pipeline di intelligenza artificiale**, che oggi combinano librerie come LangChain con stringhe di prompt scritte a mano e logiche di ritentativo artigianali.
- **Team in contesti regolamentati** — banche, sanità, pubblica amministrazione — che devono sapere con precisione che cosa fa il codice e devono poterlo riprodurre offline a distanza di tempo.

</div>
<div class="column compact">

**Cosa si tiene in un solo file `.aer`**

- Lo script operativo, oggi in `bash` o Python, diventa un programma Aeris in cui ogni scrittura esterna dichiara il proprio scopo con un blocco `intent`.
- Il manifesto della pipeline, oggi in Airflow o Argo, diventa una `saga` con i passi `do` e `undo` obbligatori.
- Il grafo degli agenti AI, oggi in LangChain o CrewAI, diventa una `agent_net` con messaggi validati ad ogni passaggio contro uno schema tipizzato.
- Le regole di sicurezza sulla rete in uscita, oggi in Open Policy Agent (OPA), diventano costrutti `policy` valutati dal runtime ad ogni chiamata.

**Tre modi di scriverlo, una sola grammatica**

- **Modalità script.** Si scrivono istruzioni direttamente nel file, senza `main`, senza dichiarare i permessi. Adatta a prototipi, demo, automazioni veloci.
- **Modalità progressiva.** Le funzioni dichiarano i propri permessi (`cap`), il manifesto del progetto fa da limite massimo. Adatta a chi vuole salire gradualmente verso la disciplina piena.
- **Modalità rigida.** Permessi dichiarati ovunque, blocco `intent` obbligatorio su ogni scrittura, le firme delle funzioni vengono congelate in un file di blocco controllato in revisione. Adatta a produzione, audit, compliance.

> In tutte e tre le modalità il file di traccia e la possibilità di rigiocare l'esecuzione restano attivi: la capacità di rivedere e riprodurre ciò che il programma ha fatto non si può togliere.

</div>
</div>

---

<!-- _class: tight -->

# Come si presenta un programma Aeris

<div class="columns">
<div class="column">

```aeris
// Un programma che legge un file di log, fa classificare
// ogni riga da un modello, e annota nel registro di
// audit le righe segnalate come critiche.
// Senza fn main, senza dichiarazione di permessi.

let sessione = ai.session(
  system: "Classifica la riga come critical, warning o info.",
  model:  "claude-haiku-4-5",
)

let righe = fs.read_file("./error.log")
              .split("\n")

for riga in righe.slice(0, 50) {
  let categoria = ai.decide(
    prompt:  "Classifica: {riga}",
    choices: ["critical", "warning", "info"],
  )?

  if categoria == "critical" {
    audit.event("triage.critical", { riga: riga })
  }
}

io.println("triage completato — {ai.usage().calls} chiamate al modello")
```

</div>
<div class="column compact">

**Cosa rende possibile questa scrittura**

- Le istruzioni vengono scritte direttamente al livello del file e vengono eseguite in ordine, senza bisogno di racchiuderle in una funzione `main`. È lo stesso modello di uno script Python.
- Le stringhe interpolano direttamente le espressioni racchiuse tra graffe: `"{riga}"` al posto della concatenazione manuale.
- Le funzioni della libreria standard accettano argomenti per nome. Una chiamata come `ai.decide(prompt: ..., choices: [...])` si legge come una documentazione di sé stessa.
- Le funzioni di intelligenza artificiale (`ai.session`, `ai.decide`, `ai.usage`) sono nella libreria standard del linguaggio, non in un pacchetto da installare a parte.
- Il punto interrogativo `?` posto dopo una chiamata propaga l'errore al chiamante: nel caso di `ai.decide`, scatta quando il modello restituisce una risposta che non rientra fra quelle elencate in `choices`.
- Ogni chiamata a `ai.decide` e a `audit.event` viene registrata in un file di traccia (in formato JSON Lines, un oggetto JSON per riga) accanto al sorgente. Lo stesso programma si rigioca offline con `aeris replay`, ottenendo gli stessi byte sulle parti deterministiche.

> La grammatica scala senza rotture fino al programma "settle" della Parte III: ci si aggiungono `fn`, `cap`, `intent`, `saga`, senza cambiare linguaggio.

</div>
</div>

---

<!-- _class: tight -->

# I costrutti di base del linguaggio

<div class="columns">
<div class="column">

```aeris
// Tre forme di legame: let immutabile (predefinito),
// var mutabile, const costante a livello del file.
let titolo      = "report"
var contatore   = 0
const MAX_ITEMS = 100

// Tipi inferiti; le annotazioni si scrivono ai confini
// (firme di funzioni esposte, dati in arrivo).
let importo: decimal = 12.50

// if è un'espressione: produce il valore del ramo scelto.
let etichetta = if importo > 100.0 { "grande" } else { "piccolo" }

// Cicli su intervalli, mappe e altre iterabili.
for i in 0..contatore { io.println("{i}: {titolo}") }

// I tipi enumerazione hanno varianti che possono portare
// dati: senza dati, in posizione, o con campi nominali.
enum Stato {
  Attivo,
  Bannato { motivo: string },
  Sospeso,
}

let s = stato_corrente()

// match è un'espressione. I pattern possono estrarre i
// campi nominali di un costruttore; le guardie filtrano
// ulteriormente con una condizione (`if`).
let messaggio = match s {
  Bannato { motivo } -> "bloccato: {motivo}",
  Attivo             -> "ok",
  Sospeso            -> "in attesa",
}

// Gli errori sono valori. ? propaga al chiamante, ??
// sostituisce un'assenza (None) o un errore (Err).
let nick = cerca_soprannome() ?? "anonimo"
let dati = fs.read_file(percorso)?

// Le funzioni sono valori di prima classe (closure).
// I tipi nativi (list, string, map) hanno metodi.
let maiuscoli = ["alice", "bob"].map(fn(n) { strings.upper(n) })
```

</div>
<div class="column compact">

**Quello che ci si aspetta da un linguaggio moderno**

I legami fra nome e valore sono di tre tipi: `let` (immutabile, è il caso normale), `var` (riassegnabile, all'interno di una funzione), `const` (costante a livello del file). I tipi vengono inferiti automaticamente dal compilatore; le annotazioni di tipo restano utili ai confini di interfaccia.

Sia `if` sia `match` sono **espressioni**: il loro valore è il valore del ramo scelto. Si possono usare ovunque sia ammesso un valore, anche a destra di un `let`. Il `match` accetta pattern su letterali, costruttori di enumerazione e record (estraendo direttamente i campi nominali), oltre a guardie introdotte con `if`.

Gli errori sono **valori restituiti**, non eccezioni. Una funzione il cui esito può fallire restituisce un valore di tipo `result<T>` (cioè `Ok(t)` oppure `Err(e)`). L'operatore `?` posto dopo una chiamata propaga l'errore al chiamante; l'operatore `??` fornisce un valore di sostituzione quando l'espressione di sinistra produce `None` o `Err`.

Le funzioni (incluse quelle anonime, le *closure*) sono valori di prima classe. I tipi nativi `list`, `string`, `map` espongono i metodi di uso comune: `.map`, `.contains`, `.slice`, `.join`, `.split`, `.trim` e simili.

> Sintassi familiare a chi conosce Rust, Swift o Kotlin. Tutta la novità del linguaggio è concentrata nei costrutti dei livelli superiori: `cap`, `intent`, `saga`, `agent`, `policy`.

</div>
</div>

---

# Perché è adatto a essere scritto e letto da un modello linguistico

<div class="columns">
<div class="column compact">

**Una grammatica con una sola forma legale**

Ogni parola chiave del linguaggio è riservata: nessun termine cambia significato a seconda del contesto. Una ricerca testuale di `step` o `saga` trova *davvero* tutte le occorrenze. Ogni costrutto ha una sola scrittura canonica, e il formatter `aeris fmt` la impone in modo completo: dato un programma valido, esiste una sola sua forma legale. Non esistono varianti sintattiche per la stessa cosa — si scrive `fn`, mai `function` o `def`.

Il risultato è uno spazio dei completamenti validi molto piccolo. Un modello che genera codice ha meno decisioni da prendere, e quindi meno modi di sbagliare. Una `saga` da dieci passi sta in mezza pagina; una rete di agenti in sei righe.

**Il "perché" è parte della grammatica**

Concetti che in altri linguaggi vivono nei commenti, nei messaggi di commit o nei ticket, in Aeris sono costrutti del linguaggio.

`intent "..."` dichiara lo scopo di una scrittura esterna ed è obbligatorio in modalità rigida. `model Fattura@v1` versiona uno schema dato sui confini di fiducia (chi entra ed esce dal programma) e lo valida a runtime. `policy` esprime una regola di sicurezza come parte del programma, non come convenzione di prompt. Le clausole `requires:` ed `ensures:` portano le pre-condizioni e le post-condizioni dentro la firma della funzione.

</div>
<div class="column compact">

**L'intelligenza artificiale è nella libreria standard**

Le funzioni che oggi vivono in librerie esterne come LangChain qui sono **primitive del linguaggio**:

- `ai.session` e `ai.session_ask` reggono una conversazione multi-turno, con compattazione automatica della cronologia oltre i quaranta messaggi.
- `ai.decide(prompt, choices)` impone al modello di scegliere fra un insieme dichiarato di valori, con ritentativo automatico se la risposta cade fuori.
- `ai.chat(system, dir)` costruisce un chatbot caricando in avvio una cartella di documenti come base di conoscenza.
- `ai.network` programmatico oppure `agent` e `agent_net` dichiarativi descrivono le reti di agenti.
- Il backend del modello è configurabile via file di progetto: chiamata HTTP a un'API (Anthropic, OpenAI) oppure invocazione di un sottoprocesso da riga di comando (`claude --print`, `ollama run`). Nessuna libreria di terze parti collegata in fase di compilazione.

**Riproducibilità inclusa**

Ogni chiamata al modello viene registrata nel file di traccia come un evento JSON che contiene il prompt, il modello, la risposta e il numero di token consumati. Il comando `aeris replay` rigioca l'intera sessione offline ottenendo gli stessi byte sulla parte deterministica. La prima esecuzione resta stocastica per natura; ogni esecuzione successiva di tipo replay è deterministica.

</div>
</div>

---

<!-- _class: divider -->

# Parte II — Come gira un programma

> I quattro livelli del linguaggio, il flusso che porta dal sorgente alla traccia, i codici di uscita previsti, il file di progetto come riferimento unico.

---

# I quattro livelli del linguaggio

| Livello | Cosa aggiunge | Costrutti |
|---|---|---|
| **1** — Sintassi adatta ai modelli | Lessico denso, una sola forma canonica per ogni costrutto, tutte le parole chiave riservate | `fn`, `record`, `enum`, `match`, `if`, `for` |
| **2** — Semantica verificabile | I permessi sugli effetti esterni sono valori passati per parametro, contratti controllati a runtime, blocco `intent` obbligatorio sulle scritture | `cap[...]`, `requires:` / `ensures:`, `intent` |
| **3** — Operazioni reversibili | `saga` con passi obbligati a dichiarare `do` (azione) e `undo` (compensazione); chiavi di idempotenza generate dal runtime | `saga`, `step`, `do`, `undo` |
| **4** — Coordinamento di agenti AI | Agenti come unità tipizzate, grafo aciclico fra agenti, validazione di schema ad ogni passaggio | `agent`, `agent_net`, `flow`, `until:` |

> I livelli **si attivano a richiesta**: un programma usa solo i livelli che servono. Uno script di trenta righe vive nel livello 1; chi sale ai livelli superiori paga il costo solo di ciò che usa.

---

# Dal sorgente alla traccia

<div class="columns">
<div class="column">

<div class="mermaid">
flowchart LR
  A["sorgente<br/><code>.aer</code>"] --> B["analisi<br/>lessicale"]
  B --> C["analisi<br/>sintattica"]
  C --> D["controllo<br/>statico"]
  D --> E["esecuzione<br/>(interprete)"]
  E --> F["traccia<br/><code>.jsonl</code>"]
  D -. "se errore" .-> X(["uscita con<br/>codice ≠ 0"])
  E -. "se errore" .-> X
  classDef step fill:#F6F3F0,stroke:#1C2035,stroke-width:1px,color:#0E1020;
  classDef err fill:#FF7E51,stroke:#D14600,color:#0E1020;
  class A,B,C,D,E,F step;
  class X err;
</div>

L'esecuzione procede in cinque fasi: il **lexer** divide il sorgente in token; il **parser** li ricompone in un albero sintattico; il **controllore statico** verifica le proprietà strutturali (permessi dichiarati, schemi versionati, blocchi `intent` presenti sulle scritture); l'**interprete** visita l'albero nodo per nodo; ogni evento significativo finisce in una riga del **file di traccia**.

</div>
<div class="column compact">

**Codici di uscita previsti**

Ogni categoria di violazione ha un proprio codice di uscita, in modo che i sistemi di integrazione continua (CI) possano reagire in maniera differenziata.

| Codice | Significato |
|---|---|
| `0`  | esecuzione conclusa senza errori |
| `64` | errore di sintassi, di tipo o di contratto |
| `65` | permesso mancante o uso di `cap[*]` nel codice utente |
| `66` | scrittura esterna fuori da un blocco `intent` |
| `67` | passo di `saga` che scrive ma dichiara `undo: noop` |
| `68` | `model` usato senza versione `@vN` su un confine di fiducia |
| `69` | file di blocco scaduto: l'impronta di una dipendenza non corrisponde |
| `70` | ciclo dichiarato in `agent_net` |
| `71` | la firma di una funzione concede più di quanto consente il manifesto |
| `74` | `saga` finita in stato parziale (le retry sugli `undo` sono esaurite) |

</div>
</div>

---

# Registrazione e riesecuzione

<div class="columns">
<div class="column compact">

**La traccia è sempre attiva**

Ogni interazione con il mondo esterno (rete, file system, modelli, orologio, generatore di numeri casuali) finisce su una riga del file di traccia in formato JSON Lines, cioè un oggetto JSON per riga.

- Chiamate al modello (`ai.*`): vengono registrati il prompt, il nome del modello, la risposta, il numero di token e il timestamp.
- Lettura dell'orologio (`clock.now`): viene registrato il valore letto.
- Generazione di un numero casuale (`random.next`): viene registrato il valore generato.
- Chiamate HTTP o invocazioni di shell: vengono registrate le impronte (hash) della richiesta e della risposta.

**Rigiocare un'esecuzione**

Il comando `aeris replay <id-traccia> <sorgente>` rilegge il file di traccia e riproduce l'esecuzione del programma leggendo dal "nastro" registrato invece di interrogare il mondo esterno. Sulla parte deterministica del programma l'esecuzione è **identica byte per byte** all'originale.

L'opzione `--live` lascia che HTTP e modello vengano richiamati realmente, utile per individuare divergenze fra l'esecuzione registrata e una nuova esecuzione (debug differenziale). Il comando `aeris trace diff` confronta due tracce passo per passo e segnala dove divergono.

</div>
<div class="column">

```json
{"event":"intent_enter",
 "intent":"chiudi il lotto fatture",
 "scope":"main.settle",
 "ts":"2026-05-16T08:30:00Z"}

{"event":"ai_call",
 "scope":"classify",
 "prompt":"Classifica la fattura...",
 "model":"claude-opus-4-7",
 "response":"{\"tipo\":\"utenze\"}",
 "tokens":142}

{"event":"http_request",
 "scope":"main.settle.charge",
 "url":"https://api.acme.com/charge",
 "idempotency_key":"blake3:7a3f...",
 "req_hash":"...",
 "resp_hash":"..."}

{"event":"intent_exit","outcome":"ok"}
```

</div>
</div>

---

# Il file di progetto come riferimento unico

<div class="columns">
<div class="column">

```toml
[project]
name  = "pipeline-fatture"
aeris = "0.3.0"

# Dipendenze esterne: nome locale, sorgente, versione,
# impronta crittografica del contenuto.
[deps]
deploy = { source = "github.com/acmecorp/aeris-devops",
           version = "1.2.0",
           hash    = "blake3:..." }

# Permessi consentiti al programma nel suo insieme.
# Modalità rigida: ogni funzione deve dichiarare i propri.
[caps]
enforce         = "strict"
http.allow      = ["api.acme.com"]
fs.allow_write  = ["./out/**"]
ai.models       = ["claude-opus-4-7", "claude-haiku-4-5"]

# Come raggiungere il modello: chiamata HTTP a un'API
# oppure invocazione di un sottoprocesso da riga di comando.
[ai.backend]
kind = "http"
url  = "https://api.anthropic.com"
auth = "env:ANTHROPIC_API_KEY"

# Regole di runtime attive nel progetto (slide più avanti).
[policies]
active = ["egress_produzione"]
```

</div>
<div class="column compact">

**Un solo file `aeris.toml` per quello che oggi è sparso**

Il file di progetto raccoglie in un solo posto informazioni che oggi vivono distribuite fra variabili d'ambiente, file di manifest, file di configurazione e file di lock specifici dei singoli strumenti.

**Contiene quattro cose**

- **Le dipendenze**: ogni libreria esterna è registrata con sorgente, versione e impronta crittografica del contenuto. Se la libreria scaricata non corrisponde all'impronta, il programma fallisce *prima* di iniziare l'esecuzione.
- **I permessi consentiti**: l'elenco di tutto ciò che il programma può fare verso l'esterno (rete, file system, modelli). Costituisce il *tetto* dei permessi che le singole funzioni possono dichiarare.
- **Il backend del modello**: come parlare al modello linguistico (chiamata HTTP a un'API o invocazione di un sottoprocesso da riga di comando).
- **Le `policy` attive**: quali regole di sicurezza vengono applicate a ogni chiamata.

> La funzione `main` riceve i permessi **sintetizzati a partire da questo file**. Non esiste alcun altro modo di costruire un valore di tipo `cap` partendo da zero: i permessi entrano nel programma dall'unico punto controllato in revisione.

</div>
</div>

---

<!-- _class: divider -->

# Sistema di capability

> Capabilities come valori passati per parametro. La signature è il contratto. Niente namespace globale, niente effetti nascosti.

---

# Capability come tipo first-class

<div class="columns">
<div class="column">

```aeris
// Funzione pura: niente parametro cap → niente effetti, mai.
fn total(items: list<Invoice@v1>) -> decimal {
  items.fold(0, fn(acc, it) { acc + it.amount })
}

// Funzione effettuale: l'intera surface è in firma.
fn settle(
  items: list<Invoice@v1>,
  cap: cap[
    http.post @ ["api.acme.com"],
    audit.event,
  ],
) -> result<unit> {
  intent "settle invoice batch" {
    for it in items {
      http.post("https://api.acme.com/charge", it)?
    }
    audit.event("settle.complete", { count: len(items) })
  }
}
```

</div>
<div class="column compact">

- `cap[op @ ["allow-list"]]` — il tipo elenca le operazioni concesse e i loro vincoli
- Allow-list per host, path glob, model, bucket, queue, ...
- Una funzione **senza** parametro `cap` non può fare IO/rete/AI: il parser rifiuta `http.post(...)` se nessun `cap` in scope contiene `http.post`
- Il body resolution lega `<module>.<op>(...)` al `cap` ricevuto — non c'è namespace globale

> Object-capability security applicato agli effetti esterni, non all'aliasing di memoria.

</div>
</div>

---

# Narrowing e regole di escape

<div class="columns">
<div class="column">

```aeris
fn settle(items, cap: cap[
  http.post @ ["api.acme.com", "api.stripe.com"],
  fs.write_file @ ["./out/**"],
]) {
  // Restringo prima di passare a charge:
  // mai broadening, sempre subset.
  charge(items, cap.subset[
    http.post @ ["api.stripe.com"]
  ])
}

fn charge(items, cap: cap[http.post @ ["api.stripe.com"]]) {
  intent "charge cards" {
    for it in items { http.post(it.endpoint, it)? }
  }
}
```

</div>
<div class="column compact">

**Regole strutturali (verificate da M2):**

- `cap.subset[...]` ammette solo restringimenti — broadening rifiutato
- `cap` non può essere salvato in record, ritornato come tipo non-cap, inviato in un `channel`
- `cap[*]` proibito nel codice utente (E65)
- Solo `main` riceve il `cap` sintetizzato dal lockset

> Un cap che scende lungo l'albero delle chiamate può solo restringersi. Un'attacco che inietta una chiamata a `evil.com` muore al parser, non in produzione.

</div>
</div>

---

# Body resolution

`http.post(...)` **non** è una chiamata a un namespace globale. È risolta verso il `cap` in scope.

```aeris
use http                                          // rende il modulo visibile

fn ping_status() -> int {
  http.get("https://api.acme.com/health")?.status // E65: nessun cap in scope
}

fn ping_status_ok(cap: cap[http.get @ ["api.acme.com"]]) -> int {
  http.get("https://api.acme.com/health")?.status // ok
}
```

- Importare un modulo capability-bearing **non** introduce alcuna funzione globale `http.post`
- Aggiungere `use http` in cima al file non abilita niente; serve il `cap` nella firma
- Un LLM che genera codice e dimentica di dichiarare la capability fallisce **al compile time**, non in runtime

---

# Surface lock — V3

Per ogni funzione `pub`, la sua effect surface va in `.aeris/surface.lock`:

```toml
[settle]
caps = [
  "http.post @ [\"api.acme.com\"]",
  "audit.event",
]

[total]
caps = []
```

- Una PR che amplia una surface (aggiunge una sub-cap) **deve** rigenerare il lockfile
- Il diff del `surface.lock` appare come **primo hunk** nella review
- Le contrazioni non richiedono re-lock
- `aeris fmt --narrow-caps` (V1) propone restringimenti basati sull'uso reale del body

> Una PR LLM-generata che aggiunge una chiamata di rete a una funzione prima air-gapped è visibile a colpo d'occhio nella review.

---

<!-- _class: divider -->

# Contracts, intent, model

> Tre costrutti per portare il *perché* dentro la grammatica: pre/post-condizioni, intent obbligatorio sui write, schemi versionati ai trust boundary.

---

# Contracts — `requires:` / `ensures:`

```aeris
fn pay(
  amount: decimal,
  account: string,
  cap: cap[http.post @ ["api.stripe.com"]],
) -> result<Receipt@v1>
  requires: amount > 0
  requires: len(account) == 26
  ensures:  result.ok implies result.value.amount == amount
{
  intent "charge customer" {
    let resp = http.post("https://api.stripe.com/v1/charges",
                          { amount, account })?
    Ok(Receipt@v1 { amount, txn_id: resp.id })
  }
}
```

- Pre-condizioni controllate **all'ingresso**, post-condizioni a **ogni return path**
- Violazione → `ContractViolation`, flush della trace, exit 64
- **Non** catchabile da `?` — è un errore strutturale, non un `Err` di dominio
- Niente SMT, niente proof obligations: tutto runtime, errori actionable

---

# Intent obbligatorio — V2

Ogni chiamata **write-effectful** deve stare dentro un blocco `intent "..."`.

<div class="columns">
<div class="column">

```aeris
// E66 — parse-time error:
// "missing enclosing intent for write-effectful call"
fn rotate_cert(cap: cap[fs.write_file @ ["/etc/ssl/**"]]) {
  fs.write_file("/etc/ssl/new.pem", new_pem())?
}
```

```aeris
// OK — intent dichiara il perché.
fn rotate_cert(cap: cap[fs.write_file @ ["/etc/ssl/**"]]) {
  intent "rotate TLS cert before 30-day expiry" {
    fs.write_file("/etc/ssl/new.pem", new_pem())?
  }
}
```

</div>
<div class="column compact">

**Op write-effectful**: `cap.fs.write*`, `cap.http.{post,put,patch,delete}`, `cap.kube.apply`, `cap.audit.*`, `cap.ai.*`

**Trace events:**

- `intent_enter` — stringa, scope, ts
- `intent_exit` — outcome, durata
- ogni evento dentro il body porta `"intent": "..."` come campo

> Il *perché* diventa antenato grammaticale di ogni effetto. Una PR non può nascondere una write nel silenzio.

</div>
</div>

---

# Model versionato — `@vN`

```aeris
model Invoice@v1 {
  id:       uuid
  amount:   decimal where amount > 0
  customer: string  where len(customer) <= 64
  status:   InvoiceStatus

  where: status == Cancelled implies amount == 0   // invariante cross-field
}

// Bare `Invoice` senza @vN su un trust boundary → exit 68.
fn ingest(raw: string) -> result<Invoice@v1> {
  json.decode<Invoice@v1>(raw)        // validazione automatica al confine
}
```

- Validazione **automatica** sui trust boundary: HTTP ingress, `json.decode`, agent boundary
- `where` per campo (predicato) e `where:` record-level (invariante cross-field)
- Migrazione tra versioni = funzione pura **esplicita**, mai implicita
- Bare `Invoice` rifiutato al check (E68) — versioning forzato dove conta

---

<!-- _class: divider -->

# Saga e agenti

> Operazioni multi-step con compensation obbligatoria. LLM unit tipati. Dataflow aciclico fra agenti.

---

<!-- _class: tight -->

# Saga — anatomia

```aeris
saga settle(
  batch: list<Invoice@v1>,
  cap: cap[
    http.post  @ ["api.acme.com"],
    kube.apply @ ["prod-eu-1"],
    audit.event,
  ],
) {
  intent "settle invoice batch and notify finance"

  step charge {
    do   { for it in batch { http.post("https://api.acme.com/charge", it)? } }
    undo { for it in batch { http.post("https://api.acme.com/refund", it)? } }
  }

  step ledger {
    requires: charge.ok
    do   { kube.apply(ledger_manifest(batch))? }
    undo { kube.delete(ledger_manifest(batch))? }
  }

  step record {
    requires: ledger.ok
    do   { audit.event("settle.complete",    { count: len(batch) }) }
    undo { audit.event("settle.rolled_back", { count: len(batch) }) }
  }
}
```

- `do`/`undo` obbligatori. `undo: noop` ammesso **solo** se il `do` non è write-effectful (E67)
- Esiti deterministici: `ok` | `rolled_back` | `PartialFailure` (exit 74) — mai stato a metà

---

# Idempotency key — N1

Ogni capability di scrittura riceve automaticamente:

```text
key = blake3(trace_id ‖ step_name ‖ invocation_index)
```

| Capability | Iniezione |
|---|---|
| `http.post/put/patch` | Header `Idempotency-Key: <key>` |
| `kube.apply` | Annotation `aeris.dev/idempotency-key: <key>` |
| `rabbitmq.publish` | `message-id: <key>` |
| `mongodb.insert` | Sentinel field `_aeris_idem: <key>` |

- Replay di uno step già completato → **no-op** lato backend
- Cascading undo durante rollback è retry-driven con queste chiavi
- Un retry su un undo già parzialmente applicato non duplica l'effetto

> Riduce la frequenza pratica di `PartialFailure` senza pretendere di eliminarla.

---

# Agent — single LLM unit

<div class="columns">
<div class="column">

```aeris
model Invoice@v1  { id: uuid, amount: decimal where amount > 0 }
model Category@v1 { kind: string }

agent classify {
  llm:     "claude-haiku-4-5"
  intent:  "classify invoice into expense kind"
  prompt:  """
    Classify the invoice with amount {input.amount}.
    Return JSON with a single field `kind`.
  """
  accept:  Invoice@v1
  produce: Category@v1
  retries: 2
  budget:  { tokens: 2_000, latency: 3s }
}

fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]])
  -> result<Category@v1>
{
  intent "classify one invoice" {
    classify(Invoice@v1 { id: uuid_v7(), amount: 99.0 }, cap)
  }
}
```

</div>
<div class="column compact">

- `accept`/`produce` sono **`model@vN`** validati a ogni invocazione
- Routing contract auto-iniettato dal runtime nel system prompt — non scritto a mano
- `retries:` su `SchemaViolation` con backoff
- `budget:` per token e latency; sforamento → `BudgetExceeded`, exit 1
- Ogni invocazione registrata nel trace JSONL (tape recorder N3)

</div>
</div>

---

# agent_net — dataflow tipato

```aeris
agent_net invoice_pipeline {
  flow extract  -> classify  -> route
  flow classify -> { audit, archive }            // fan-out type-driven
  flow audit    -> notify_finance

  until: classify.confidence > 0.95 || iterations >= 3
}
```

- **DAG aciclico** — ciclo dichiarato → E70 al parse
- Routing risolto per **match `accept` ↔ `produce`**: il runtime sa quale ramo prendere dai tipi
- `until:` per loop di convergenza con bound massimo
- Una net può essere usata come **nodo** di un'altra net — composizione gerarchica
- Esiti: `ok(value)` o `Err("agent_net <name> exhausted")`

> Il protocollo di routing diventa parte del *programma*, non una stringa di prompt.

---

<!-- _class: divider -->

# Policy, refusal, limiti

> Policy come costrutto. Le scelte rifiutate per principio. I limiti onesti del modello.

---

# Policy come costrutto

```aeris
policy production_egress {
  match:  http.*
  deny:   url.host not in ["api.acme.com", "api.stripe.com"]
  audit:  { url, method }
  when:   env == "production"
}

policy ai_budget {
  match: ai.complete
  limit: tokens_per_minute = 60_000
  audit: { model, tokens }
}
```

- Attivata via **import del modulo**, **attributo** `#[policy(name)]`, o **lockset** `[policies] active`
- Valutata su ogni chiamata che matcha — non "ricordata" da un system prompt
- Violazione → `PolicyViolation`, trace event, exit 1
- **Drift in replay**: se la policy diverge fra live e replay, evento `policy_drift` nel trace

---

# Cose che il linguaggio rifiuta — e perché

| Refusal | Motivazione tecnica |
|---|---|
| **No SMT / refinement types** | Il verdetto del solver dipende dalla macchina → introduce non-determinismo *nel tooling*, peggio di quello che proviamo a controllare |
| **No tier system** (`draft/standard/verified`) | La semantica del boundary fra tier è inevitabilmente confusa: cosa succede se `standard` importa `draft`? Nessuna risposta ergonomica |
| **No capability inference** | Cambi interni a un callee modificherebbero silenziosamente la surface dei caller; il diff PR diventa ingannevole |
| **No soft keyword** | Lookahead position-dependent produce bug di parsing sottili (`time` token splittato silenziosamente). `grep step` deve trovare ogni `step` |
| **No import di riferimenti mutabili** | Niente `latest`, niente `*`, nessun tag git mutabile. Ogni `use X@vN.M.P` deve combaciare col lockset o fallisce alla risoluzione |
| **No `.so` plugin** | Un binario su disco introdurrebbe una effect surface invisibile al check M2. Romperebbe la verificabilità statica delle capability |

---

# Limiti onesti del modello

| Limite | Cosa significa |
|---|---|
| **Prima esecuzione LLM non-deterministica** | Il tape recorder rende `aeris replay` bit-identical *dopo* la prima run. Quella prima resta in balìa del modello: `temperature=0` riduce la varianza, non la elimina |
| **Logica dentro capability legittima non verificata** | Se una funzione tiene legittimamente `cap[audit.write]` e scrive l'attore sbagliato, Aeris non se ne accorge. Garantiamo visibilità (firma) e obbligo (intent, saga); la correttezza del body è responsabilità di test, review, RBAC |
| **Cascading undo best-effort** | L'`undo` di uno step può a sua volta fallire. Ritentiamo con idempotency key di N1; dopo retry, evento `PartialFailure` ed exit 74 → richiesta risoluzione umana. Limite noto del pattern SAGA, nessun linguaggio lo elimina |

> Aeris è la **prima linea difensiva**, non l'unica. Promettere di più sarebbe disonesto con l'audience che conta (compliance, audit, security).

---

# v0.3 — superficie ergonomica

<div class="columns">
<div class="column compact">

**Stringhe, controllo, errori**

- Interpolazione `"hi {name}"` con `\{` / `\}` escapes (M16)
- `loop { … }` sugar per `while true` (M24)
- `??` null-coalesce: `Ok/Some/value → v`, `Err/None/() → rhs` (M24)
- `expr catch err { … }`, `error("...")`, `defer stmt` (M17)
- `every 5s { }`, `retry 3, delay: 1s { }`, `timeout 30s { }`, `clock.sleep` (M18)

**Tipi e moduli**

- `model X@v2 extends X@v1 { … }` (M23) — fields + `where:` ereditati
- Top-level statements senza `main` (M26)
- Parametri non tipati per script (`fn f(x, y)`) (v0.3)
- `strings.*`, `date.*`, `json.pretty/parse`, `yaml.parse` (M24)

</div>
<div class="column compact">

**Toolkit AI**

- `ai.session(system, model)` + `ai.session_ask(s, p)` con auto-compaction 40→20 (M19)
- `ai.decide(p, choices, retries?)` enum-style (M19)
- `ai.usage() → { total_tokens, cost_usd, calls }` (M19)
- `ai.chat(system, dir)` + `chat.ask` + `chat.kb_size` (M19.T6)
- `ai.network(max_rounds)` builder programmatico (M28)
- Backend `cli` per `ai.complete` (M9.T1) — subprocess spawn

**Test helpers**

- `assert_status`, `assert_json`, `assert_semantic` (M21) — l'ultimo usa il backend AI come giudice

</div>
</div>

> Trace, replay, `model@vN` e `policy` restano attivi sopra **tutta** la superficie v0.3: nessuna ergonomia toglie l'audit.

---

<!-- _class: tight -->

# Resilienza inline — `catch`, `retry`, `timeout`, `defer`, `every`

<div class="columns">
<div class="column">

```aeris
// catch — gestore inline; il valore del blocco è il fallback.
let bytes = fs.read_file("config.json") catch err {
  io.eprintln("config mancante: {err.message}")
  b"{}"
}

// retry — riesegue il blocco su Err, con pausa configurabile.
let resp = retry 5, delay: 2s {
  http.get("https://unstable/health")
}

// timeout — bound sul wall-clock; non interrompe a metà.
let r = timeout 30s {
  long_running_call()
}

// defer — pulizia LIFO su ogni via d'uscita.
fn build(cap: cap[fs.write_file @ ["./build/**"]]) -> result<unit> {
  let tmp = fs.create_temp()?
  defer fs.remove(tmp)
  intent "compila l'artefatto in {tmp}" {
    fs.write_file("./build/out.bin", compile(tmp))?
    Ok(())
  }
}

// every — ciclo periodico, granularità al secondo.
every 5m {
  let h = http.get("https://api/health")
  if !h.ok { audit.event("api.down", { ts: clock.now() }) }
}
```

</div>
<div class="column compact">

**Pattern temporali ricorrenti, come costrutti del linguaggio**

`catch` è un gestore inline. Il valore del blocco di recupero diventa il valore dell'espressione. Si compone con `?`: il blocco di recupero stesso può propagare un errore.

`retry N, delay: D` riesegue il blocco se restituisce `Err`, fino a `N` tentativi totali, con pausa `D` tra l'uno e l'altro. Il valore dell'espressione è il risultato dell'ultimo tentativo, riuscito o meno.

`timeout D` misura il wall-clock alla fine di ogni cancel-point. Non interrompe il blocco a metà istruzione — il runtime tree-walk è onesto sui suoi limiti, e l'esito è un `Err(err.user("timeout"))` propagabile.

`defer stmt` registra una pulizia che gira al ritorno della funzione, in ordine LIFO. Gira su **ogni** via d'uscita: ritorno normale, propagazione con `?`, fallimento di contratto. È il posto giusto per chiudere file, rimuovere temporanei, rilasciare lock.

`every D` rientra nel blocco ogni `D` dopo la fine dell'iterazione precedente. `break` esce, `continue` salta al prossimo tick. La prima iterazione parte subito.

> Tutti e cinque emettono eventi nel trace JSONL — tentativi, durate, esiti, ogni `defer` eseguito. Il debugging non richiede `println` aggiuntivi.

</div>
</div>

---

<!-- _class: tight -->

# Inventario AI built-in

<div class="columns">
<div class="column">

```aeris
// Chiamata diretta — input string, output string.
let answer = ai.complete("Analizza: {log}")

// Scelta vincolata — la risposta deve cadere in choices,
// altrimenti retry e infine Err(err.llm(...)) propagabile.
let action = ai.decide(
  prompt:  "CPU al 95%. Cosa fare?",
  choices: ["scale_up", "restart", "alert", "noop"],
  retries: 3,
)?

// Conversazione multi-turno con compaction automatica 40→20.
let s         = ai.session(
  system: "Sei un assistente SRE.",
  model:  "claude-haiku-4-5",
)
let (s2, a)   = ai.session_ask(s,  "Analizza: {log}")
let (s3, b)   = ai.session_ask(s2, "Qual è la causa principale?")

// Chatbot su una cartella di documentazione.
let chat = ai.chat(
  "Rispondi solo dalla knowledge base.",
  "./docs",
)
io.println("kb: {chat.kb_size()} file")
io.println(chat.ask("come funzionano le capability?"))

// Contatori di processo.
let u = ai.usage()
io.println("speso: ${u.cost_usd} su {u.calls} chiamate")
```

</div>
<div class="column compact">

**Sei builtin per i casi d'uso più frequenti**

- **`ai.complete(prompt)`** — chiamata diretta al backend del modello. È la primitiva su cui poggiano gli altri builtin.
- **`ai.decide(prompt, choices, retries?)`** — scelta vincolata. La risposta deve essere uno degli elementi di `choices`; il runtime ritenta automaticamente fino a `retries` volte, poi produce un `Err(err.llm(...))` propagabile con `?`.
- **`ai.session` / `ai.session_ask`** — conversazione multi-turno. La sessione accumula la cronologia; quando supera quaranta messaggi viene compattata al riassunto degli ultimi venti.
- **`ai.chat(system, dir)`** — chatbot su una cartella di markdown, testo o yaml. La knowledge base viene caricata in startup come parte del prompt di sistema. Il valore restituito espone `.ask(prompt)` e `.kb_size()`.
- **`ai.usage()`** — contatori di processo: token totali, costo accumulato in dollari, numero di chiamate.
- **`ai.network(max_rounds)`** — builder programmatico di una rete di agenti con hand-off testuale (sibling più leggero di `agent_net`).

**Backend pluggable, niente SDK linkati**

Il backend del modello viene scelto dal manifest `aeris.toml [ai.backend]`: una chiamata HTTP verso un'API OpenAI-compatible (Anthropic, OpenAI), oppure un subprocess CLI (`claude --print`, `ollama run`, `llm`).

> Ogni chiamata `ai.*` emette un evento `ai_call` nel trace JSONL con prompt, modello, risposta e numero di token. `aeris replay` rigioca offline sulla traccia.

</div>
</div>

---

# Rilassamento controllato del non-determinismo

La tesi v0.2 fissa la disciplina; la pratica v0.3 ammette che non tutti i progetti vogliono pagarla *da subito*. Tre modalità, una sola sorgente di verità.

| Modalità | Default `aeris init` | Effetto |
|---|---|---|
| `enforce = "off"` | **sì** (v0.3) | `cap[*]` sintetizzato a `main`; niente E65/E66/E67/E71; niente allow-list runtime |
| `enforce = "loose"` | — | manifest cap come ceiling runtime; le fn senza `cap` restano ammesse; le fn con `cap` sono check-ate normalmente |
| `enforce = "strict"` | — | piena disciplina v0.2 (`intent` obbligatorio, `cap[*]` rifiutato nel codice utente, surface lock) |

**Cosa NON cambia mai:**

- Trace JSONL sempre attivo
- `aeris replay` bit-identical sul subset deterministico
- Validazione `model@vN` ai trust boundary
- `policy` valutata su ogni chiamata che matcha

> Il rilassamento è solo del **vincolo statico**, non della **registrazione runtime**. Un progetto può salire la ladder (`off` → `loose` → `strict`) senza riscrivere codice — solo aggiungendo annotazioni.

---

<!-- _class: tight -->

# Esempio end-to-end — triage SRE (1/2)

<div class="columns">
<div class="column">

```aeris
// Schemi versionati ai trust boundary, validati a runtime.
model Alert@v1 {
  id:      uuid
  service: string
  message: string
}

model Diagnosis@v1 {
  severity:   string  where ["critical","high","medium","low"].contains(severity)
  kind:       string  where ["database","api","infra"].contains(kind)
  confidence: f64     where confidence >= 0.0 and confidence <= 1.0
}

model FixPlan@v1 {
  commands:  list<string>
  rollback:  list<string>
  rationale: string
}

agent classify {
  llm:     "claude-haiku-4-5"
  intent:  "classifica l'alert per severità, tipo, fiducia"
  prompt:  "Classifica: {input.message} su {input.service}."
  accept:  Alert@v1
  produce: Diagnosis@v1
  retries: 2
  budget:  { tokens: 2_000, latency: 3s }
}

agent plan {
  llm:     "claude-opus-4-7"
  intent:  "propone un fix concreto e il suo rollback"
  prompt:  "Fix per {input.severity} {input.kind}. Dati: {input}."
  accept:  Diagnosis@v1
  produce: FixPlan@v1
}

agent_net triage {
  flow classify -> plan
  until: classify.confidence > 0.85 or iterations >= 3
}
```

</div>
<div class="column compact">

**Una rete di agenti tipata**

L'intera pipeline AI di triage è una `agent_net`. Ogni agente dichiara il suo schema di **input** (`accept`) e di **output** (`produce`) come `model@vN`, validati a runtime su ogni passaggio.

Il routing tra agenti è risolto dal runtime per **match dei tipi**: l'output di `classify` è un `Diagnosis@v1`, e `plan` lo accetta — non c'è prompt-string di coordinamento. Il protocollo di scambio è parte del programma.

Il sistema dei tipi taglia le hallucination del modello: una risposta che non rispetta lo schema diventa `Err(err.schema(...))` e l'agente la ritenta entro il budget `retries`.

`until:` mette un bound sulla convergenza: niente loop infiniti. La rete restituisce il valore dell'ultimo nodo terminale, oppure `Err(err.user("agent_net exhausted"))` allo scadere delle iterazioni.

> Niente prompt-string per il routing, niente JSON parsing manuale, niente retry casalingo. Tutto è infrastruttura del linguaggio.

</div>
</div>

---

<!-- _class: tight -->

# Esempio end-to-end — triage SRE (2/2)

<div class="columns">
<div class="column">

```aeris
policy production_egress {
  match: http.*
  deny:  url.host not in ["api.acme.com", "slack.com"]
  audit: { url, method }
}

saga apply_fix(
  fix:   FixPlan@v1,
  alert: Alert@v1,
  cap:   cap[
    shell.run @ ["kubectl"],
    http.post @ ["slack.com"],
    audit.event,
  ],
) {
  intent "applica il fix per l'alert {alert.id} ({alert.service})"

  step snapshot {
    do   { shell.run("kubectl get all -n prod -o yaml > /tmp/{alert.id}.yaml") }
    undo { shell.run("rm -f /tmp/{alert.id}.yaml") }
  }

  step apply {
    requires: snapshot.ok
    do   { for cmd in fix.commands { shell.run(cmd)? } }
    undo { for cmd in fix.rollback { shell.run(cmd)? } }
  }

  step notify {
    requires: apply.ok
    do   { http.post("https://slack.com/hook", { text: "fix ok: {fix.rationale}" })? }
    undo { http.post("https://slack.com/hook", { text: "rollback: {alert.id}" })? }
  }
}

every 30s {
  let raw   = http.get("https://alertmanager/api/v1/alerts")?
  let items = json.decode<list<Alert@v1>>(raw.body)?
  for it in items {
    let plan = triage(it)?
    apply_fix(plan, it, cap.subset[shell.run, http.post, audit.event])?
  }
}
```

</div>
<div class="column compact">

**Lato ops: saga, policy, scheduler — tutto nello stesso file**

La `saga apply_fix` esegue lo snapshot del cluster, applica il fix, notifica Slack. Ogni `step` dichiara `do` e `undo`. Se uno step intermedio fallisce, il runtime esegue gli `undo` degli step già completati in ordine inverso — niente stato a metà strada.

La `policy production_egress` viene valutata su ogni chiamata `http.*`. Una richiesta verso un host non autorizzato finisce in `PolicyViolation`, con evento dedicato nel trace. Una review della PR vede subito chi ha provato a violarla.

Lo scheduler `every 30s` chiude il loop: scarica gli alert da Alertmanager, lancia la rete di agenti `triage` per ottenere un `FixPlan@v1`, e invoca la saga con una capability ristretta da `cap.subset[...]` — il principio del minimo privilegio applicato per chiamata.

> Un solo file `.aer`, una sola sintassi. Quello che oggi richiede LangChain + Argo + OPA + script bash, qui sta in un centinaio di righe. Ed è eseguibile, audit-friendly, replayabile.

</div>
</div>

---

# Stato dell'implementazione

| Tag | Milestone | Stato |
|---|---|---|
| **v0.1** | Prototipo esplorativo: AI builtins, L2 handlers, network listeners, inline errors — senza cap typing, intent, replay | legacy |
| **v0.2** | M0–M8 bootstrap → policy (parser, check, interp, trace, http, saga, manifest, model) | done |
| | M9–M15 AI + replay, agent_net, L2 handlers, test/fmt, diagnostics, packaging, prototype mode | done |
| **v0.3** | M15B `enforce = off \| loose \| strict` · M24 script surface (`loop`, `??`, `strings.*`, methods, natural JSON) | done |
| | M16 interpolazione `{x}` · M17 `catch`/`error`/`defer` · M18 `every`/`retry`/`timeout`/`clock.sleep` · M23 `model extends` | done |
| | M19 AI builtins (`session`, `decide`, `usage`, `ai.chat(dir:)`+`chat.ask`+`kb_size`) · M21 `assert_status`/`json`/`semantic` | partial |
| | M20 network listeners · M22 parità L2 estesa con v0.1 | deferred |

> Binario v0.2: < 8 MB stripped · tree-walk interpreter · zero dipendenze runtime.
> M20/M22 rinviati: richiedono runtime async / SDK esterni fuori dallo scope di v0.3.
