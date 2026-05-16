# Agent e agent_net

## agent

Un `agent` è un'unità LLM tipata:

```aeris
agent classify {
  llm:     "claude-sonnet-4-6"
  intent:  "Triage the invoice into one of four categories"
  prompt:  "Reply with a Category@v1 JSON object."
  accept:  Invoice@v1
  produce: Category@v1
  retries: 2
  budget:  { tokens: 2000, latency: 3s }
}
```

`accept` e `produce` sono `model@vN` validati a ogni invocazione.
`budget:` limita tokens e latency per call: sforare → `BudgetExceeded`.
Ogni chiamata viene registrata nel trace come `ai_call` con prompt,
modello, risposta, tokens.

## agent_net

Un DAG aciclico di agenti, con routing risolto per match
`accept` ↔ `produce`:

```aeris
agent_net review_loop {
  flow extract -> critique
  until: critique.ok == true or iterations >= 3
}
```

Cicli rifiutati al parse (E70). Composizione: una net può essere
usata come nodo di un'altra. Il protocollo di routing è iniettato dal
runtime nel system prompt — non si scrive a mano.
