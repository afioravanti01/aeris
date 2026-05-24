#!/usr/bin/env bash
#
# Exercise the webhook router with a handful of representative
# payloads, plus a few negative cases. Each call prints the
# request, the HTTP status, and the JSON reply on one line.
#
# Usage:
#   ./test.sh                # talks to http://localhost:8080
#   ./test.sh 9090           # talks to http://localhost:9090
#
# Pre-requisite: `aeris run ./main.aer [<port>]` running in another
# terminal.

set -u
PORT="${1:-8080}"
BASE="http://localhost:${PORT}"
# Use a marker that survives word-splitting and quoting, with no
# embedded escape sequences (avoids the `\n → n` trap of `eval` +
# `-w '\n...'`). The marker is unique enough not to clash with
# any JSON the server might emit.
MARK="__AERIS_HTTP_STATUS__"

# ─── helpers ──────────────────────────────────────────────────────

# Pretty-print one HTTP call.
#   $1   : short label
#   $2.. : curl args (passed verbatim, no `eval`)
hit() {
  local label="$1"
  shift
  printf "\n── %s ──\n" "$label"
  # Best-effort preview of the request: bare args when they're
  # shell-safe, single-quoted otherwise. Purely cosmetic — the
  # actual call below uses the array verbatim.
  printf "  > curl"
  for a in "$@"; do
    if [[ "$a" =~ ^[A-Za-z0-9._:/=@-]+$ ]]; then
      printf " %s" "$a"
    else
      printf " '%s'" "$a"
    fi
  done
  printf "\n"
  local raw status body
  raw=$(curl -sS -o - -w "${MARK}%{http_code}" "$@" 2>&1)
  status="${raw##*${MARK}}"
  body="${raw%${MARK}*}"
  printf "  < %s\n" "$status"
  if [ -n "$body" ]; then
    printf "    %s\n" "$body"
  fi
}

post_ticket() {
  local label="$1"
  local payload="$2"
  hit "$label" \
    -X POST "${BASE}/ticket" \
    -H "Content-Type: application/json" \
    --data "${payload}"
}

# ─── 1. health probe ──────────────────────────────────────────────

hit "GET /health" "${BASE}/health"

# ─── 2. happy-path tickets ────────────────────────────────────────
# The AI triager should classify each ticket into one of:
#   billing | bug | feature | spam

post_ticket "POST /ticket  (billing-shaped)" \
  '{"subject":"refund my last invoice","body":"I was double-charged in March","sender":"alice@acme.io"}'

post_ticket "POST /ticket  (bug-shaped)" \
  '{"subject":"500 on /v1/orders","body":"The orders endpoint returns 500 since this morning","sender":"ops@retail.example"}'

post_ticket "POST /ticket  (feature-shaped)" \
  '{"subject":"dark mode please","body":"Would love a system-aware dark theme","sender":"happy.user@example.com"}'

post_ticket "POST /ticket  (spam-shaped)" \
  '{"subject":"Earn $$$ from home","body":"Click this link to triple your income","sender":"noreply@suspicious.tld"}'

# ─── 3. negative cases ────────────────────────────────────────────

hit "GET /ticket  (wrong method → 405)" \
  -X GET "${BASE}/ticket"

post_ticket "POST /ticket  (empty body → 400)" ""

hit "GET /does-not-exist  (404)" "${BASE}/does-not-exist"

printf "\n"
