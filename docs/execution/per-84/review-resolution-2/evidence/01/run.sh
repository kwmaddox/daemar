#!/bin/sh
set +e
log=$1
shift
{
  printf 'COMMAND: '
  printf '%s ' "$@"
  printf '\nCWD: %s\n' "$(pwd)"
  "$@"
  status=$?
  printf 'COMMAND_EXIT=%s\n' "$status"
} >"$log" 2>&1
exit 0
