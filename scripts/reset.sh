#!/usr/bin/env bash

set -euo pipefail

log() {
    echo "$(date +"%H:%M:%S") - $(printf '%s' "$@")" 1>&2
}

main() {
    log "Loading instances"
    local instances=$(curl -s localhost:8000 | jq -r '.statuses[] | select(.[1] | .!=null) | .[0]')

    for instance in $instances
    do
        log "Stop ${instance}"
        curl -fsS -XPOST -d "{\"name\": \"${instance}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/stop > /dev/null
    done

    log "Delete instances"
    rm -rf data/*
    rm -rf logs/*
}

main "$@"
