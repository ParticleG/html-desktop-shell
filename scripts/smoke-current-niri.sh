#!/usr/bin/env bash
set -euo pipefail

project_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin=${HTML_DESKTOP_SHELL_BIN:-"$project_root/target/debug/html-desktop-shell"}
config=${HTML_DESKTOP_SHELL_CONFIG:-"$project_root/test/panel-default.toml"}
startup_delay=${HTML_DESKTOP_SHELL_SMOKE_DELAY:-5}

if [[ ! -x "$bin" ]]; then
  echo "missing executable: $bin" >&2
  echo "run cargo build first or set HTML_DESKTOP_SHELL_BIN" >&2
  exit 1
fi

args=("$bin")
if [[ -f "$config" ]]; then
  args+=(--config "$config")
fi

"${args[@]}" &
pid=$!
cleanup() {
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid"
    wait "$pid" || true
  fi
}
trap cleanup EXIT INT TERM

sleep "$startup_delay"
niri msg -j layers
