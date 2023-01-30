#!/usr/bin/env bash

set -euo pipefail

log() {
    echo "$(date +"%H:%M:%S") - $(printf '%s' "$@")" 1>&2
}

main() {
    log "Initial state"
    curl -fsS 127.0.0.1:8000 | jq .

    log "Create instance"
    local template=$(curl -fsS -XPOST -d "{\"dbname\": \"example\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/create | jq -r '.id')
    log "  > created: ${template}"

    log "Stop instance"
    curl -fsS -XPOST -d "{\"id\": \"${template}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/stop > /dev/null

    log "Fork instance"
    local target=$(curl -fsS -XPOST -d "{\"id\": \"${template}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/fork | jq -r '.id')
    log "  > created: ${target}"

    log "Start instance"
    curl -sS --fail-with-body -XPOST -d "{\"id\": \"${target}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/start | jq .
}

main "$@"
