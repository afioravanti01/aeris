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


<style>
  figure.aeris-figure {
    margin: 0.4em auto;
    width: 100%;
    text-align: center;
  }
  figure.aeris-figure svg {
    width: 100%;
    height: auto;
    display: block;
  }
  /* Inline code on divider slides — readable on navy background. */
  section.divider code,
  section.divider p code,
  section.divider blockquote code,
  section.divider li code,
  section.divider h1 code {
    background: rgba(255, 255, 255, 0.14) !important;
    color: var(--cream, #F6F3F0) !important;
    border: 1px solid rgba(255, 255, 255, 0.18) !important;
  }
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
| **II** | Come gira un programma | I quattro livelli del linguaggio, come funziona un linguaggio interpretato, il flusso di esecuzione, la registrazione delle attività, il sistema di moduli, il file di progetto |
| **III** | Le capabilities | Il permesso di compiere effetti esterni come valore di tipo, regole di restringimento, come il linguaggio lega le chiamate ai permessi |
| **IV** | Contratti, intenzioni, schemi | Condizioni prima e dopo una funzione, il blocco `intent` obbligatorio sulle scritture, schemi `model@vN` validati ai confini |
| **V** | Operazioni reversibili e agenti | `saga` con compensazione obbligatoria, chiavi di idempotenza automatiche, agenti AI singoli e in rete |
| **VI** | Regole di runtime, rifiuti, limiti | Le `policy`, le scelte che il linguaggio rifiuta per principio, i limiti onesti del modello |
| **VII** | Ergonomia di v0.3 e un esempio reale | Le novità di v0.3, pattern di automazione tipici, test integrati, un sistema di triage SRE end-to-end, lo stato di sviluppo |

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

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 360" role="img" aria-label="I quattro livelli del linguaggio impilati: sintassi, semantica verificabile, operazioni reversibili, coordinamento di agenti AI">
<defs>
<marker id="arrL" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<rect x="20" y="15" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="45" font-size="22" font-weight="700" fill="#0E1020">Livello 1 — Sintassi</text>
<text x="40" y="72" font-size="16" fill="#5F6470">variabili, tipi, controllo di flusso</text>
<line x1="300" y1="85" x2="300" y2="100" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="100" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="130" font-size="22" font-weight="700" fill="#0E1020">Livello 2 — Semantica verificabile</text>
<text x="40" y="157" font-size="16" fill="#5F6470">capabilities, contratti, intent</text>
<line x1="300" y1="170" x2="300" y2="185" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="185" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="215" font-size="22" font-weight="700" fill="#0E1020">Livello 3 — Operazioni reversibili</text>
<text x="40" y="242" font-size="16" fill="#5F6470">saga con passi do e undo obbligatori</text>
<line x1="300" y1="255" x2="300" y2="270" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrL)"/>
<rect x="20" y="270" width="560" height="70" rx="10" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="40" y="300" font-size="22" font-weight="700" fill="#0E1020">Livello 4 — Coordinamento di agenti AI</text>
<text x="40" y="327" font-size="16" fill="#5F6470">agent, agent_net tipizzato</text>
</g>
</svg>
</figure>

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

```go
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

```go
// let immutabile, var mutabile, const a livello del file.
let titolo      = "report"
var contatore   = 0
const MAX_ITEMS = 100

// Tipi inferiti; annotazioni opzionali ai confini.
let importo: decimal = 12.50

// if è un'espressione.
let etichetta = if importo > 100.0 { "grande" } else { "piccolo" }

// Enumerazioni con varianti tipizzate.
enum Stato {
  Attivo,
  Bannato { motivo: string },
  Sospeso,
}

// match è un'espressione, con destrutturazione e guardie.
let messaggio = match stato_corrente() {
  Bannato { motivo } -> "bloccato: {motivo}",
  Attivo             -> "ok",
  Sospeso            -> "in attesa",
}

// Errori come valori: ? propaga, ?? sostituisce None / Err.
let nick = cerca_soprannome() ?? "anonimo"
let dati = fs.read_file(percorso)?

// Closure di prima classe; metodi sui tipi nativi.
let maiuscoli = ["a", "b"].map(fn(n) { strings.upper(n) })
```

</div>
<div class="column compact">

**Una sintassi familiare**

I legami sono di tre tipi: `let` (immutabile, predefinito), `var` (riassegnabile dentro una funzione), `const` (costante a livello del file). I tipi sono inferiti; le annotazioni si scrivono ai confini di interfaccia.

`if` e `match` sono **espressioni**: il loro valore è quello del ramo scelto. Il `match` accetta letterali, costruttori di enum e record (con destrutturazione dei campi), e guardie introdotte da `if`.

Gli errori sono **valori restituiti**, non eccezioni. Una funzione che può fallire restituisce `result<T>` (cioè `Ok(t)` o `Err(e)`). L'operatore `?` propaga l'errore al chiamante; `??` fornisce un valore di sostituzione su `None` o `Err`.

Le funzioni (incluse le closure) sono valori di prima classe. I tipi nativi `list`, `string`, `map` espongono i metodi comuni: `.map`, `.contains`, `.slice`, `.join`, `.split`, `.trim`.

> La novità del linguaggio è altrove: nei costrutti `cap`, `intent`, `saga`, `agent`, `policy`.

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

<!-- _class: tight -->

# Come gira un programma Aeris dall'interno

<div class="columns">
<div class="column compact">

**Aeris è un linguaggio *interpretato***

Non viene compilato in un binario eseguibile: il programma sorgente viene letto, trasformato in una struttura dati in memoria, e l'eseguibile `aeris` la visita per eseguire il programma. Le quattro fasi sono:

**1. Analisi lessicale (*lexer*).** Legge i byte del file `.aer` e li raggruppa in *token* tipizzati: parole chiave, identificatori, letterali, simboli di punteggiatura. Ogni token porta con sé la posizione nel sorgente. Un carattere non riconosciuto interrompe la lettura.

**2. Analisi sintattica (*parser*).** Prende il flusso di token e costruisce l'**albero di sintassi astratta** (AST, *Abstract Syntax Tree*): un albero in cui ogni nodo rappresenta una costruzione del linguaggio — una `let`, una chiamata, una `saga`, un `agent`. Un programma è la lista dei nodi al livello del file.

**3. Controllo statico.** Una visita sull'AST che verifica le proprietà strutturali del programma: i permessi `cap` dichiarati nelle firme combaciano con le chiamate effettive, i blocchi `intent` sono presenti sulle scritture, gli schemi `model` portano la versione `@vN` sui confini di fiducia. Un errore qui termina prima di eseguire una riga.

**4. Interprete.** Visita l'AST nodo per nodo (*tree walk*) e valuta ogni espressione. Le funzioni sono valori; chiamarle vuol dire creare un ambiente nuovo e visitare ricorsivamente l'albero del loro corpo.

</div>
<div class="column">

```go
// L'interprete è (di fatto) una funzione che cammina
// sull'AST. Una sola funzione ricorsiva visita ogni
// nodo, e per ciascun tipo di nodo decide cosa fare.

fn walk(nodo: Nodo, env: &mut Env) -> Valore {
  match nodo {
    Let(nome, espressione) => {
      let v = walk(espressione, env);
      env.set(nome, v);
    },

    If(condizione, allora, altrimenti) =>
      if walk(condizione, env).is_truthy() {
        walk(allora, env)
      } else {
        walk(altrimenti, env)
      },

    Chiamata(funzione, argomenti) => {
      let valori = argomenti
        .map(|a| walk(a, env));
      applica(funzione, valori, env)
    },

    Blocco(istruzioni) =>
      istruzioni.for_each(|s| walk(s, env)),

    // ...un ramo per ogni costrutto del linguaggio.
  }
}
```

> L'AST **è** il programma. Non c'è una fase di compilazione che produce bytecode, né una rappresentazione intermedia.

</div>
</div>

---

# Dal sorgente alla traccia

<div class="columns">
<div class="column">

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 820 290" role="img" aria-label="Flusso di esecuzione: sorgente, analisi lessicale, analisi sintattica, controllo statico, esecuzione, traccia. Uscita anticipata con codice diverso da zero in caso di errore.">
<defs>
<marker id="arrP" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
<marker id="arrPerr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#D14600"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif">
<g transform="translate(20, 40)">
<rect x="0" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="60" y="38" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">sorgente</text>
<text x="60" y="60" text-anchor="middle" font-size="15" fill="#5F6470" font-family="JetBrains Mono, monospace">.aer</text>
<line x1="121" y1="35" x2="131" y2="35" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrP)"/>
<rect x="132" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="192" y="32" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">analisi</text>
<text x="192" y="56" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">lessicale</text>
<line x1="253" y1="35" x2="263" y2="35" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrP)"/>
<rect x="264" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="324" y="32" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">analisi</text>
<text x="324" y="56" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">sintattica</text>
<line x1="385" y1="35" x2="395" y2="35" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrP)"/>
<rect x="396" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="456" y="32" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">controllo</text>
<text x="456" y="56" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">statico</text>
<line x1="517" y1="35" x2="527" y2="35" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrP)"/>
<rect x="528" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="588" y="38" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">esecuzione</text>
<text x="588" y="60" text-anchor="middle" font-size="14" fill="#5F6470">(interprete)</text>
<line x1="649" y1="35" x2="659" y2="35" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrP)"/>
<rect x="660" y="0" width="120" height="70" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="720" y="38" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">traccia</text>
<text x="720" y="60" text-anchor="middle" font-size="15" fill="#5F6470" font-family="JetBrains Mono, monospace">.jsonl</text>
</g>
<text x="375" y="148" font-size="16" font-style="italic" fill="#D14600">se errore</text>
<path d="M 476 110 L 476 175 L 560 175 L 560 200" fill="none" stroke="#D14600" stroke-width="2" stroke-dasharray="6,4" marker-end="url(#arrPerr)"/>
<path d="M 608 110 L 608 175 L 680 175 L 680 200" fill="none" stroke="#D14600" stroke-width="2" stroke-dasharray="6,4" marker-end="url(#arrPerr)"/>
<rect x="510" y="205" width="220" height="60" rx="8" fill="#FF7E51" stroke="#D14600" stroke-width="2"/>
<text x="620" y="232" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">uscita con</text>
<text x="620" y="256" text-anchor="middle" font-size="19" font-weight="700" fill="#0E1020">codice ≠ 0</text>
</g>
</svg>
</figure>

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

<!-- _class: tight -->

# Il sistema di moduli a tre livelli

<div class="columns">
<div class="column">

```go
// Livello 1 — Libreria standard, compilata dentro al binario.
use io, json, http, fs, shell, audit, strings, date, yaml

// Livello 2 — Gestori nativi per dominio specifico, anch'essi
// dentro al binario, ma con un proprio tipo di permesso.
use ai, kube

// Livello 3 — Librerie esterne, scritte in Aeris, distribuite
// come repository su GitHub con impronta crittografica
// obbligatoria nel file di progetto.
use deploy from "github.com/acmecorp/aeris-devops" deploy@"1.2.0"
use { rollout, status } from deploy        // ri-esportazione selettiva

// Livello 3 (variante locale) — file Aeris sullo stesso disco.
use "./lib/utilities.aer"
use utilities from "./lib/utilities.aer"   // alias di spazio dei nomi
```

</div>
<div class="column compact">

**Tre livelli, una sola parola chiave**

Tutte le importazioni si scrivono con la parola chiave `use`. Il livello viene dedotto dalla forma.

**Livello 1 — Libreria standard.** Sono i moduli "fondamentali" del linguaggio: `io` (terminale), `fs` (file system), `http` (chiamate HTTP), `json` e `yaml` (decodifica/codifica), `strings`, `date`, `shell`, `audit`. Sono compilati dentro al binario `aeris` — l'unico eseguibile statico — e diventano visibili a richiesta tramite `use`.

**Livello 2 — Gestori nativi per dominio specifico.** Anch'essi dentro al binario, ma con un tipo di permesso dedicato e una API più strutturata. In v0.3 sono `ai` (chiamate ai modelli linguistici) e `kube` (operazioni su Kubernetes). I loro effetti vengono registrati nella traccia con campi specifici del dominio.

**Livello 3 — Librerie esterne in Aeris.** Sono file `.aer` scritti dagli utenti e distribuiti tramite GitHub oppure presenti sul file system locale. Ogni libreria esterna deve essere registrata nel `aeris.toml [deps]` con la sua **impronta crittografica** (`blake3:...`): il mismatch fra ciò che si scarica e ciò che è registrato è un errore fatale prima di iniziare l'esecuzione. Non esistono riferimenti mobili (`latest`, `*`, tag Git riassegnabili).

> Le librerie dei livelli 1 e 2 non aggiungono permessi al programma — è il `cap` dichiarato in firma che li abilita. L'`use` rende soltanto i nomi visibili nel file.

</div>
</div>

---
<!-- _class: tight -->

# Il file di progetto come riferimento unico

<div class="columns">
<div class="column">

```toml
[project]
name  = "pipeline-fatture"
aeris = "0.3.0"

# Dipendenze esterne con impronta crittografica.
[deps]
deploy = { source = "github.com/acmecorp/aeris-devops",
           version = "1.2.0", hash = "blake3:..." }

# Permessi consentiti al programma (tetto runtime).
[caps]
enforce         = "strict"
http.allow      = ["api.acme.com"]
fs.allow_write  = ["./out/**"]
ai.models       = ["claude-opus-4-7"]

# Come raggiungere il modello.
[ai.backend]
kind = "http"
url  = "https://api.anthropic.com"
auth = "env:ANTHROPIC_API_KEY"

# Regole di runtime attive nel progetto.
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

# Parte III — Le capabilities

> Il permesso di compiere un effetto esterno è un **valore** passato come parametro. La firma della funzione è il contratto. Non esiste uno spazio dei nomi globale da cui poter chiamare funzioni con effetti; non esistono effetti collaterali nascosti.

---
<!-- _class: tight -->

# Una capability è un valore di tipo `cap`

<div class="columns">
<div class="column">

```go
// Funzione pura: senza il parametro cap non può compiere
// alcun effetto esterno (rete, file, modello).
fn totale(items: list<Fattura@v1>) -> decimal {
  items.map(fn(it) { it.importo }).sum()
}

// Funzione con effetti: l'elenco degli effetti permessi
// è esposto interamente nella firma.
fn chiudi_lotto(
  items: list<Fattura@v1>,
  cap: cap[
    http.post @ ["api.acme.com"],   // solo questo host
    audit.event,                    // qualsiasi evento
  ],
) -> result<unit> {
  intent "chiudi il lotto fatture" {
    for it in items {
      http.post("https://api.acme.com/charge", it)?
    }
    audit.event("chiusura.completata", { conteggio: len(items) })
  }
}
```

</div>
<div class="column compact">

**Cosa significa "capability come valore"**

Per compiere una chiamata che ha un effetto esterno (`http.post`, `fs.write_file`, `ai.complete`, ...) una funzione deve aver ricevuto un parametro di tipo `cap` che concede quel preciso permesso. Senza tale parametro, il compilatore *rifiuta* la chiamata.

Il tipo `cap[...]` elenca le operazioni concesse e i loro vincoli: ogni operazione può avere una lista di valori ammessi (host per HTTP, percorso per il file system, modello per le chiamate AI, bucket, coda di messaggi, e così via).

La firma di `chiudi_lotto` dice **tutto**: questa funzione può fare `http.post` solo verso `api.acme.com` e scrivere eventi di audit; nient'altro. Né lei né le funzioni che chiama possono uscire da questo perimetro.

> È l'idea della *object-capability security*: tenere a vista il diritto di compiere un effetto, sotto forma di valore esplicito, invece di leggerlo da uno spazio dei nomi globale.

</div>
</div>

---

<!-- _class: tight -->

# Restringere la capability quando si passa al chiamato

<div class="columns">
<div class="column">

```go
fn chiudi_lotto(items, cap: cap[
  http.post     @ ["api.acme.com", "api.stripe.com"],
  fs.write_file @ ["./out/**"],
]) {
  // Quando si chiama `addebita` le si passa una capability
  // più stretta: solo http.post verso api.stripe.com.
  // Non si può MAI allargare ciò che si è ricevuto.
  addebita(items, cap.subset[
    http.post @ ["api.stripe.com"]
  ])
}

fn addebita(items, cap: cap[http.post @ ["api.stripe.com"]]) {
  intent "addebita le carte di credito" {
    for it in items { http.post(it.endpoint, it)? }
  }
}
```

</div>
<div class="column compact">

**Quattro regole strutturali**

Sono verificate dal controllore statico, prima che il programma giri.

- L'operatore `cap.subset[...]` accetta solo **restringimenti** della capability ricevuta. Un tentativo di allargarla (chiedere un permesso non posseduto) viene rifiutato.
- Un valore di tipo `cap` non può essere salvato dentro un record, restituito come campo di un altro tipo, inviato attraverso un canale. La sua circolazione resta visibile nella firma delle funzioni.
- La forma "tuttofare" `cap[*]` è proibita nel codice utente: nessuna funzione può chiedere "tutti i permessi possibili".
- Solo `main` riceve la capability iniziale, e la riceve **sintetizzata a partire dal file di progetto**. Non c'è modo di costruirne una da zero altrove.

> Una capability che scende lungo l'albero delle chiamate può solo restringersi. Un attacco che cerca di inserire una chiamata a `evil.com` viene fermato dal compilatore, prima di arrivare in produzione.

</div>
</div>

---

<!-- _class: tight -->

# Le chiamate con effetti sono legate al `cap` ricevuto

<div class="columns">
<div class="column">

```go
use http       // rende il modulo http visibile nel file

// FALLISCE: in questa funzione non esiste alcun cap
// che concede http.get. Il compilatore rifiuta la
// chiamata con un errore di tipo "permesso mancante".
fn stato_servizio() -> int {
  http.get("https://api.acme.com/health")?.status
}

// OK: la firma dichiara il permesso http.get verso
// api.acme.com; la chiamata viene legata a quel cap.
fn stato_servizio_ok(
  cap: cap[http.get @ ["api.acme.com"]]
) -> int {
  http.get("https://api.acme.com/health")?.status
}
```

</div>
<div class="column compact">

**Le chiamate `modulo.operazione(...)` non sono globali**

Quando in un programma compare `http.get(...)`, non si sta invocando una funzione di uno spazio dei nomi globale. Il compilatore risolve la chiamata cercando, fra i parametri visibili nel contesto, un valore di tipo `cap` che concede l'operazione `http.get`. Se non lo trova, la chiamata viene rifiutata.

**Conseguenze pratiche**

- Importare un modulo (`use http`) non introduce alcuna funzione globale `http.post`. L'`use` rende visibile il nome del modulo per scrivere la chiamata, ma il permesso vero arriva sempre dal `cap` ricevuto.
- Aggiungere `use http` in cima al file non abilita nulla: serve un `cap` nella firma della funzione che lo concede.
- Un modello linguistico che genera codice e dimentica di dichiarare il permesso necessario **fallisce al momento del controllo statico**, prima di girare. L'errore viene visto in fase di revisione, non in produzione.

</div>
</div>

---

<!-- _class: tight -->

# La firma delle funzioni `pub` viene congelata in un file di blocco

<div class="columns">
<div class="column">

```toml
# .aeris/surface.lock — generato da `aeris lock surface`,
# controllato in revisione.

[chiudi_lotto]
caps = [
  "http.post @ [\"api.acme.com\"]",
  "audit.event",
]

[totale]
caps = []     # funzione pura, nessun effetto esterno

[stato_servizio_ok]
caps = [
  "http.get @ [\"api.acme.com\"]",
]
```

</div>
<div class="column compact">

**Cosa c'è nel file di blocco**

Per ogni funzione `pub` (esportata) del progetto, il file `.aeris/surface.lock` registra l'elenco esatto dei permessi che la firma dichiara. Il file viene generato automaticamente con `aeris lock surface` e viene messo sotto controllo di versione.

**Cosa significa in revisione**

Quando qualcuno (umano o modello) modifica una funzione esportata aggiungendo un nuovo effetto — per esempio una chiamata di rete dove prima non ce n'erano — il file di blocco va rigenerato, e la differenza appare come **prima modifica** nella richiesta di merge.

- Un allargamento dei permessi richiede di rigenerare il file: il revisore vede il diff come prima cosa.
- Un restringimento non richiede di rigenerarlo (il vecchio insieme è ancora un sovrainsieme di quello nuovo).
- `aeris fmt --narrow-caps` propone in automatico il restringimento minimo della firma in base ai permessi davvero usati dal corpo della funzione.

> Una pull request generata da un modello che aggiunge una chiamata di rete a una funzione finora isolata risulta visibile a colpo d'occhio: la prima cosa che il revisore legge è il cambio del file di blocco.

</div>
</div>

---

<!-- _class: divider -->

# Parte IV — Contratti, intenzioni, schemi

> Tre costrutti per portare il *perché* del codice dentro la grammatica: condizioni di ingresso e di uscita di una funzione, dichiarazione di scopo obbligatoria sulle scritture esterne, schemi versionati validati sui confini di fiducia.

---

# Condizioni di ingresso e di uscita — `requires:` ed `ensures:`

<div class="columns">
<div class="column">

```go
fn paga(
  importo: decimal,
  conto:   string,
  cap:     cap[http.post @ ["api.stripe.com"]],
) -> result<Ricevuta@v1>
  // pre-condizioni: controllate all'ingresso
  requires: importo > 0
  requires: len(conto) == 26

  // post-condizione: controllata su ogni via di uscita
  ensures:  result.ok implies result.value.importo == importo
{
  intent "addebita il cliente" {
    let r = http.post("https://api.stripe.com/v1/charges",
                       { importo, conto })?
    Ok(Ricevuta@v1 { importo, id_transazione: r.id })
  }
}
```

</div>
<div class="column compact">

**Cosa dichiarano `requires:` ed `ensures:`**

`requires:` elenca le pre-condizioni che gli argomenti devono soddisfare *prima* che il corpo della funzione venga eseguito. `ensures:` elenca le post-condizioni che il valore di ritorno deve soddisfare *dopo*, su qualsiasi via di uscita della funzione (ritorno normale, ritorno anticipato, propagazione di errore).

Il binding speciale `result` dentro `ensures:` permette di ragionare sul valore restituito.

**Cosa succede quando una condizione fallisce**

La violazione produce un errore strutturale del tipo `ContractViolation`. Il runtime:

1. registra l'evento nella traccia, incluse le variabili coinvolte;
2. svuota i buffer di scrittura della traccia su disco;
3. termina il programma con codice di uscita 64.

> Una violazione di contratto **non** si propaga con `?` né si recupera con `catch`: è un errore strutturale, non un errore di dominio. Recuperare in silenzio da una violazione di contratto significherebbe vanificare la dichiarazione del contratto stesso.

</div>
</div>

---

# Il blocco `intent` è obbligatorio su ogni scrittura esterna

<div class="columns">
<div class="column">

```rust
// FALLISCE al controllo statico:
// "manca il blocco intent intorno a una scrittura esterna"
fn ruota_certificato(
  cap: cap[fs.write_file @ ["/etc/ssl/**"]],
) -> result<unit> {
  fs.write_file("/etc/ssl/new.pem", nuovo_pem())
}
```

```rust
// CORRETTO: il blocco intent dichiara lo scopo della
// scrittura. La descrizione finisce nella traccia.
fn ruota_certificato(
  cap: cap[fs.write_file @ ["/etc/ssl/**"]],
) -> result<unit> {
  intent "ruota il certificato TLS prima dello scadere a 30 giorni" {
    fs.write_file("/etc/ssl/new.pem", nuovo_pem())
  }
}
```

</div>
<div class="column compact">

**Quali chiamate richiedono un `intent`**

Tutte le operazioni che producono un effetto **scritturale** verso l'esterno: scritture su file system (`fs.write_*`), HTTP modificanti (`http.{post,put,patch,delete}`), apply su Kubernetes (`kube.apply`), eventi di audit (`audit.*`), chiamate al modello (`ai.*`).

Le operazioni di sola lettura (`io.println`, `fs.read_file`, `http.get`) non lo richiedono: possono comparire dentro un blocco `intent` ma non sono obbligate a farlo.

**Cosa finisce nella traccia**

Il runtime emette tre eventi attorno al blocco:

- `intent_enter` all'ingresso: contiene la descrizione testuale, lo scope, il timestamp.
- `intent_exit` all'uscita: contiene l'esito (`ok`, `err`, `partial`) e la durata.
- Ogni evento emesso *dentro* il corpo porta il campo `"intent"` con la stessa descrizione, in modo che si riesca a risalire allo scopo di ogni singola riga della traccia.

> Lo scopo della scrittura diventa parte della grammatica. Una pull request non può aggiungere una scrittura esterna in silenzio: il `intent` deve esistere, e la sua descrizione è leggibile al colpo d'occhio.

</div>
</div>

---

# Schemi versionati — `model X@vN`

<div class="columns">
<div class="column">

```go
model Fattura@v1 {
  id:       uuid
  importo:  decimal where importo > 0
  cliente:  string  where len(cliente) <= 64
  stato:    StatoFattura

  // Vincoli che coinvolgono più campi insieme
  // si scrivono come "where:" a livello del record.
  where: stato == Annullata implies importo == 0
}

// Sui confini di fiducia (richieste HTTP in ingresso,
// decodifica JSON, scambi fra agenti) il tipo DEVE
// portare la sua versione. Usare "Fattura" senza @vN
// è rifiutato dal controllore.
fn ingerisci(raw: string) -> result<Fattura@v1> {
  json.decode<Fattura@v1>(raw)   // validazione automatica al confine
}
```

</div>
<div class="column compact">

**Cos'è un `model@vN`**

È uno schema tipizzato — l'analogo di una `struct` Rust o di un `class` Python — etichettato con un numero di versione obbligatorio. La versione è parte del tipo: `Fattura@v1` e `Fattura@v2` sono due tipi distinti, non convertibili l'uno nell'altro senza una funzione di migrazione esplicita.

**Quando il valore viene validato a runtime**

- alla costruzione (`Fattura@v1 { ... }`);
- alla decodifica da JSON (`json.decode<Fattura@v1>(...)`);
- al passaggio fra agenti (lo schema `accept` / `produce` di ogni agente);
- in ingresso da HTTP o da una coda di messaggi.

**Due forme di vincolo**

- Vincolo per campo: `importo: decimal where importo > 0`.
- Vincolo a livello del record, che mette in relazione più campi: `where: stato == Annullata implies importo == 0`.

> Il versioning **è obbligatorio sui confini di fiducia** del programma. Una bare `Fattura` (senza `@vN`) viene rifiutata dal controllore statico con codice di uscita 68. La migrazione fra versioni è sempre una funzione pura, scritta a mano: niente conversioni implicite, niente sorprese alla decodifica.

</div>
</div>

---

<!-- _class: divider -->

# Parte V — Operazioni reversibili e agenti AI

> Operazioni composte da più passi, dove ogni passo dichiara come **compensare** in caso di errore (`saga`). Agenti AI come unità tipizzate (`agent`). Reti di agenti come grafi aciclici fra unità (`agent_net`).

---

<!-- _class: tight -->

# Una `saga` — passi con `do` e `undo` obbligatori

<div class="columns">
<div class="column">

```go
saga chiudi_lotto(
  lotto: list<Fattura@v1>,
  cap:   cap[
    http.post  @ ["api.acme.com"],
    kube.apply @ ["prod-eu-1"],
    audit.event,
  ],
) {
  intent "chiudi il lotto fatture e avvisa la finanza"

  step addebita {
    do   { for it in lotto { http.post("https://api.acme.com/charge", it)? } }
    undo { for it in lotto { http.post("https://api.acme.com/refund", it)? } }
  }

  step registro {
    requires: addebita.ok
    do   { kube.apply(manifesto_registro(lotto))? }
    undo { kube.delete(manifesto_registro(lotto))? }
  }

  step audit {
    requires: registro.ok
    do   { audit.event("chiusura.completata",    { conteggio: len(lotto) }) }
    undo { audit.event("chiusura.annullata",     { conteggio: len(lotto) }) }
  }
}
```

</div>
<div class="column compact">

**Cosa garantisce una `saga`**

Una `saga` è un'operazione composta da più passi (`step`), dove ogni passo dichiara *come si fa* (`do`) e *come si annulla* (`undo`).

Se uno step intermedio fallisce, il runtime esegue automaticamente gli `undo` dei passi già completati, in ordine inverso. L'effetto è quello di una transazione: o l'intera operazione va a buon fine, oppure il sistema torna allo stato precedente.

**Regole strutturali**

- `do` e `undo` sono **obbligatori** su ogni passo. La forma `undo: noop` è ammessa solo quando il `do` non scrive nulla all'esterno (per esempio se è una pura lettura). Una scrittura senza `undo` viene rifiutata dal controllore statico.
- Gli esiti possibili sono **tre, deterministici**: `ok` (tutto a buon fine), `rolled_back` (rollback completato con successo), `parziale` (uno o più `undo` hanno esaurito i ritentativi). Non esiste un quarto stato "a metà strada".

> Una `saga` rende il rollback un'esigenza grammaticale, non una scelta del programmatore. Una pipeline che scrive senza dichiarare come disfare la scrittura non parsa.

</div>
</div>

---

# Chiavi di idempotenza generate automaticamente dal runtime

<div class="columns">
<div class="column compact">

**Il problema dell'idempotenza**

Quando un'operazione esterna fallisce a metà — chiamata HTTP scaduta, connessione di rete caduta — il runtime non sa se l'effetto sul sistema remoto sia avvenuto o meno. Un semplice ritentativo rischia di duplicare l'effetto: due addebiti per la stessa fattura, due `apply` dello stesso manifesto Kubernetes.

**Come Aeris lo risolve**

Per ogni chiamata di scrittura, il runtime genera **automaticamente** una chiave di idempotenza derivata da:

```text
chiave = blake3(id_traccia ‖ nome_step ‖ indice_invocazione)
```

Cioè un'impronta crittografica che dipende solo dall'identità dell'esecuzione e dal punto del programma. Lo stesso programma rigiocato con `aeris replay` produce le stesse chiavi: il sistema remoto, se le riconosce, scarta i duplicati.

</div>
<div class="column compact">

**Dove viene inserita la chiave**

A seconda del protocollo, il runtime la inserisce nel campo che il sistema remoto si aspetta.

| Operazione | Dove finisce la chiave |
|---|---|
| `http.post`, `http.put`, `http.patch` | Intestazione `Idempotency-Key: <chiave>` |
| `kube.apply` | Annotation `aeris.dev/idempotency-key: <chiave>` |
| `rabbitmq.publish` | Campo `message-id: <chiave>` |
| `mongodb.insert` | Campo di sentinella `_aeris_idem: <chiave>` |

**Conseguenze pratiche**

- Rigiocare uno step già completato non lo esegue di nuovo: il sistema remoto, vedendo la chiave già usata, risponde senza ripetere l'effetto.
- Durante un rollback, i ritentativi sui passi `undo` non duplicano gli annullamenti già applicati.
- La frequenza pratica dello stato "parziale" si riduce molto. Il problema non si elimina del tutto (richiede cooperazione del sistema remoto), ma il linguaggio fa quello che può.

</div>
</div>

---

# Un singolo agente AI come costrutto del linguaggio

<div class="columns">
<div class="column">

```go
model Fattura@v1   { id: uuid, importo: decimal where importo > 0 }
model Categoria@v1 { tipo: string }

agent classifica {
  llm:     "claude-haiku-4-5"
  intent:  "classifica la fattura per tipo di spesa"
  prompt:  """
    Classifica la fattura di importo {input.importo}.
    Restituisci un JSON con un solo campo `tipo`.
  """
  accept:  Fattura@v1
  produce: Categoria@v1
  retries: 2
  budget:  { tokens: 2_000, latency: 3s }
}

fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]])
  -> result<Categoria@v1>
{
  intent "classifica una fattura" {
    classifica(Fattura@v1 { id: uuid_v7(), importo: 99.0 }, cap)
  }
}
```

</div>
<div class="column compact">

**Cosa dichiara un `agent`**

Un `agent` è un'unità di chiamata al modello linguistico promossa a costrutto del linguaggio. Dichiara cinque cose:

- `llm`: il nome del modello da usare;
- `intent`: lo scopo, parte del trace ad ogni invocazione;
- `prompt`: il testo da inviare, con interpolazione dei campi dell'input;
- `accept` e `produce`: gli schemi versionati (`model@vN`) dell'input e dell'output, validati a ogni chiamata;
- `retries:` e `budget:`: vincoli su ritentativi, token e latenza.

**Cosa fa il runtime per ogni invocazione**

- Inietta automaticamente nel prompt di sistema un **contratto di formato** in JSON che descrive lo schema di uscita atteso. Il prompt del modello non lo deve scrivere a mano.
- Valida la risposta contro lo schema `produce`. Se la risposta non corrisponde, ritenta entro il budget `retries:`.
- Se viene sforato il budget di token o latenza, l'agente fallisce con `BudgetExceeded` e codice di uscita 1.
- L'intera invocazione (prompt, risposta, token, esito) viene registrata nel file di traccia per il replay successivo.

</div>
</div>

---

# Una rete di agenti — `agent_net`

<div class="columns">
<div class="column">

```go
agent_net pipeline_fatture {
  intent "estrai → classifica → smista la fattura"

  flow estrai     -> classifica   -> smista
  flow classifica -> { audit, archivio }   // fork sui due rami
  flow audit      -> avvisa_finanza

  // L'iterazione si ferma quando la fiducia del classificatore
  // supera 0.95, o dopo 3 giri.
  until: classifica.confidence > 0.95 or iterations >= 3
}
```

<figure class="aeris-figure">
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 280" role="img" aria-label="Rete di agenti: estrai, classifica, smista, audit, archivio, avvisa finanza. Classifica si dirama su tre rami.">
<defs>
<marker id="arrN" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L10,5 L0,10 Z" fill="#1C2035"/></marker>
</defs>
<g font-family="Inter, system-ui, sans-serif" font-size="20" font-weight="700" fill="#0E1020">
<rect x="20" y="115" width="110" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="75" y="147" text-anchor="middle">estrai</text>
<rect x="190" y="115" width="130" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="255" y="147" text-anchor="middle">classifica</text>
<rect x="380" y="30" width="110" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="435" y="62" text-anchor="middle">smista</text>
<rect x="380" y="115" width="110" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="435" y="147" text-anchor="middle">audit</text>
<rect x="380" y="200" width="110" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="435" y="232" text-anchor="middle">archivio</text>
<rect x="540" y="115" width="170" height="50" rx="8" fill="#F6F3F0" stroke="#1C2035" stroke-width="2"/>
<text x="625" y="147" text-anchor="middle">avvisa_finanza</text>
<line x1="131" y1="140" x2="188" y2="140" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrN)"/>
<path d="M 321 140 L 350 140 L 350 55 L 378 55" fill="none" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrN)"/>
<line x1="321" y1="140" x2="378" y2="140" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrN)"/>
<path d="M 321 140 L 350 140 L 350 225 L 378 225" fill="none" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrN)"/>
<line x1="491" y1="140" x2="538" y2="140" stroke="#1C2035" stroke-width="2.5" marker-end="url(#arrN)"/>
</g>
</svg>
</figure>

</div>
<div class="column compact">

**Cos'è una `agent_net`**

È un grafo di agenti connessi da archi `flow`. Il grafo deve essere **aciclico**: un ciclo dichiarato esplicitamente nel codice viene rifiutato dal controllore con codice di uscita 70. L'iterazione si ottiene con la clausola `until:`, che mette un bound massimo sul numero di giri.

**Come avviene lo smistamento**

Il runtime risolve la diramazione a un nodo a più uscite confrontando lo schema `produce` del nodo di partenza con gli schemi `accept` dei nodi di destinazione: prende il ramo i cui schemi combaciano. Il protocollo di smistamento è parte del **programma**, non una stringa di prompt scritta a mano.

**Composizione**

Una `agent_net` può comparire come nodo di un'altra `agent_net`, permettendo di comporre reti più grandi a partire da reti più piccole. Gli esiti possibili sono `ok(valore)` oppure `Err("agent_net <nome> esaurita")` quando il bound di iterazioni viene raggiunto senza convergenza.

> Lo schema dei messaggi che attraversano la rete è parte della firma. Una hallucination del modello che viola lo schema viene bloccata dal sistema dei tipi, non dal revisore umano.

</div>
</div>

---

<!-- _class: divider -->

# Parte VI — Regole di runtime, rifiuti, limiti

> Le `policy` come costrutto valutato a runtime. Le scelte che il linguaggio rifiuta per principio. I limiti che Aeris dichiara onestamente di non risolvere.

---

# Regole di runtime — `policy`

<div class="columns">
<div class="column">

```go
policy egress_produzione {
  match: http.*
  deny:  url.host not in ["api.acme.com", "api.stripe.com"]
  audit: { url, method }
  when:  env == "production"
}

policy budget_modello {
  match: ai.complete
  limit: tokens_per_minute = 60_000
  audit: { model, tokens }
}
```

</div>
<div class="column compact">

**Cos'è una `policy`**

Una `policy` esprime una regola di sicurezza o di limite operativo come **costrutto del linguaggio**, non come convenzione esterna al codice. È fatta di alcune clausole, tutte facoltative:

- `match:` — su quali chiamate la regola si applica (per esempio `http.*` o `ai.complete`).
- `deny:` — una violazione se la condizione è vera (rifiuta la chiamata).
- `require:` — una violazione se la condizione è falsa.
- `limit:` — quota su una finestra temporale (per minuto, ora, giorno).
- `audit:` — campi aggiuntivi da includere nell'evento di trace per le chiamate che combaciano.
- `when:` — pre-condizione di attivazione (per esempio "solo in produzione").

**Quando una policy è attiva**

Una `policy` può essere attivata in tre modi: dall'`use` di un modulo che la dichiara, con l'attributo `#[policy(nome)]` su una funzione, o dichiarandola nel file di progetto `aeris.toml [policies] active = [...]`.

Ad ogni chiamata che corrisponde a `match:`, il runtime valuta la regola. Una violazione produce un `PolicyViolation`, un evento dedicato nel file di traccia, e un'uscita con codice 1. Se durante una riesecuzione (replay) l'esito della policy diverge dall'esecuzione registrata, il runtime emette un evento `policy_drift` invece di fermarsi.

</div>
</div>

---

# Scelte che il linguaggio rifiuta per principio

| Cosa Aeris non ha | Perché |
|---|---|
| **Nessuna verifica formale tramite SMT solver** | Il verdetto di un solver dipende dalla macchina e dalle euristiche di ricerca, e quindi introduce non-determinismo *negli strumenti di sviluppo*. Sarebbe peggio del non-determinismo che cerchiamo di controllare. |
| **Nessun sistema di "livelli" del codice** (`draft / standard / verified`) | Definire la semantica del confine fra un livello e l'altro è inevitabilmente confuso: cosa succede se `standard` importa `draft`? Nessuna risposta è ergonomica. |
| **Nessuna inferenza automatica dei permessi** | Una modifica interna a una funzione chiamata cambierebbe in silenzio i permessi delle funzioni chiamanti. Il diff in revisione diventerebbe ingannevole: l'ambito degli effetti deve essere dichiarato a mano. |
| **Nessuna parola chiave "morbida"** (con significato dipendente dal contesto) | Un parser che decide il senso di una parola in base ai token successivi (*lookahead* variabile) produce errori sottili: la parola `time` può essere interpretata in modo diverso da quanto si vede a colpo d'occhio. Cercare una parola chiave con `grep` deve restituire *tutte* le occorrenze. |
| **Nessun riferimento mutabile fra le dipendenze** | Niente `latest`, niente `*`, nessun tag Git che possa cambiare contenuto. Ogni `use X@v1.2.3` deve combaciare con il file di blocco oppure fallisce alla risoluzione. |
| **Nessuna libreria binaria caricata dinamicamente** (`.so` o `.dll`) | Un binario su disco aggiungerebbe una superficie di effetti che il controllore statico non può ispezionare. Romperebbe la verificabilità delle capability. |

---

# Limiti dichiarati onestamente

| Limite | Cosa significa concretamente |
|---|---|
| **La prima esecuzione del modello resta non-deterministica** | La registrazione su nastro rende `aeris replay` identico byte per byte *dopo* la prima esecuzione. La prima volta resta in balia del modello: impostare `temperature = 0` riduce la varianza, non la elimina. |
| **La correttezza del corpo di una funzione non viene verificata** | Se una funzione possiede legittimamente il permesso `audit.write` e all'interno scrive l'attore sbagliato, Aeris non se ne accorge. Il linguaggio garantisce visibilità (la firma dichiara cosa la funzione può fare) e obbligo (i blocchi `intent` e `saga` non si possono saltare); la correttezza interna resta affidata a test, code review e controlli di accesso lato sistema (RBAC). |
| **Il rollback a catena è "il meglio che si può"** | Anche l'`undo` di un passo può a sua volta fallire. Il runtime ritenta usando le chiavi di idempotenza. Esauriti i ritentativi emette `PartialFailure` con codice di uscita 74, e chiede risoluzione umana. È un limite noto del pattern SAGA, nessun linguaggio lo elimina davvero. |

> Aeris è la **prima linea di difesa**, non l'unica. Promettere di più sarebbe disonesto con chi davvero deve fidarsi del linguaggio: chi fa compliance, audit, sicurezza.

---

# Le novità di v0.3 — riepilogo delle aggiunte

<div class="columns">
<div class="column compact">

**Più ergonomia su stringhe, controllo, errori**

- Interpolazione `"ciao {nome}"` con `\{` e `\}` come sequenze di escape per le graffe letterali.
- `loop { … }` come abbreviazione di `while true { … }`.
- L'operatore `??` di sostituzione: `Ok(v) → v`, `Some(v) → v`, `Err`/`None` → valore di destra.
- `expr catch err { … }`, `error("...")`, `defer stmt` per il recupero locale dagli errori.
- I blocchi temporali `every D { … }`, `retry N, delay: D { … }`, `timeout D { … }` e la primitiva `clock.sleep`.

**Tipi e moduli**

- `model X@v2 extends X@v1 { … }`: una versione successiva eredita campi e clausole `where:` della versione precedente.
- Istruzioni a livello di file senza la necessità di `fn main` (modalità script).
- Parametri di funzione senza annotazione di tipo, per scripting (`fn f(x, y) { ... }`).
- Funzioni di utilità sui tipi base: `strings.*`, `date.*`, `json.pretty`, `json.parse`, `yaml.parse`.

</div>
<div class="column compact">

**Strumenti per il modello linguistico**

- `ai.session(system, model)` e `ai.session_ask(s, p)`: sessione multi-turno con compattazione automatica della cronologia oltre i quaranta messaggi.
- `ai.decide(p, choices, retries?)`: scelta vincolata fra valori dichiarati.
- `ai.usage()`: contatori di token, costo, numero di chiamate.
- `ai.chat(system, dir)`, con i metodi `.ask(p)` e `.kb_size()`: chatbot costruito su una cartella di documentazione.
- `ai.network(max_rounds)`: rete di agenti costruita programmaticamente.
- Backend di tipo `cli` per `ai.complete`: il modello viene invocato come sottoprocesso a riga di comando, in alternativa alla chiamata HTTP.

**Strumenti per i test**

- Le funzioni `assert_status`, `assert_json` e `assert_semantic` (quest'ultima usa il modello stesso come giudice del contenuto).

</div>
</div>

> La traccia, la riesecuzione, gli schemi `model@vN` e le `policy` restano attivi **su tutta la superficie v0.3**: nessuna scelta di ergonomia rimuove la possibilità di audit.

---

<!-- _class: tight -->

# Recupero locale dagli errori — `catch`, `retry`, `timeout`, `defer`, `every`

<div class="columns">
<div class="column">

```go
// catch — gestore in linea; il blocco fornisce il fallback.
let dati = fs.read_file("config.json") catch err {
  io.eprintln("config mancante: {err.message}"); b"{}"
}

// retry — riesegue su Err, con pausa fra i tentativi.
let r = retry 5, delay: 2s {
  http.get("https://servizio-instabile/health")
}

// timeout — limite sul tempo a parete; non interrompe a metà.
let r = timeout 30s { chiamata_lunga() }

// defer — pulizia in ordine inverso, a ogni uscita.
fn compila(cap: cap[fs.write_file @ ["./build/**"]]) -> result<unit> {
  let tmp = fs.create_temp()?
  defer fs.remove(tmp)
  intent "compila l'artefatto" {
    fs.write_file("./build/out.bin", compila_da(tmp))?; Ok(())
  }
}

// every — ciclo periodico, granularità al secondo.
every 5m {
  let h = http.get("https://api/health")
  if !h.ok { audit.event("api.giu", { ts: clock.now() }) }
}
```

</div>
<div class="column compact">

**Pattern temporali come costrutti del linguaggio**

`catch` è un gestore inserito in linea sull'espressione. Quando il valore è `Err(e)`, l'errore viene legato al nome dichiarato e il blocco di recupero ne fornisce il valore di sostituzione. Si compone con `?`.

`retry N, delay: D` riesegue il blocco se restituisce `Err`, fino a `N` tentativi totali con pausa `D`. Il valore finale è l'esito dell'ultimo tentativo.

`timeout D` misura il tempo a parete (*wall clock*) e produce `Err(err.user("timeout"))` se il blocco supera la soglia. Non interrompe a metà istruzione: il controllo avviene a fine istruzione.

`defer stmt` programma una pulizia che gira al ritorno della funzione, in ordine inverso. Gira su **ogni** via di uscita: ritorno normale, propagazione con `?`, violazione di contratto.

`every D` esegue il blocco, attende `D`, lo riesegue. `break` esce, `continue` salta al prossimo intervallo.

> Tutti e cinque emettono eventi nel file di traccia: tentativi, durate, esiti, `defer` eseguiti.

</div>
</div>

---

<!-- _class: tight -->

# Pattern di automazione tipici

<div class="columns">
<div class="column">

```go
// 1 — Deploy con ritentativo, timeout, e pulizia garantita.
fn deploy(versione: string, cap: cap[shell.exec @ ["kubectl"]]) -> result<unit> {
  let tmp = fs.create_temp()?
  defer fs.remove(tmp)
  intent "deploy della versione {versione} in produzione" {
    let r = retry 3, delay: 5s {
      timeout 60s { shell.exec("kubectl apply -f manifest.yaml") }
    }
    if !r.ok { return Err(error("deploy fallito dopo 3 tentativi")) }
    Ok(())
  }
}

// 2 — Loop di monitoraggio periodico.
every 5m {
  let h = http.get("https://api.acme.com/health")
  if !h.ok {
    audit.event("api.giu", { stato: h.status, ora: clock.now() })
  }
}

// 3 — Rotazione di un segreto con scopo dichiarato.
fn ruota_token(cap: cap[fs.write_file @ ["/etc/secrets/**"], audit.event]) -> result<unit> {
  intent "ruota il token API prima della scadenza a 30 giorni" {
    fs.write_file("/etc/secrets/api.token", genera_token())?
    audit.event("token.ruotato", { ora: clock.now() })
    Ok(())
  }
}
```

</div>
<div class="column compact">

**Tre automazioni operative minime, costruite con i costrutti già visti**

**1. Deploy con compensazione e ritentativi.** Il blocco `intent` dichiara lo scopo della scrittura (obbligatorio in modalità rigida). La combinazione `retry 3, delay: 5s { timeout 60s { ... } }` ripete fino a tre volte la chiamata `kubectl apply`, abbandonando se ogni singolo tentativo supera il minuto. Il `defer` rimuove il file temporaneo su qualsiasi via d'uscita: ritorno normale, errore, propagazione con `?`. Niente *finally* da ricordare; la pulizia è dichiarata accanto alla creazione della risorsa.

**2. Loop di monitoraggio periodico.** L'istruzione `every 5m { ... }` esegue il blocco ogni cinque minuti. Quando il health check fallisce, `audit.event` lascia traccia con il timestamp dell'orologio. Non serve uno scheduler esterno (`cron`, `systemd timer`, Airflow): il loop è parte del programma, e di conseguenza è osservabile nel file di traccia e rigiocabile con `aeris replay`.

**3. Rotazione di un segreto.** L'unica scrittura della funzione è racchiusa in un `intent` con una descrizione che finisce in ogni evento della traccia. Una revisione che modifica la rotazione del segreto vede subito il `intent` come prima cosa.

> Tutti e tre i pattern lasciano traccia rigiocabile, anche quando l'esecuzione fallisce. È la stessa scelta di base che vale per `saga` e `agent_net`: niente automazione "muta" verso il mondo esterno.

</div>
</div>

---

<!-- _class: tight -->

# Le funzioni del modulo `ai`

<div class="columns">
<div class="column">

```go
// Chiamata diretta al modello.
let risposta = ai.complete("Analizza: {log}")

// Scelta vincolata fra valori dichiarati.
let azione = ai.decide(
  prompt:  "CPU al 95%. Cosa fare?",
  choices: ["scala_su", "riavvia", "avvisa", "ignora"],
  retries: 3,
)?

// Conversazione multi-turno (compattazione auto a 40 msg).
let s        = ai.session(
  system: "Sei un assistente per l'affidabilità.",
  model:  "claude-haiku-4-5",
)
let (s2, a)  = ai.session_ask(s,  "Analizza: {log}")
let (s3, b)  = ai.session_ask(s2, "Qual è la causa?")

// Chatbot su una cartella di documentazione.
let chat = ai.chat(
  "Rispondi solo dalla base di conoscenza.",
  "./docs",
)
io.println("{chat.kb_size()} file caricati")
io.println(chat.ask("come funzionano le capability?"))

// Contatori dell'intero processo.
let u = ai.usage()
io.println("speso ${u.cost_usd} in {u.calls} chiamate")
```

</div>
<div class="column compact">

**Sei funzioni per i casi d'uso più frequenti**

- **`ai.complete(prompt)`** — invio diretto al modello. È la primitiva su cui poggiano le altre funzioni.
- **`ai.decide(prompt, choices, retries?)`** — scelta vincolata. La risposta deve cadere in `choices`; in caso contrario il runtime ritenta, poi produce `Err(err.llm(...))`.
- **`ai.session` / `ai.session_ask`** — conversazione multi-turno. Oltre i quaranta messaggi la cronologia viene compattata al riassunto degli ultimi venti.
- **`ai.chat(system, dir)`** — chatbot su una cartella di documenti (markdown, testo, yaml). Restituisce un valore con `.ask(p)` e `.kb_size()`.
- **`ai.usage()`** — contatori di processo: token, costo, chiamate.
- **`ai.network(max_rounds)`** — costruttore programmatico di una rete di agenti.

**Backend configurabile, nessuna libreria collegata**

Il modello si raggiunge in due modi, scelti nel manifesto `aeris.toml [ai.backend]`: chiamata HTTP a un'API compatibile con il protocollo OpenAI, oppure invocazione di un sottoprocesso (`claude --print`, `ollama run`, `llm`).

> Ogni chiamata `ai.*` genera nel file di traccia un evento `ai_call` con prompt, modello, risposta e numero di token. `aeris replay` la rigioca offline.

</div>
</div>

---

<!-- _class: tight -->

# Test integrati nel linguaggio

<div class="columns">
<div class="column">

```go
// In Aeris i test stanno in un qualsiasi file `.aer`: il file
// è l'unità di raggruppamento, non c'è una parola chiave `suite`.

test "addizione commutativa" {
  assert 2 + 3 == 3 + 2
}

test "GET /health restituisce 200" {
  let resp = http.get("https://api.acme.com/health")
  assert_status(resp, 200)
  assert_json(resp.body, ["stato", "versione"])
}

test "il riassunto del modello è fedele al testo" {
  let testo = "I costi del Q3 sono cresciuti del 12%, "
            + "principalmente per il rincaro dell'energia."
  let riassunto = ai.complete("Riassumi in una riga: {testo}")
  assert_semantic(
    riassunto,
    "il riassunto contiene il numero 12% e cita l'energia",
  )
}
```

</div>
<div class="column compact">

**I test sono blocchi di prima classe**

Aeris non integra una libreria di test esterna. Si scrive un blocco `test "nome" { ... }` in un file `.aer` qualunque, e si lancia il file con `aeris test programma.aer`. Ogni blocco gira in isolamento: un fallimento ferma quel test, gli altri proseguono. Non esiste una parola chiave `suite`; l'unità di raggruppamento è **il file** stesso.

**Quattro asserzioni built-in coprono i casi comuni**

- `assert e` — fallisce se l'espressione booleana è falsa. La forma estesa `assert e, "messaggio"` aggiunge contesto al fallimento.
- `assert_status(resp, codice)` — passa solo se la risposta HTTP ha lo `status` indicato.
- `assert_json(testo, [chiavi])` — passa se la stringa si decodifica come JSON valido e contiene tutte le chiavi elencate.
- `assert_semantic(valore, criterio)` — usa il modello configurato come **giudice**: gli chiede se `valore` soddisfa il criterio con risposta binaria, e fallisce sul "no".

> Il modello come strumento di asserzione è ciò che permette di scrivere test *qualitativi* — "il riassunto è fedele all'originale", "il messaggio di errore è chiaro" — quello che una asserzione numerica non riesce a formulare. La chiamata del giudice viene registrata nella traccia come ogni altra chiamata AI, quindi `aeris replay` rigioca anche i test.

</div>
</div>

---

# Tre modalità di disciplina, una sola grammatica

La tesi del linguaggio fissa la disciplina alla sua forma più rigorosa; in pratica, non tutti i progetti vogliono pagarne il costo *da subito*. Il file di progetto può scegliere fra tre modalità di applicazione, tutte basate sulla stessa grammatica.

| Modalità | Predefinita per `aeris init` | Cosa succede |
|---|---|---|
| `enforce = "off"` | **sì** (v0.3) | I permessi vengono sintetizzati come "tutto consentito" e passati a `main`. Il controllore statico ignora le verifiche di capability, `intent` e `saga`; nessun controllo di lista bianca a runtime. |
| `enforce = "loose"` | — | I permessi elencati nel manifesto fanno da limite massimo a runtime. Le funzioni *senza* parametro `cap` restano ammesse; quelle *con* parametro `cap` vengono controllate normalmente dal compilatore. |
| `enforce = "strict"` | — | Disciplina piena: `cap` obbligatorio sulle funzioni con effetti, `intent` obbligatorio sulle scritture, `cap[*]` rifiutato nel codice utente, file di blocco sulle firme `pub` controllato in revisione. |

**Cosa resta attivo in tutte e tre le modalità**

- La scrittura del file di traccia non si può disattivare.
- `aeris replay` produce esecuzioni identiche byte per byte sulla parte deterministica.
- Gli schemi `model@vN` vengono validati a ogni confine di fiducia.
- Le `policy` vengono valutate a runtime su ogni chiamata che combacia con il loro `match:`.

> La modalità di applicazione regola solo il **controllo statico**, non la **registrazione a runtime**. Un progetto può salire dalla modalità `off` alla modalità `strict` *senza riscrivere il codice esistente* — basta aggiungere progressivamente le annotazioni (`cap`, `intent`) là dove vanno.

---

<!-- _class: tight -->

# Esempio completo — triage di un sistema SRE (1 di 2)

<div class="columns">
<div class="column">

```go
// Schemi versionati sui confini di fiducia,
// validati a runtime ad ogni ingresso e uscita.
model Allarme@v1 {
  id:        uuid
  servizio:  string
  messaggio: string
}

model Diagnosi@v1 {
  severita:  string  where ["critica","alta","media","bassa"].contains(severita)
  tipo:      string  where ["database","api","infrastruttura"].contains(tipo)
  fiducia:   f64     where fiducia >= 0.0 and fiducia <= 1.0
}

model PianoFix@v1 {
  comandi:    list<string>
  rollback:   list<string>
  spiegazione: string
}

agent classifica {
  llm:     "claude-haiku-4-5"
  intent:  "classifica l'allarme per severità, tipo, fiducia"
  prompt:  "Classifica: {input.messaggio} sul servizio {input.servizio}."
  accept:  Allarme@v1
  produce: Diagnosi@v1
  retries: 2
  budget:  { tokens: 2_000, latency: 3s }
}

agent pianifica {
  llm:     "claude-opus-4-7"
  intent:  "propone un fix concreto e il rollback corrispondente"
  prompt:  "Fix per allarme {input.severita} di tipo {input.tipo}. Dati: {input}."
  accept:  Diagnosi@v1
  produce: PianoFix@v1
}

agent_net triage {
  flow classifica -> pianifica
  until: classifica.fiducia > 0.85 or iterations >= 3
}
```

</div>
<div class="column compact">

**La parte AI: una rete di agenti tipizzata**

L'intera pipeline di triage degli allarmi è scritta come `agent_net`. Ogni agente dichiara lo schema dell'**ingresso** (`accept`) e dello **uscita** (`produce`) come `model@vN`, validati ad ogni invocazione.

Il passaggio fra agenti è risolto dal runtime per **combaciamento dei tipi**: l'uscita di `classifica` è una `Diagnosi@v1`, e `pianifica` la accetta. Non esiste un prompt-string di coordinamento scritto a mano; il protocollo di scambio è parte del programma.

Il sistema dei tipi taglia le hallucination del modello: una risposta che non rispetta lo schema diventa `Err(err.schema(...))` e l'agente la ritenta entro il budget `retries`.

La clausola `until:` stabilisce un limite massimo alla convergenza: niente loop infiniti. La rete restituisce il valore prodotto dall'ultimo nodo terminale, oppure `Err(err.user("rete agenti esaurita"))` quando il limite di iterazioni viene raggiunto senza che il criterio di convergenza sia soddisfatto.

> Niente stringhe di prompt per smistare i messaggi, niente parsing manuale del JSON di risposta, niente ritentativi cuciti a mano. Tutto questo è infrastruttura del linguaggio.

</div>
</div>

---

<!-- _class: tight -->

# Esempio completo — triage di un sistema SRE (2 di 2)

<div class="columns">
<div class="column">

```go
policy egress_produzione {
  match: http.*
  deny:  url.host not in ["api.acme.com", "slack.com"]
}

saga applica_fix(
  fix:     PianoFix@v1,
  allarme: Allarme@v1,
  cap:     cap[shell.exec @ ["kubectl"], http.post @ ["slack.com"], audit.event],
) {
  intent "applica fix per allarme {allarme.id}"

  step snapshot {
    do   { shell.exec("kubectl get all -n prod > /tmp/{allarme.id}.yaml") }
    undo { shell.exec("rm -f /tmp/{allarme.id}.yaml") }
  }

  step applica {
    requires: snapshot.ok
    do   { for c in fix.comandi  { shell.exec(c)? } }
    undo { for c in fix.rollback { shell.exec(c)? } }
  }

  step notifica {
    requires: applica.ok
    do   { http.post("https://slack.com/hook", { text: "ok: {fix.spiegazione}" })? }
    undo { http.post("https://slack.com/hook", { text: "rollback: {allarme.id}" })? }
  }
}

every 30s {
  let raw     = http.get("https://alertmanager/api/v1/alerts")?
  let allarmi = json.decode<list<Allarme@v1>>(raw.body)?
  for a in allarmi {
    let piano = triage(a)?
    applica_fix(piano, a, cap.subset[shell.exec, http.post, audit.event])?
  }
}
```

</div>
<div class="column compact">

**La parte operativa: `saga`, `policy`, scheduler insieme**

La `saga applica_fix` salva una fotografia del cluster, applica il fix, notifica Slack. Se uno step intermedio fallisce, il runtime esegue gli `undo` dei passi già completati in ordine inverso: niente stato a metà strada.

La `policy egress_produzione` viene valutata su ogni chiamata `http.*`. Una richiesta verso un host fuori lista bianca finisce in `PolicyViolation`, con evento nel file di traccia.

Lo scheduler `every 30s` chiude il giro: scarica gli allarmi, lancia la rete di agenti `triage` per ottenere un `PianoFix@v1`, e invoca la saga con una capability ridotta da `cap.subset[...]` — il principio del *minimo privilegio* applicato per chiamata.

> Un solo file, una sola grammatica. Quello che oggi richiede LangChain, Argo, OPA e script bash messi insieme, qui sta in poco più di cento righe — eseguibile, controllabile in audit, riproducibile offline.

</div>
</div>

---

# Stato dello sviluppo

| Versione | Cosa contiene | Stato |
|---|---|---|
| **v0.1** | Prototipo esplorativo: funzioni di intelligenza artificiale di base, gestori di rete e file, recupero locale dagli errori. **Non** ha ancora capability tipizzate, blocco `intent` obbligatorio, replay deterministico. | precedente |
| **v0.2** | Le fondamenta: lexer, parser, interprete, file di traccia, supporto a `http`, `saga`, file di progetto, `model@vN`. | completata |
| | Aggiunte: funzioni `ai.*`, replay deterministico, `agent_net`, gestori dei livelli più alti, comandi `aeris test` e `aeris fmt`, diagnostica, impacchettamento, modalità prototipo. | completata |
| **v0.3** | Le tre modalità di applicazione (`enforce = "off" / "loose" / "strict"`) e la superficie da scripting (`loop`, `??`, libreria standard più ricca, metodi sui tipi base, codifica naturale di JSON). | completata |
| | Interpolazione delle stringhe `"{x}"`, recupero locale dagli errori (`catch`/`error`/`defer`), costrutti temporali (`every`/`retry`/`timeout`/`clock.sleep`), versioning di schemi con `model X@v2 extends X@v1`. | completata |
| | Funzioni AI di livello più alto (`ai.session`, `ai.decide`, `ai.usage`, `ai.chat(system, dir)`); strumenti di test (`assert_status`, `assert_json`, `assert_semantic`). | parzialmente |
| | Server TCP/HTTP di basso livello (M20) e parità completa con la libreria v0.1 (M22). | rimandate |

> Eseguibile sotto gli 8 MB una volta strippato. Interprete a visita d'albero, nessuna dipendenza esterna a runtime.
> Le voci rimandate richiedono un runtime asincrono o librerie esterne, scelte fuori dallo scopo di v0.3.

---

<!-- _class: divider -->

# Grazie

> Aeris è un progetto aperto. Domande, riscontri e contributi sono benvenuti.
