#!/bin/sh
# Runs every measurement, each in a fresh kitty window of the expected size.
# Results land in results/<date>-<commit>/ (see README.md).
#
# Environment overrides, handy for a quick check of the tool itself:
#   BENCH_RUNS, BENCH_LIVE_SECONDS, BENCH_COST_SECONDS, BENCH_WARMUP
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
runs=${BENCH_RUNS:-3}
live_seconds=${BENCH_LIVE_SECONDS:-120}
cost_seconds=${BENCH_COST_SECONDS:-30}
warmup=${BENCH_WARMUP:-5}

command -v kitty >/dev/null || {
    echo "kitty is required" >&2
    exit 1
}

CARGO_TARGET_DIR="$here/target" cargo build --release --manifest-path "$here/Cargo.toml"
bench="$here/target/release/redraw_bench"

commit=$(git -C "$here" rev-parse --short HEAD)
out="$here/results/$(date +%Y-%m-%d_%H%M%S)-$commit"
mkdir -p "$out"

power=$(cat /sys/class/power_supply/*/status 2>/dev/null | head -n 1 || true)
governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || true)
{
    echo "commit: $(git -C "$here" rev-parse HEAD)"
    echo "uncommitted changes outside this tool: $(git -C "$here" status --porcelain -- ../../crates | wc -l)"
    echo "cpu: $(grep -m 1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')"
    echo "memory: $(free -h | awk '/^Mem:/ {print $2}')"
    echo "kernel: $(uname -r)"
    echo "kitty: $(kitty --version)"
    echo "power: ${power:-unknown}"
    echo "governor: ${governor:-unknown}"
    echo "rustc: $(rustc --version)"
} >"$out/machine.txt"

if [ "$power" = "Discharging" ]; then
    echo "warning: running on battery, the numbers will not be comparable" >&2
fi

# Run one measurement in a new window. The default kitty settings are used, so
# that a personal configuration does not change what the terminal costs.
measure() {
    mode=$1 scenario=$2 cols=$3 rows=$4 seconds=$5 run=$6
    echo "$mode $scenario ${cols}x$rows, run $run/$runs"
    kitty --config NONE \
        -o remember_window_size=no \
        -o "initial_window_width=${cols}c" \
        -o "initial_window_height=${rows}c" \
        "$bench" "$mode" "$scenario" --cols "$cols" --rows "$rows" \
        --seconds "$seconds" --warmup "$warmup" --run "$run" --out "$out"
}

run=1
while [ "$run" -le "$runs" ]; do
    for size in "80 24" "200 50"; do
        # shellcheck disable=SC2086 # the split into two arguments is the point
        set -- $size
        for scenario in idle typical small full many; do
            measure live "$scenario" "$1" "$2" "$live_seconds" "$run"
        done
        for scenario in small full many; do
            measure cost "$scenario" "$1" "$2" "$cost_seconds" "$run"
        done
    done
    run=$((run + 1))
done

if [ -s "$out/errors.log" ]; then
    echo "some measurements failed, see $out/errors.log:" >&2
    cat "$out/errors.log" >&2
    exit 1
fi

echo "done: $out/results.csv"
