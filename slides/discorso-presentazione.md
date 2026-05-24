# Discorso di accompagnamento — Aeris v0.3

Per ogni slide: cosa dire al pubblico, in italiano parlato. I nomi dei costrutti (`cap`, `intent`, `saga`, …) restano in inglese perché sono identificatori del linguaggio. Il discorso non duplica i bullet — li commenta e li traduce in voce.

Stima dei tempi: in media 45–60 secondi per slide, ~40–45 minuti per il deck intero. Slide-divisori e Q&A non contati.

---

## Slide 1 — Cover

Apertura veloce. *"Buongiorno, oggi vi presento Aeris, un linguaggio di programmazione interpretato che abbiamo costruito per uno scopo molto preciso: scrivere automation, orchestrazione di modelli linguistici, operazioni di sistema e governance, all'interno di una grammatica sola."* Anticipa che capabilities, intent e saga sono i costrutti distintivi — e che li vedremo uno per uno.

---

## Slide 2 — Agenda

Presenta le otto sezioni in modo lineare, senza soffermarti. *"Prima vediamo cosa è e a cosa serve. Poi entriamo dentro al runtime, scopriamo come è organizzato il linguaggio in livelli. Poi tre sezioni grandi — Core language, AI primitives, Verifiability — sui costrutti veri e propri. Chiudiamo con Governance & reasoning, dove tiriamo le fila teoriche, e un esempio end-to-end di triage SRE che mette tutto insieme."* Serve per dare un'aspettativa: chi ascolta sa dove sta andando e quanto manca.

---

## Slide 3 — Aeris at a glance

*"Aeris è un linguaggio interpretato general-purpose scritto in Rust. Ci tengo a dire general-purpose: con Aeris si scrivono CLI, parser, validatori, qualsiasi cosa scriviate in Python o Go. Il dominio in cui pensiamo Aeris ecceda — operations, AI, governance — è dove abbiamo concentrato la sintassi dedicata, non un limite."* Punta sui quattro pilastri:

1. **Runtime**: binario unico, sotto gli 8 MB, niente dipendenze esterne. Lo scarichi, lo esegui.
2. **Libraries**: tre livelli organizzativi — stdlib built-in, gestori nativi per dominio, librerie esterne pinned per hash.
3. **LLM integration**: il backend si configura, non si compila. Può essere un'API HTTP o un processo CLI locale.
4. **Cosa rimpiazza in un solo file**: questa è la cosa più impattante per chi viene dal mondo ops. Oggi un sistema completo è bash + Python + Terraform + Airflow + LangChain + OPA; in Aeris è un file `.aer`.

Chiudi con la frase *"opt-in by depth"*: il programma paga il costo solo dei layer che usa.

---

## Slide 4 — Hello world

Due esempi affiancati. *"A sinistra script mode, tre righe — `use io`, una chiamata, fatto. Non c'è `main`, l'istruzione gira al volo come in Python. A destra la stessa cosa con una funzione `main` esplicita."*

Il messaggio chiave è il bullet "*A `.aer` file without `fn main` is a valid program*". Aeris può essere usato come scripting senza cerimonia, ed è la stessa grammatica dei programmi di produzione. Sottolinea anche `use io` mandatorio — niente import implicito di moduli built-in. Questa è una decisione di leggibilità: chi legge sa esattamente quali moduli un file tocca.

---

## Slide 5 — How an interpreted language works

Apri dicendo: *"Prima di entrare nei costrutti, vi faccio vedere come è fatto dentro Aeris, così quando parleremo di static check capirete dove avviene."* Quattro fasi:

1. **Lexer** — legge i byte e produce token annotati con la riga.
2. **Parser** — costruisce l'**AST**, l'albero di sintassi. Spiega che AST sta per *Abstract Syntax Tree* — un albero dove ogni nodo è un costrutto del linguaggio (una `let`, una chiamata, un `saga`, un `agent`).
3. **Static check** — verifica strutturale prima di girare. È qui che vengono catturati errori come "schema senza versione" o "ciclo in un agent_net". Ogni categoria ha un suo exit code, vedremo la tabella in fondo.
4. **Interpreter** — visita l'albero nodo per nodo e produce i side effect attraverso la libreria standard.

Chiudi con *"l'AST è il programma"* — non c'è bytecode, non c'è compilazione. E con "~6 KLOC di Rust" per dare la dimensione: un linguaggio piccolo, ispezionabile.

---

## Slide 6 — The AST walk

Slide didattica. *"Per chi non ha mai visto un tree-walking interpreter: questo è praticamente lui. Una funzione ricorsiva che riceve un nodo e un ambiente, fa `match` sul tipo di nodo, e per ognuno decide cosa fare."*

Vai veloce sui rami: `Let` aggiorna l'ambiente, `If` valuta la condizione e sceglie un ramo, `Call` valuta gli argomenti e applica la funzione, `Block` itera. Spiega che `return` / `break` / `continue` sono varianti di errore che salgono lo stack — un trucco comune negli interpreter scritti in Rust per gestire la non-locality del flusso.

Sul lato destro la chiusura: una chiamata di funzione è solo una sotto-walk; le closure mantengono vivo l'ambiente, quindi `spawn { ... }` può portarsi appresso lo scope.

---

## Slide 7 — The four layers

Slide architetturale. *"Aeris è organizzato in quattro layer concettuali. Ogni layer si compone sopra il precedente."* Indica il diagramma con la mano:

- **L1, sintassi** — la grammatica vera e propria.
- **L2, semantica verificabile** — le capabilities, i contratti, l'intent. Qui sta la parte "veracizzabile" del linguaggio.
- **L3, agentic loop** — il pattern saga con do/undo, le chiavi di idempotenza.
- **L4, orchestrazione multi-agente** — i grafi di agenti tipati.

L'idea centrale è "**opt-in by depth**": uno script di trenta righe vive in L1; una pipeline che si ripristina da sola usa L1+L2+L3; un sistema multi-agente coordinato usa tutti e quattro. Non paghi quello che non usi.

---

## Slide 8 — Why these four layers?

Slide di riflessione, vai più lento. *"Perché proprio questi quattro layer? La risposta sta in un cambio di paradigma."*

Il codice oggi è generato sempre più da LLM, e letto da LLM — per ragionare, debuggare, modificare. Due requisiti diventano dominanti: **ridurre il non-determinismo** in ogni punto in cui il linguaggio può controllarlo, e **rendere il codice meccanicamente verificabile** — quello che è scritto nel sorgente deve essere la verità.

I quattro layer sono la risposta:

- **L1** — densità sintattica e zero ambiguità riducono le allucinazioni del modello.
- **L2** — `cap` rende l'intento di una funzione meccanicamente controllabile dal compilatore.
- **L3** — traccia per ogni step più compensazioni idempotenti rendono il recovery deterministico anche sopra un'esecuzione che non lo è.
- **L4** — quando tre o più agenti coordinano, il protocollo di routing *è* il programma: lo trasformiamo in un grafo tipato e togliamo il coordinamento dalle stringhe di prompt.

Chiudi con: *"opt-in è il contratto"* — script monouso usa solo L1, sistema regolamentato usa tutti e quattro.

---

## Slide 9 — Divider "Core language"

Slide di transizione, una frase. *"Adesso il giro guidato dei costrutti del linguaggio: tipi, controllo di flusso, sagas, concorrenza, moduli, test."*

---

## Slide 10 — Language at a glance

Slide ampia, vai per priorità. *"La sintassi è curly-brace come in Rust, Swift, Kotlin, TypeScript. Tre forme di binding: `let` immutabile come default, `var` mutabile solo dentro una funzione, `const` a livello di file."*

Tre cose da segnalare:

- Le **annotazioni di tipo sono opzionali**: si scrivono ai confini delle API, non ovunque.
- Le funzioni **ritornano l'ultima espressione** del body. Niente `return` per le funzioni corte.
- I **call site usano named arguments**: `greet(name: "Aeris")` invece di `greet("Aeris")`. È leggibile da chiunque, anche da chi non conosce la signature.

Mostra l'interpolazione `"{expr}"` e i triple-quoted per i prompt LLM lunghi. Chiudi con: *"questo è il fondamento — la novità sta nei costrutti più alti che vediamo tra poco"*.

---

## Slide 11 — Control flow

*"`if` e `match` sono espressioni — ritornano un valore. Niente operatore ternario, non serve."* Indica l'esempio: `let label = if score >= 90 { "A" } else ...`.

Il `match` ha pattern di letterali, range come `400..499`, wildcard `_`. È **esaustivo**: il compilatore ti dice quale caso hai dimenticato. Questo è un dettaglio importante perché significa che un LLM che dimentica un ramo si ferma in compile, non a runtime.

Loops: `for` su qualsiasi iterabile, `while`, `loop` come sugar per `while true`. `break` e `continue` funzionano in tutte le forme.

---

## Slide 12 — Pattern matching

Slide di approfondimento. *"Il `match` non guarda solo letterali e range — fa destrutturazione su enum e su liste."*

Mostra l'enum `Status` con tre varianti, una delle quali ha un payload nominato (`Banned { reason, until }`). Spiega che il pattern lega il payload — `Active(t)` rende `t` disponibile nel corpo del ramo.

I pattern di lista con `..rest` sono il trucco classico per gestire teste e code. E `result<T>` si destruttura con `Ok` e `Err` — è la stessa API uniforme per `option<T>`, `result<T>`, ed enum custom. Una grammatica sola per la dispatch sui dati.

---

## Slide 13 — Models

Slide importante. *"`record` è quello che vi aspettate: una struct con campi nominati e tipizzati. Niente di nuovo."*

Il costrutto distintivo è **`model X@vN`**: una struct più un tag di versione più clausole `where` per i vincoli. Spiega: *"qualunque dato attraversi un confine di fiducia — ingress HTTP, decodifica JSON, scambio fra agenti — viene validato contro il modello. Una `Invoice@v1` ricevuta via HTTP con un `amount` negativo non entra nel programma: produce errore."*

`extends` è il modo per evolvere uno schema: `v2` eredita i campi di `v1` e ne aggiunge. Non puoi rimuovere o rinominare. Una bare `Invoice` senza `@vN` su un confine di fiducia è un errore al parse — esit code 68. Questo è uno dei meccanismi più importanti per la sicurezza: hallucination del modello LLM che producono JSON fuori shape vengono fermate qui.

---

## Slide 14 — Errors & recovery

*"In Aeris gli errori sono valori, non eccezioni. Una funzione che può fallire ritorna `result<T>` — cioè `Ok(value)` oppure `Err(error)`."*

I quattro strumenti:

- **`?`** dopo un'espressione: se è `Err`, ritorna dal chiamante. È il `try` di Rust.
- **`??`** sostituisce con un valore di default su `Err`, `None`, o unit. Comodo per i fallback.
- **`catch err { ... }`** gestisce l'errore inline, dando un fallback.
- **`defer stmt`** schedula una cleanup che gira a ogni uscita — return, `?`, raise. È esattamente il `defer` di Go.

Chiudi con: *"niente exception da inseguire su stack, niente stack trace da decifrare. O torna il valore, o torna l'errore — e la firma vi dice quale."*

---

## Slide 15 — Time control

Slide breve, fai velocemente. *"Tre costrutti per i pattern temporali che si scrivono ogni giorno."*

- **`every D`** — loop periodico. La prima iterazione gira subito.
- **`retry N, delay: D`** — riesegue il blocco su `Err`, fino a N volte.
- **`timeout D`** — fa fallire il blocco se supera il budget di tempo a parete.

Spiega che `clock.sleep(D)` è la primitiva sotto: viene **registrata** nella traccia, così `aeris replay` riproduce la stessa timeline. Il messaggio finale è il *takeaway*: niente scheduler esterno — niente cron, niente systemd timer, niente Airflow. Il loop è parte del programma, osservabile nella traccia, replayabile.

---

## Slide 16 — Saga

Slide importante, vai più lento. *"`saga` è il costrutto principale per le operazioni di scrittura che hanno bisogno di compensazione."*

Vai per analogia: *"pensate a un'operazione bancaria che addebita più conti in sequenza. Se l'addebito al terzo conto fallisce, vorreste annullare i primi due. Le saghe fanno esattamente questo."*

Ogni `step` ha sia `do` (l'azione) sia `undo` (la compensazione). Se uno step intermedio fallisce, il runtime esegue gli `undo` degli step già completati in ordine inverso. È il pattern delle transazioni distribuite.

I quattro bullet:

- Multi-step con compensazione.
- `do` e `undo` obbligatori.
- Failure → undo in reverse.
- `undo: noop` è ammesso solo se il `do` non scrive — ogni scrittura deve dichiarare come disfarsi.

Esiti deterministici: `ok`, `rolled_back`, o `PartialFailure` (quando anche il rollback fallisce — exit code 74). Niente stato a metà nascosto. Aggiungi: *"`intent "..."` dichiara lo scopo del saga, e finisce in ogni evento della traccia"*.

---

## Slide 17 — Idempotency key

Slide di chiarimento, parti dal problema. *"Capita che la rete cada nel mezzo di una scrittura. Mandate `POST /charge`, non sapete se è arrivato. Riprovate? Rischiate il doppio addebito. Non riprovate? Rischiate di non addebitare. Questa è una vita."*

La soluzione che il mondo ha adottato: la **idempotency key**. Una stringa univoca per ogni richiesta; il backend memorizza le key che ha già processato e droppa i duplicati. La usano Stripe, AWS, Kubernetes, le code di messaggi — ognuno ha il campo standard dove infilarla.

Aeris la genera per voi: dentro ogni step di un saga, la formula è `blake3(trace_id ‖ step_name ‖ invocation_index)`. Sono tre valori stabili: l'identificativo dell'esecuzione, il nome dello step, e un contatore del retry. L'hash blake3 li mescola in una stringa unica.

Tre punti chiave da dire:

- **Generata in ogni modalità `enforce`** (off / loose / strict) — non dipende dalla disciplina di capability.
- **Solo dentro saga** — fuori da un saga non c'è il `step_name`, quindi nessuna key automatica.
- **Registrata nel trace** — `aeris replay` rigioca le stesse identical key, quindi il backend droppa anche i replay.

Chiusura forte: *"stesso programma rigiocato, stesse key, il backend droppa ogni duplicato. Replay senza paura di addebiti doppi o doppie apply su Kubernetes."*

---

## Slide 18 — Concurrency

*"`spawn` lancia un blocco in un thread separato e ritorna un handle; `await` aspetta il risultato. La forma è quella che vi aspettate da Rust o Kotlin."*

`channel<T>` è una coda bounded fra thread: send su pieno blocca, recv su vuoto blocca. Si itera con `for x in ch { ... }` fino a chiusura.

La cancellazione è **cooperativa**. Significa: il runtime non interrompe un blocco arbitrariamente. Spiega che ci sono cancel-point precisi — `await`, `?`, le chiamate di capability, `for x in ch` — e la cancel arriva solo a uno di quelli. Limitazione attuale: nel runtime tree-walk corrente `spawn` gira in linea sullo stesso thread; un evento `spawn_inline` nel trace lo segnala. Una vera scheduler arriva in v0.4.

---

## Slide 19 — Modules

*"Tutte le import si fanno con la keyword `use`. La forma dell'import dice in quale layer state."*

Vai per i tre layer:

- **Layer 1** — stdlib general-purpose: `use io, json, fs, http, shell`. Built-in al binario.
- **Layer 2** — gestori nativi per dominio: `use ai, kube, mongodb`. Anch'essi built-in.
- **Layer 3** — librerie `.aer` esterne: `use deploy from "github.com/..." deploy@"1.2.0"`. Pinned per hash blake3 in `aeris.toml`. Niente `.so`, niente `.dll` a runtime.

Sottolinea il bullet "`use` è mandatorio": un body call `http.post(...)` senza `use http` in cima al file è un errore di compile (exit code 72). Niente namespace globale implicito — chi legge il file sa quali moduli vengono toccati. Cyclic imports rifiutati al parse.

---

## Slide 20 — Standard library — general-purpose modules

Slide-catalogo, vai veloce. *"Questi sono i moduli built-in: niente di esotico, le solite cose."* Cita le righe più importanti: `io` per terminale, `fs` per file system, `http` per il client, `shell` per i sottoprocessi, `json` e `yaml` per parsing. Menziona `clock` e `random` come **registrati per il replay** — sono la chiave per la riproducibilità.

L'esempio a destra mostra una sequenza idiomatic — decode JSON, get HTTP, println. Tre righe per fare un health check.

---

## Slide 21 — Standard library — native domain handlers

*"Qui invece sono i moduli di dominio: `ai` per le chiamate al modello, `kube` per Kubernetes, `docker`, `mongodb`, `minio`, `rabbitmq`. E `audit` per gli eventi di audit log."*

L'esempio mostra un intent + `kube.apply` + `audit.event`. Sottolinea che `intent "..."` è obbligatorio in modalità strict prima di una scrittura — vedremo questa regola fra qualche slide. Chiusura: *"questi moduli sono compilati dentro al binario, non sono plug-in caricati a runtime"*. È una scelta di sicurezza: ogni effetto possibile è noto al compilatore.

---

## Slide 22 — A full HTTP server

Slide colpo d'occhio. *"Un server HTTP completo in dieci righe. `net.http(port)` apre un listener TCP, `server.accept()` riceve una richiesta, `req.reply` risponde. Per-request fan-out con `spawn`."*

Niente framework, niente Express, niente Flask. È parte del binario. Ogni accept produce un evento `http_request` nella traccia, ogni apertura del listener produce `net_listen`. Va detto: il fan-out con `spawn` in v0.3 gira ancora in linea (cf. slide concorrenza); il pattern è già forward-compatible per quando arriva il vero scheduler.

---

## Slide 23 — Tests

*"I test sono parte del linguaggio. Si scrive un blocco `test "nome" { ... }` in un qualsiasi file `.aer`, e si lancia con `aeris test`."*

Vai sulle quattro asserzioni:

- `assert` per il booleano semplice.
- `assert_status` per lo status HTTP.
- `assert_json` per controllare che una stringa contenga certe chiavi JSON.
- `assert_semantic` — questa è notevole — **usa il modello come giudice**: gli passate l'output prodotto e un criterio, e l'assert passa solo se il modello dice "sì, soddisfa il criterio". Permette test qualitativi: *"il riassunto è fedele e completo all'originale"*.

`property` blocks generano casi random (200 di default) — utile per test di invarianti come associatività, commutatività. Il file è la suite — niente keyword `suite`, una concettualità in meno da ricordare.

---

## Slide 24 — Divider "AI primitives"

Una frase. *"Adesso entriamo nella parte AI. Niente librerie esterne — le primitive sono nella libreria standard."*

---

## Slide 25 — AI primitives — direct call and multi-turn

*"`ai.complete` è la chiamata diretta al modello. Prompt in input, risposta in output."*

Mostra `ai.session` — sessione multi-turno. Restituisce un valore `Session`; `ai.session_ask` aggiunge un turno e ritorna `(nuova_sessione, risposta)`. Spiega l'**auto-compaction**: oltre i 40 messaggi nella cronologia, il runtime tiene gli ultimi 20 e collassa i precedenti in un riassunto di sistema. È il modo per evitare di sforare il context window in conversazioni lunghe.

Ogni chiamata viene registrata come evento `ai_call` nel trace. `aeris replay` la rigioca dal nastro senza contattare il modello: la sessione di test gira offline, deterministicamente.

---

## Slide 26 — AI primitives — constrained choice and usage

Slide breve. *"`ai.decide` è una decisione vincolata."* Spiega: gli passate un prompt e una lista di scelte, il modello deve sceglierne una. Se la risposta cade fuori dalla lista, il runtime fa retry — fino a `retries` volte. Esauriti i retry, `Err(err.llm(...))`. Usate quando volete che il modello prenda una decisione discreta.

`ai.usage` è il contatore di processo: token consumati, costo stimato, numero di chiamate. Il costo viene da una tabella prezzi statica indicizzata sul nome del modello — è un diagnostic in memoria, non una chiamata di rete.

---

## Slide 27 — `ai.chat` — knowledge base and integrated server

*"`ai.chat` ha due forme. La prima carica una directory di documenti come knowledge base nel prompt di sistema, e vi dà un valore `Chat` su cui chiamare `.ask(prompt)`. Tre righe di codice per un chatbot su una cartella di documentazione."*

La seconda forma è l'overload con `port`: stessa knowledge base, ma la chiamata **non ritorna** — entra in un accept-loop HTTP che espone `GET /`, `POST /api/chat`, `GET /api/health`, e gestisce CORS. *"Un chatbot completo — knowledge base, server, healthcheck, CORS preflight — in una singola chiamata di libreria standard. Niente strato applicativo da scrivere."*

Buon momento per dire: *"se vi chiedete perché il backend supporta sia HTTP API che CLI locale, vediamo la configurazione nel manifest fra poco"*.

---

## Slide 28 — Multi-agent

*"Per quando un solo agente non basta, due strade: dichiarativo o programmatico."*

A sinistra `agent_net`: un grafo aciclico tipato. Ogni `agent` ha `accept` e `produce` come `model@vN`, e ogni `flow` viene validato — il messaggio che attraversa un edge deve avere il tipo giusto. I cicli sono rifiutati al parse (exit code 70); l'iterazione si esprime con la clausola `until:`. Il **protocollo di routing è iniettato dal runtime nel system prompt di ogni agente** — non lo scrivete a mano nei prompt.

A destra `ai.network`: programmatico. Si registrano gli agenti a runtime, si avvia con `net.run`. L'hand-off è testuale: un reply prefissato `>>NAME:` instrada al nodo nominato. Quando usarli? *"`agent_net` quando gli schemi sono stabili; `ai.network` quando l'insieme degli agenti è scoperto a runtime, per esempio caricato da una directory di prompt."*

---

## Slide 29 — Divider "Verifiability"

Una frase di transizione. *"Adesso il primo dei due blocchi teorici. La signature è la verità su cosa una funzione può fare."*

---

## Slide 30 — `cap` — a permission, carried as a value

Slide cardine. *"In Aeris il permesso di compiere un effetto esterno — fare una chiamata HTTP, scrivere un file, chiamare il modello — è un valore passato come parametro."*

Mostra i due esempi: `fn fetch` ha `cap: cap[http.get @ ["api.acme.com"]]` nella firma. Significa: questa funzione può chiamare `http.get` solo verso `api.acme.com`. `fn total` non ha il parametro `cap` — non può raggiungere nulla di esterno, è strutturalmente pura.

Quattro punti da fare:

- `cap` è un **parametro** la cui tipologia elenca le operazioni consentite.
- Una funzione senza `cap` non può raggiungere niente di esterno.
- Le **allow-list** restringono *quali* endpoint sono raggiungibili.
- **Pura ⇔ niente `cap`** — la purezza è una proprietà strutturale della signature, non una keyword.

Chiusura: *"Aeris applica un'idea ben nota — permessi come parametri — a codice generato da LLM. Non è ricerca nuova, è ingegneria applicata."*

---

## Slide 31 — Allow-list

*"Le allow-list non sono solo per HTTP — ogni famiglia di capability ha la sua forma."*

Mostra la tabella: HTTP ha host, FS ha path globs, Kubernetes ha context, AI ha modelli, shell ha argv0. *"Quando leggete una firma, sapete subito quali sistemi esterni vengono toccati **e quali endpoint** sono raggiungibili — senza entrare nel body."*

Una firma che chiede un endpoint fuori dal ceiling del progetto è errore al parse (exit code 71). È un controllo importante perché impedisce a un LLM di sbagliare in silenzio: se generano `cap[http.get @ ["evil.com"]]` ma il manifest non lo autorizza, il programma non parte.

---

## Slide 32 — Narrowing and `main(cap)`

*"Quando passate `cap` a una funzione chiamata, potete **restringere** — mai allargare. `cap.subset[...]` produce un sotto-cap che il chiamato vede; il padre rimane invariato."*

Spiega l'output a destra: quando lanciate `aeris run`, il runtime stampa la **effective cap di main**. Questo dice cosa il programma intero è autorizzato a fare — un singolo blocco di testo che il revisore può leggere.

Il punto fondamentale è in fondo: *"`main(cap)` è sintetizzato da `aeris.toml [caps]`. È l'**unica** via per cui un valore `cap` entra nel programma. Revisionare il manifest equivale a revisionare la superficie di autorità completa."*

---

## Slide 33 — `enforce` — three modes, one grammar

*"La disciplina cap non è on/off. Sono tre modalità."*

Vai per la tabella, riga per riga. Sottolinea: *"in modalità `off` — che è il default di `aeris init` — i check statici sono soppressi. Il programma gira come uno script. In `strict` ogni check è errore. `loose` è la via di mezzo: il manifest fa da ceiling a runtime, ma il check statico non forza ancora il cap annotato."*

Il messaggio centrale è la callout finale: *"le modalità governano **solo il check statico**. Traccia, replay, validazione `model@vN`, valutazione delle `policy` restano attive in tutte e tre."* Questo è importante: anche un programma in modalità script ha trace e replay completi.

Spiega la scala di adozione: si parte con `off` per prototipare, si flippa a `loose` per portare il progetto in pre-prod, infine `strict` per produzione. Non c'è da riscrivere il codice — solo da aggiungere annotazioni dove servono. `aeris fmt --narrow-caps` propone in automatico le firme minime.

---

## Slide 34 — Divider "Governance & reasoning"

Una frase. *"Tirate le fila teoriche. Intent, contratti, policy, trace, supply chain. Tutti i meccanismi per rendere il non-determinismo esplicito, isolato, governabile."*

---

## Slide 35 — The thesis — controlled non-determinism

Slide-tesi. *"La tesi di Aeris in una frase: un piccolo linguaggio dove la **visibilità** degli effetti, la **compensazione** delle scritture esterne, l'**integrità** della supply chain, e l'**intent** sono proprietà strutturali del sorgente."*

La tabella delle tre sorgenti di non-determinismo: il modello (stesso prompt, output diverso), la grammatica (costrutti ambigui forzano il modello a indovinare), il mondo (le reti cadono, i database mutano). Per ognuna Aeris ha una risposta: trace+replay per il modello, una sola forma canonica per la grammatica, `cap`/`intent`/`policy`/`model@vN` per il mondo.

Chiudi forte: *"Aeris non prova a eliminare il non-determinismo — lo rende esplicito, isolato e governabile."*

---

## Slide 36 — From a language for humans to a language for agents (1/2)

Slide di riflessione, parla lentamente. *"I linguaggi di programmazione sono sempre stati un'interfaccia fra la mente umana e la macchina. Sintassi leggibile, errori chiari, idiomi: tutto era pensato per ridurre il carico cognitivo dell'umano che scriveva e leggeva. **Questa assunzione è caduta.**"*

Due colonne:

- **WHAT, not HOW**: il principale *autore* di codice oggi è un LLM. Un LLM non ha un modello mentale, ha una distribuzione di probabilità sul token successivo. Quindi la domanda non è più *"come faccio la sintassi più leggibile?"* ma *"quali intenzioni posso lasciare che un agente esprima direttamente, senza codificarle come meccanismo?"* In Aeris `saga`, `agent`, `intent`, `policy` non sono meccanismi — sono **intenzioni complete** promosse a costrutti del linguaggio.

- **High abstraction, not low**: c'è la tentazione opposta — fare un linguaggio low-level perché l'LLM sbaglia meno. *Sbagliato.* Un LLM genera codice corretto con probabilità proporzionale a (a) quanto il codice somiglia al suo corpus di training, e (b) quanto lo spazio di completamenti validi è ristretto dal linguaggio stesso. Abstraction alta fa entrambe le cose: meno decisioni, meno punti di errore; più signal-to-noise per token generato.

---

## Slide 37 — From a language for humans to a language for agents (2/2)

Continui. *"I linguaggi storicamente separavano due cose: **cosa fa il codice** (semantica) e **perché lo fa** (commit, ticket, descrizioni PR). La separazione era necessaria per gli umani — la macchina non aveva bisogno del perché."*

- **Il costo di quella separazione**: un LLM che legge un `.aer` *senza* il perché deve fare reverse-engineering del proposito dalle meccaniche. **Ogni inferenza è un punto di non-determinismo.** Un agente che esegue codice senza sapere perché non può decidere autonomamente se continuare, fermarsi, o escalare quando qualcosa sembra sbagliato — non ha un criterio di accettazione contro cui giudicare uno stato inatteso.

- **Why-as-grammar**: in Aeris il perché è parte della grammatica. `intent`, `requires:` / `ensures:`, `policy` sono **costrutti tracciabili e strutturalmente enforced** che riducono lo spazio di interpretazioni valide che l'agente può adottare, rendono lo scopo del programma **machine-readable**, e si propagano come dati strutturati nel trace.

Chiusura: *"l'obiettivo non è un linguaggio che gli umani scrivono meglio — è un linguaggio che gli agenti **eseguono con più certezza**."*

---

## Slide 38 — `intent` — executable documentation

*"`intent` è quello che oggi vive nei commenti, nei commit, nei ticket — e che l'agente non legge mai. Aeris lo porta dentro la grammatica."*

Mostra l'esempio: un blocco `intent "monitor API latency, alert above the threshold"` che racchiude un `every 1m` con `http.get` e `http.post`. Spiega: la stringa `intent` viene emessa nel trace come evento `intent_enter` all'ingresso e `intent_exit` all'uscita. Ogni evento emesso dentro il blocco porta il campo `"intent"` — così potete risalire allo scopo di ogni singola riga del trace.

`intent` è **mandatorio** attorno a ogni chiamata write-effectful. Il check è lessicale al compile time — exit code 66 quando manca. Punto onesto: *"non verifica che il body corrisponda alla stringa. Rende impossibile l'omissione — non la disonestà."*

---

## Slide 39 — `requires:` / `ensures:`

*"`requires:` lista le pre-condizioni — controllate all'ingresso della funzione, prima che giri il body. `ensures:` lista le post-condizioni — controllate su ogni via di uscita; l'identificatore `result` fa riferimento al valore ritornato."*

Mostra `fn discount`: tre clausole. La pre-condizione su `amount`, la pre-condizione su `pct`, la post-condizione che il risultato è dentro il range. Spiega: disponibili anche sulle saghe — `saga deploy` con un requires sull'env e un ensures su un health check.

Punto chiave: *"questi sono contratti **runtime**. Una violazione produce un `ContractViolation` fatale — exit code 64, non catturabile con `?` o `catch`."* Non si nasconde una violazione di contratto. Se vi chiedono "perché non SMT?" rispondete: i verdetti di un solver dipenderebbero dalla macchina e dalle euristiche, sarebbe non-determinismo *negli strumenti di sviluppo* — peggio di quello che stiamo provando a controllare.

---

## Slide 40 — `policy` — declarative governance

*"`policy` esprime una regola di sicurezza o di limite come costrutto del linguaggio. Quello che oggi vive in OPA, Rego, sentinel — qui sta nel sorgente."*

Tre esempi affiancati: `production_egress` che nega le chiamate HTTP fuori da una whitelist di host; `model_budget` che mette un limite ai token per minuto e al costo per giorno; `pii_redact` che richiede di non avere PII nel prompt e nega le risposte che contengono email.

Le clausole disponibili:

- `match:` — su quali chiamate la regola si applica.
- `deny:` — viola se la condizione è vera.
- `require:` — viola se è falsa.
- `limit:` — quota su una finestra.
- `audit:` — campi aggiuntivi nel trace event.
- `when:` — gate ambientale.

Chiusura: *"le regole vivono nel programma, non nel prompt di sistema. Il modello non se le può dimenticare; il runtime le valuta su ogni chiamata che combacia."*

---

## Slide 41 — Trace — what every run records

Slide-tabella. *"Ogni esecuzione di Aeris scrive un trace JSONL in `<progetto>/<output_dir>/traces/<id>.jsonl`. Sempre attivo, in tutte le modalità."*

Apri sottolineando il banner: *"al boot vedete su `stderr` la riga `[aeris] trace_id = 01JFE… → …/traces/01JFE….jsonl`. Quell'identificativo è la chiave per replay e bisect — copialo e tienilo a portata di mano."*

`output_dir` di default è `.aeris/`, relativo alla directory del progetto (quella che contiene `main.aer` o `aeris.toml`), non alla `cwd` della shell. Configurabile dal manifest nella sezione `[runtime]`: *"se l'osservabilità deve finire altrove — per esempio in `build/obs/` perché abbiamo già un raccoglitore puntato lì — basta una riga nel manifest."* `trace = false` disattiva la scrittura su disco lasciando il canale in memoria per i test.

Vai sulla tabella delle fonti: chiamate `ai.*` con prompt e risposta; `clock.now` e `random.next` con il valore letto; `http.*` con URL, method, status, hash di richiesta e risposta; lettura/scrittura file con path, lunghezza, hash; chiamate L2 (`minio.*`, `mongodb.*`, `rabbitmq.*`) con i campi specifici della famiglia e un marker `backend` che dice quale impl ha gestito la chiamata — mock, real-fs, replay. E gli eventi strutturali — `intent`, `saga`, `agent_net`, `policy` — ognuno con i suoi sotto-eventi.

L'esempio JSON in basso è una riga reale del trace: un `ai_call` con prompt, modello, token, intent attivo.

Il punto in fondo è importante: *"i trace ID sono propagati attraverso le chiamate HTTP via header `X-Aeris-Trace-Id`. Una singola richiesta resta contigua attraverso processi diversi."*

---

## Slide 42 — Replay and bisect

*"Quel trace serve a due cose: rigiocare l'esecuzione e fare bisect delle regressioni."*

`aeris replay <id>` rigioca il programma contro il nastro registrato. `ai.*` torna la risposta registrata — niente chiamata al modello, niente costo. `clock.now` e `random.next` tornano i valori registrati. `http.*` rigioca le fixture (default) o va live con `--live`. L'output è **bit-identical** sul sottoinsieme deterministico del programma — il sottoinsieme stocastico è fissato dal nastro.

`aeris trace diff <a> <b>` allinea gli eventi per `(scope, ordinal)` e segnala dove divergono. È la base per il **bisect** delle regressioni: stesso programma, due esecuzioni differenti, dove si rompe?

`policy_drift` è un evento di prima classe — quando l'esito di una policy in replay diverge dall'originale, il runtime emette questo evento invece di fermarsi.

---

## Slide 43 — External libraries — content-addressed supply chain

*"Le librerie esterne sono `.aer` source, hostate su GitHub. Ma non è solo `use lib from github` — c'è una disciplina."*

Ogni dipendenza è identificata dal **blake3 hash** dei suoi byte. Se quello che scaricate non corrisponde all'hash registrato in `aeris.toml`, il run fallisce **prima** che una sola riga della dipendenza venga eseguita. Niente `latest`, niente `*`, niente Git tag mobili — la risposta a "che versione c'è in questo build?" è sempre nel manifest.

Le librerie esterne sono **sempre `.aer`** — niente `.so` o `.dll` a runtime. È una scelta di sicurezza: un binario su disco aggiungerebbe una superficie di effetti che il controllore statico non può ispezionare.

Per orientamento: *"è lo stesso approccio di content-addressing già usato da Cargo, npm, Nix"*.

---

## Slide 44 — Manifest and lock file

*"Il `aeris.toml` è il singolo riferimento del progetto. Quattro sezioni principali."*

- **`[project]`** — nome e versione di Aeris.
- **`[caps]`** — il **massimo livello di autorità** che il programma ha a runtime. È il ceiling: ogni `cap` nel codice è un sottoinsieme di questo.
- **`[ai.backend]`** — dove vanno le chiamate AI: API HTTP o processo CLI locale.
- **`[policies]`** — quali regole sono attive.

A destra il `surface.lock`: una entry per ogni `pub fn`. Spiega: *"se una PR cambia una funzione pubblica in modo che raggiunga un nuovo host o path, il file di lock va rigenerato. Il diff appare come prima hunk nella code review. Il revisore vede subito la nuova superficie."*

---

## Slide 45 — Divider "Putting it together"

Una frase. *"Mettiamo tutto insieme con un esempio reale: triage di alert SRE."*

---

## Slide 46 — End-to-end — SRE alert triage (1/2)

Slide concreta, parla dell'esempio. *"Sistema reale di triage degli alert. La parte AI è una rete di agenti tipizzata."*

Indica i tre modelli `Alert@v1`, `Diagnosis@v1`, `FixPlan@v1` — schemi versionati con vincoli. Poi i due agenti:

- `classify` — riceve un `Alert`, produce una `Diagnosis`. Usa Claude Haiku perché è veloce.
- `plan` — riceve una `Diagnosis`, produce un `FixPlan`. Usa Claude Opus perché serve ragionamento.

E l'`agent_net triage` che li collega — `classify -> plan`, con `until:` che ferma il loop quando la confidence supera 0.85 o dopo 3 iterazioni.

Punto chiave: *"il sistema dei tipi taglia le hallucination. Un modello che produce JSON fuori shape diventa `Err(err.schema(...))`, l'agente lo rifa entro il budget di retry — e nessun valore mal formato attraversa il confine."*

---

## Slide 47 — End-to-end — SRE alert triage (2/2)

*"Adesso la parte operativa. Una saga con due step che applica il fix e si sa annullare, dentro un loop ogni 30 secondi."*

Il `saga apply_fix` ha due step: `snapshot` (salva lo stato corrente del cluster) e `apply` (applica i comandi). Se l'`apply` fallisce, il runtime esegue l'undo dello `snapshot` — rimuove il file temporaneo. Le chiavi di idempotenza vengono iniettate automaticamente nelle chiamate `shell.exec`.

Il loop `every 30s` scarica gli alert da Alertmanager, li passa per il triage, e applica il fix. *"Quello che oggi è LangChain più Argo più OPA più bash messi insieme, qui sta in cento righe di codice — un solo file, una sola grammatica."*

---

## Slide 48 — Error model — layered exit codes

Slide-tabella, vai veloce. *"Categorie di failure distinte, codici di uscita distinti. La CI può reagire differentemente."*

- **Lex/Parse** — exit 1, sintassi mal formata.
- **Static check 64** — errore di tipo o contratto.
- **65** — `cap` mancante o troppo largo.
- **66** — chiamata write senza `intent` esterno.
- **67** — step di saga con `undo: noop` su do effettuale.
- **68** — `model` usato senza `@vN` su un confine di fiducia.
- **69** — dep hash che non combacia.
- **70** — ciclo dichiarato in un `agent_net`.
- **71** — allow-list che eccede il ceiling.
- **72** — modulo referenziato senza `use`.
- **Runtime 74** — `saga PartialFailure` (i retry sugli undo sono esauriti).

Punto chiave: *"un fallimento `intent` mancante (66) non è la stessa cosa di un missing undo (67) o di un model version sbagliato (68). La CI può prendere decisioni differenti su ognuno."*

---

## Slide 49 — Honest limits

Slide di onestà. *"Quattro cose che Aeris **non** risolve, dichiarate apertamente."*

- **La prima esecuzione del modello resta non-deterministica.** Il replay rende riproducibile *dopo* la prima volta. La prima volta è in balia del modello.
- **La correttezza interna a una `cap` legittima non è verificata.** Se una funzione ha legittimamente `audit.write` e dentro scrive l'attore sbagliato, Aeris non se ne accorge. Test, property check, RBAC al backend.
- **L'over-broadening dei permessi è un problema di processo.** Il diff del `surface.lock` lo rende visibile, ma il vero enforcement vive nella CI.
- **Il cascading undo è best-effort.** `PartialFailure` quando i retry esauriscono — è il limite noto del pattern saga.

Chiusura: *"Aeris è la **prima linea di difesa**, non l'unica."*

---

## Slide 50 — What Aeris refuses on principle

*"Quattro cose che Aeris ha deciso di **non avere**, per principio. Ogni rifiuto paga un costo dichiarato."*

- **Niente proof formali automatici** — i verdetti di un solver dipenderebbero dalla macchina e dalle sue euristiche.
- **Niente inferenza di capabilities** — la signature deve essere la verità; cambi nascosti romperebbero la code review.
- **Niente riferimenti mutabili nelle dipendenze** — niente `latest`, niente `*`, niente tag Git mobili.
- **Niente plug-in nativi a runtime** — aggiungerebbero una superficie di effetti che il check statico non può vedere.

Chiusura: *"ogni rifiuto è pagato con un costo accettato — perché ciò che è nel sorgente resti la verità."*

---

## Slide 51 — Thanks

Chiusura semplice. *"Aeris è un progetto aperto. Le fonti di verità del linguaggio sono in `docs/thesis.md`, `docs/language.md`, `docs/project.md`, `docs/plan.md`, `docs/cheatsheet.md`. Le domande sono benvenute."*

---

## Note di sopravvivenza per il Q&A

Domande che probabilmente arriveranno, con la risposta sintetica:

- *"Perché blake3 e non SHA-256?"* — È un hash crittografico moderno, più veloce di SHA-256 a parità di sicurezza, scelto già da Cargo e altri tool che hanno bisogno di throughput su grandi quantità di byte. Non è una scelta load-bearing — qualsiasi hash crittografico farebbe lo stesso mestiere.

- *"Cosa succede se un agente in un `agent_net` produce un JSON mal formato?"* — Diventa `Err(err.schema(...))`. L'agente lo ritenta entro il `retries` budget. Nessun valore mal formato attraversa l'edge.

- *"Una saga può avere step di sola lettura?"* — Sì, con `undo: noop`. Ma se hai solo letture, una saga è inutile — basta una catena di `let` dentro una funzione normale. La saga ha senso quando c'è compensazione da dichiarare.

- *"Come si configura il backend LLM?"* — In `aeris.toml [ai.backend]`. Due forme: `kind = "http"` per una API compatibile OpenAI/Anthropic, `kind = "cli"` per un sottoprocesso a riga di comando come `claude --print` o `ollama run`.

- *"L'idempotency key viene generata anche fuori da una saga?"* — No. La formula `blake3(trace_id ‖ step_name ‖ invocation_index)` ha bisogno del `step_name`, che esiste solo dentro un `step` di `saga`. Fuori da una saga, se vuoi idempotenza devi mettere l'header `Idempotency-Key` a mano.

- *"Perché niente `pipeline` come costrutto?"* — Esiste solo dove servono compensation (e lì si chiama `saga`). Per il sequencing di sole letture, una catena di `let` fa la stessa cosa con meno cerimonia. Sarebbe stato un costrutto in cerca di un problema.

- *"Cosa succede se un dep ha un hash diverso da quello in `aeris.toml`?"* — Il run fallisce **prima** che una riga della dep venga eseguita. Exit code 69. Non c'è modo di bypassare — è il commitment 3 della tesi.

- *"Il trace JSONL si può disattivare in produzione?"* — No. È always-on in tutte e tre le modalità `enforce`. È una proprietà strutturale del runtime, non un'opzione di configurazione.

- *"Si può usare Aeris come linguaggio di scripting senza tutta la cerimonia?"* — Sì. `aeris init` di default crea un progetto con `enforce = "off"`. In quella modalità non c'è bisogno di `cap`, di `intent`, di `model@vN` con versione — il programma gira come uno script. La traccia e il replay restano comunque attivi.
