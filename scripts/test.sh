#!/usr/bin/env bash

set -euo pipefail

log() {
    echo "$(date +"%H:%M:%S") - $(printf '%s' "$@")" 1>&2
}

main() {
    log "Initial state"
    curl -sS --fail-with-body 127.0.0.1:8000/pg/instance | jq .

    log "Create instance"
    local template=$(curl -fsS -XPOST -d "{\"dbname\": \"example\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/pg/instance | jq -r '.id')
    log "  > created: ${template}"

    log "Stop instance"
    curl -fsS -XPOST "127.0.0.1:8000/pg/instance/${template}/stop" > /dev/null

    log "Fork instance"
    local target=$(curl -fsS -XPOST 127.0.0.1:8000/pg/instance/${template}/fork | jq -r '.id')
    log "  > created: ${target}"

    log "Final state"
    curl -fsS 127.0.0.1:8000/pg/instance | jq .
}

main "$@"
