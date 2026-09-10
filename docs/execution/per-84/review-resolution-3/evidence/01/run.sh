#!/usr/bin/env bash
set -u

label=$1
shift
log="$(dirname "$0")/${label}.log"
set +e
{
    printf 'COMMAND:'
    printf ' %q' "$@"
    printf '\nCWD: %s\n' "$PWD"
    "$@"
    status=$?
    printf 'COMMAND_EXIT: %s\n' "$status"
    exit "$status"
} 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
exit "$status"
