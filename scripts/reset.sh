#!/usr/bin/env bash

set -euo pipefail

log() {
    echo "$(date +"%H:%M:%S") - $(printf '%s' "$@")" 1>&2
}

main() {
    log "Loading instances"
    local instances=$(curl -fsS 127.0.0.1:8000/pg/instance | jq -r '.instances[] | .id')
    local running=$(curl -fsS 127.0.0.1:8000/pg/instance | jq -r '.instances[] | select(.proc_info | .!=null) | .id')

    for id in $running
    do
        log "Stop ${id}"
        curl -fsS -XPOST "127.0.0.1:8000/pg/instance/${id}/stop" > /dev/null
    done

    for id in $instances
    do
        log "Delete ${id}"
        curl -fsS -XDELETE "127.0.0.1:8000/pg/instance/${id}" > /dev/null
    done
}

main "$@"
