#!/usr/bin/env bash
set -euo pipefail

cd "$HOME/html-desktop-shell"

app=${HTML_DESKTOP_SHELL_BIN:-"./target/debug/html-desktop-shell"}
config=${HTML_DESKTOP_SHELL_CONFIG:-"./test/panel-default.toml"}
log=/tmp/html-desktop-shell-kvm-measure.log

# Avoid measuring an app instance spawned by niri config.
pkill -f 'target/debug/html-desktop-shell' 2>/dev/null || true
sleep 1

"$app" --config "$config" >"$log" 2>&1 &
pid=$!

cleanup() {
 if kill -0 "$pid" 2>/dev/null; then
   kill "$pid" 2>/dev/null || true
   wait "$pid" 2>/dev/null || true
 fi
}
trap cleanup EXIT INT TERM

start_ns=$(date +%s%N)
visible_ns=""
deadline=$((SECONDS + 30))

while (( SECONDS < deadline )); do
 if ! kill -0 "$pid" 2>/dev/null; then
   echo "html-desktop-shell exited before panel became visible"
   echo "--- app log ---"
   cat "$log"
   exit 1
 fi

 layers=$(niri msg -j layers 2>/dev/null || true)
 case "$layers" in
   *html-desktop-shell-panel*)
     visible_ns=$(date +%s%N)
     break
     ;;
 esac

 sleep 0.2
done

if [[ -z "$visible_ns" ]]; then
 echo "panel layer did not become visible within 30s"
 echo "--- app log ---"
 cat "$log"
 exit 1
fi

startup_s=$(awk -v a="$start_ns" -v b="$visible_ns" 'BEGIN { printf "%.3f", (b - a) / 1000000000 }')

clk_tck=$(getconf CLK_TCK)

read_ticks() {
 awk '{ print $14 + $15 }' "/proc/$1/stat"
}

read_rss_kib() {
 awk '/^VmRSS:/ { print $2 }' "/proc/$1/status"
}

# Start idle window after the panel is visible.
t1_ns=$(date +%s%N)
ticks1=$(read_ticks "$pid")

sleep 60

t2_ns=$(date +%s%N)
ticks2=$(read_ticks "$pid")
rss_kib=$(read_rss_kib "$pid")

idle_cpu_percent=$(awk \
 -v dticks="$((ticks2 - ticks1))" \
 -v hz="$clk_tck" \
 -v a="$t1_ns" \
 -v b="$t2_ns" \
 'BEGIN {
   elapsed = (b - a) / 1000000000
   printf "%.3f", (dticks / hz) / elapsed * 100
 }'
)

rss_mib=$(awk -v rss="$rss_kib" 'BEGIN { printf "%.1f", rss / 1024 }')

echo "startup_until_first_visible_panel_s=$startup_s"
echo "idle_cpu_over_60s_percent=$idle_cpu_percent"
echo "rss_after_60s_kib=$rss_kib"
echo "rss_after_60s_mib=$rss_mib"
echo "--- niri layers ---"
niri msg -j layers