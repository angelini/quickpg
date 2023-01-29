#!/usr/bin/env bash

set -euo pipefail

log() {
    echo "$(date +"%H:%M:%S") - $(printf '%s' "$@")" 1>&2
}

main() {
    log "Loading instances"
    local instances=$(curl -s localhost:8000 | jq -r '.instances[] | .id')
    local running=$(curl -s localhost:8000 | jq -r '.instances[] | select(.proc_info | .!=null) | .id')

    for id in $running
    do
        log "Stop ${id}"
        curl -fsS -XPOST -d "{\"id\": \"${id}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/stop > /dev/null
    done

    for id in $instances
    do
        log "Delete ${id}"
        curl -fsS -XPOST -d "{\"id\": \"${id}\"}" -H 'Content-Type: application/json' 127.0.0.1:8000/destroy > /dev/null
    done
}

main "$@"
