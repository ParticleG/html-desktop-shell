#!/usr/bin/env bash
set -euo pipefail

project_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin=${HTML_DESKTOP_SHELL_BIN:-"$project_root/target/debug/html-desktop-shell"}
config=${HTML_DESKTOP_SHELL_CONFIG:-"$project_root/test/panel-default.toml"}
poll_interval=${HTML_DESKTOP_SHELL_SMOKE_DELAY:-0.25}
smoke_timeout=${HTML_DESKTOP_SHELL_SMOKE_TIMEOUT:-15}
validator="$project_root/scripts/niri-layer-smoke.ts"

fail() {
  echo "niri layer smoke: $*" >&2
  exit 1
}

[[ -x "$bin" ]] || fail "missing executable: $bin (run cargo build first or set HTML_DESKTOP_SHELL_BIN)"
[[ -r "$validator" ]] || fail "missing validator: $validator"
command -v niri >/dev/null 2>&1 || fail "required executable not found: niri"
command -v bun >/dev/null 2>&1 || fail "required executable not found: bun"
command -v setsid >/dev/null 2>&1 || fail "required executable not found: setsid"
command -v timeout >/dev/null 2>&1 || fail "required executable not found: timeout"
[[ "$smoke_timeout" =~ ^[1-9][0-9]*$ ]] ||
  fail "HTML_DESKTOP_SHELL_SMOKE_TIMEOUT must be a positive integer"
[[ "$poll_interval" =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]] ||
  fail "HTML_DESKTOP_SHELL_SMOKE_DELAY must be a non-negative number"

args=("$bin")
if [[ -f "$config" ]]; then
  args+=(--config "$config")
fi

if ! check_output=$("${args[@]}" --check 2>&1); then
  printf '%s\n' "$check_output" >&2
  fail "binary --check failed"
fi
monitor_count=
while IFS= read -r line; do
  case "$line" in
    "monitor_count = "*)
      monitor_count=${line#"monitor_count = "}
      ;;
  esac
done <<<"$check_output"
[[ "$monitor_count" =~ ^[1-9][0-9]*$ ]] ||
  fail "binary --check did not print a positive monitor_count (output: $check_output)"

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/html-desktop-shell-smoke.XXXXXX")
pid=
pgid=
cleanup() {
  if [[ -n "$pgid" ]] && kill -0 -- "-$pgid" 2>/dev/null; then
    kill -TERM -- "-$pgid" 2>/dev/null || true
    for _ in {1..20}; do
      kill -0 -- "-$pgid" 2>/dev/null || break
      sleep 0.05
    done
    if kill -0 -- "-$pgid" 2>/dev/null; then
      kill -KILL -- "-$pgid" 2>/dev/null || true
    fi
  fi
  if [[ -n "$pid" ]]; then
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

outputs_json="$tmp_dir/outputs.json"
layers_json="$tmp_dir/layers.json"
diagnostics="$tmp_dir/validator.txt"
app_log="$tmp_dir/app.log"
fail_if_app_exited() {
  if kill -0 "$pid" 2>/dev/null; then
    return
  fi
  status=0
  wait "$pid" || status=$?
  pid=
  cat "$app_log" >&2
  fail "application exited before layer assertions passed (status $status)"
}


now_microseconds() {
  local seconds=${EPOCHREALTIME%.*}
  local fraction=${EPOCHREALTIME#*.}
  fraction=${fraction}000000
  fraction=${fraction:0:6}
  printf '%d\n' "$((10#$seconds * 1000000 + 10#$fraction))"
}

remaining_timeout() {
  local remaining_us=$((deadline_us - $(now_microseconds)))
  (( remaining_us > 0 )) || return 1
  printf '%d.%06d\n' "$((remaining_us / 1000000))" "$((remaining_us % 1000000))"
}

run_with_deadline() {
  local stdout=$1
  local stderr=$2
  shift 2
  local duration
  duration=$(remaining_timeout) || return 124
  timeout --signal=TERM --kill-after=0.1s "${duration}s" "$@" >"$stdout" 2>"$stderr"
}

deadline_expired() {
  (( $(now_microseconds) >= deadline_us ))
}

if ! niri msg -j outputs >"$outputs_json"; then
  fail "failed to obtain niri outputs JSON"
fi

setsid --wait "${args[@]}" >"$app_log" 2>&1 &
pid=$!
pgid=$pid
deadline=$((SECONDS + smoke_timeout))
deadline_us=$(($(now_microseconds) + smoke_timeout * 1000000))

while (( SECONDS < deadline )); do
  fail_if_app_exited

  layers_status=0
  run_with_deadline "$layers_json" "$tmp_dir/niri-layers-error.txt" niri msg -j layers || layers_status=$?
  if (( layers_status == 0 )); then
    validator_status=0
    run_with_deadline "$diagnostics" "$diagnostics.stderr" bun "$validator" \
      --outputs "$outputs_json" \
      --layers "$layers_json" \
      --monitor-count "$monitor_count" || validator_status=$?
    if [[ -s "$diagnostics.stderr" ]]; then
      cat "$diagnostics.stderr" >>"$diagnostics"
    fi
    if (( validator_status == 0 )); then
      if deadline_expired; then
        printf 'validator passed after the absolute smoke deadline\n' >"$diagnostics"
        break
      fi
      fail_if_app_exited
      cat "$diagnostics"
      exit 0
    fi
    if (( validator_status == 124 || validator_status == 137 )); then
      printf 'validator timed out at the absolute smoke deadline\n' >"$diagnostics"
      break
    fi
    if (( validator_status == 2 )); then
      cat "$diagnostics" >&2
      exit 2
    fi
  elif (( layers_status == 124 || layers_status == 137 )); then
    printf 'niri layers query timed out at the absolute smoke deadline\n' >"$diagnostics"
    break
  else
    cp "$tmp_dir/niri-layers-error.txt" "$diagnostics"
  fi

  remaining=$((deadline - SECONDS))
  (( remaining > 0 )) || break
  sleep_for=$poll_interval
  delay_whole=${poll_interval%%.*}
  delay_whole=${delay_whole:-0}
  delay_whole=$((10#$delay_whole))
  if (( delay_whole >= remaining )); then
    sleep_for=$remaining
  fi
  sleep "$sleep_for"
done

fail_if_app_exited

echo "niri layer smoke: timed out after ${smoke_timeout}s; final validator diagnostics:" >&2
cat "$diagnostics" >&2
if [[ -s "$app_log" ]]; then
  echo "application output:" >&2
  cat "$app_log" >&2
fi
exit 1
