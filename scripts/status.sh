#!/usr/bin/env bash

set -euo pipefail

main() {
    curl -s 127.0.0.1:8000/pg/instance | jq .
}

main "$@"
