#!/bin/bash
# Kria Language Benchmark Suite
# Multi-run timing with warmup (bash + /usr/bin/time).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

KRIA_BINARY="./target/release/kria"
BENCH_DIR="$SCRIPT_DIR"
RESULTS_FILE="$SCRIPT_DIR/benchmark_results.txt"

WARMUP="${BENCH_WARMUP:-3}"
RUNS="${BENCH_RUNS:-10}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

if ! command -v bc >/dev/null 2>&1; then
    echo -e "${YELLOW}Warning: bc not found; floating-point stats may be limited.${NC}" >&2
fi

if [ ! -f "$KRIA_BINARY" ]; then
    echo -e "${YELLOW}Building release binary...${NC}"
    cargo build --release
fi
if [ ! -f "$KRIA_BINARY" ]; then
    echo -e "${RED}Error: $KRIA_BINARY not found${NC}" >&2
    exit 1
fi

# --- timing helpers (milliseconds) ---

run_kria_once_ms() {
    local bench_file=$1
    local elapsed
    if command -v /usr/bin/time >/dev/null 2>&1; then
        elapsed=$( { /usr/bin/time -f '%e' "$KRIA_BINARY" "$bench_file" >/dev/null; } 2>&1 | tail -1)
    else
        local start end
        start=$(date +%s%N)
        "$KRIA_BINARY" "$bench_file" >/dev/null
        end=$(date +%s%N)
        elapsed=$(awk "BEGIN { printf \"%.6f\", ($end - $start) / 1000000000 }")
    fi
    awk -v s="$elapsed" 'BEGIN { printf "%.2f", s * 1000 }'
}

run_kria_capture() {
    local bench_file=$1
    "$KRIA_BINARY" "$bench_file" 2>&1
    return $?
}

compute_stats() {
    local -n _vals=$1
    if [ ${#_vals[@]} -eq 0 ]; then
        echo "median=0 min=0 max=0 mean=0"
        return
    fi
    local sorted
    sorted=$(printf '%s\n' "${_vals[@]}" | sort -n)
    awk -v data="$sorted" '
    {
        split(data, a, "\n")
        n = 0
        for (i in a) if (a[i] != "") { n++; v[n] = a[i] + 0 }
        if (n == 0) { print "median=0 min=0 max=0 mean=0"; exit }
        sum = 0
        for (i = 1; i <= n; i++) sum += v[i]
        mean = sum / n
        if (n % 2 == 1) median = v[(n + 1) / 2]
        else median = (v[n / 2] + v[n / 2 + 1]) / 2
        printf "median=%.2f min=%.2f max=%.2f mean=%.2f", median, v[1], v[n], mean
    }'
}

run_benchmark() {
    local bench_file=$1
    local w r
    for ((w = 0; w < WARMUP; w++)); do
        run_kria_once_ms "$bench_file" >/dev/null || true
    done
    local -a samples=()
    for ((r = 0; r < RUNS; r++)); do
        local ms
        ms=$(run_kria_once_ms "$bench_file")
        samples+=("$ms")
    done
    compute_stats samples
}

write_header() {
    {
        echo "Kria Benchmark Results"
        echo "======================"
        echo "date: $(date -Iseconds 2>/dev/null || date)"
        if command -v git >/dev/null 2>&1 && git -C "$PROJECT_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
            echo "git: $(git -C "$PROJECT_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
        fi
        echo "rustc: $(rustc -V 2>/dev/null || echo n/a)"
        echo "cargo: $(cargo -V 2>/dev/null || echo n/a)"
        echo "system: $(uname -srmo 2>/dev/null || uname -a)"
        echo "binary: $KRIA_BINARY"
        if command -v md5sum >/dev/null 2>&1; then
            echo "binary_md5: $(md5sum "$KRIA_BINARY" | awk '{print $1}')"
        fi
        echo "warmup: $WARMUP"
        echo "runs: $RUNS"
        echo "timing_backend: bash (/usr/bin/time)"
        echo ""
        echo "Format: name | median=..ms min=.. max=.. mean=.. | exit=.. | output=.."
        echo ""
    } >"$RESULTS_FILE"
}

# --- main ---

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}    Kria Language Benchmark Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "Timing: ${GREEN}bash${NC} (warmup=$WARMUP, runs=$RUNS)"
echo ""

write_header

declare -a BENCH_NAMES=()
declare -a BENCH_STATS=()
declare -a BENCH_OUTPUTS=()
declare -a BENCH_EXITS=()

bench_count=0
while IFS= read -r bench_file; do
    bench_name=$(basename "$bench_file" .krx)
    BENCH_NAMES+=("$bench_name")

    echo -n "Running ${bench_name}... "

    output=$(run_kria_capture "$bench_file")
    exit_code=$?
    if [ "$exit_code" -ne 0 ]; then
        echo -e "${RED}FAILED${NC} (exit $exit_code)"
        stats="ERROR"
        BENCH_STATS+=("$stats")
        BENCH_OUTPUTS+=("$output")
        BENCH_EXITS+=("$exit_code")
        echo "${bench_name} | ERROR exit=${exit_code} | ${output}" >>"$RESULTS_FILE"
        bench_count=$((bench_count + 1))
        continue
    fi

    stats=$(run_benchmark "$bench_file")

    BENCH_STATS+=("$stats")
    BENCH_OUTPUTS+=("$output")
    BENCH_EXITS+=(0)

    median=$(echo "$stats" | sed -n 's/.*median=\([0-9.]*\).*/\1/p')
    echo -e "${GREEN}OK${NC} ${stats}ms (output: ${output})"
    echo "${bench_name} | ${stats}ms | exit=0 | output=${output}" >>"$RESULTS_FILE"
    bench_count=$((bench_count + 1))
done < <(find "$BENCH_DIR" -maxdepth 1 -name 'bench_*.krx' | sort)

echo ""
echo -e "${BLUE}════════════════════════════════════════${NC}"
echo -e "${BLUE}         Kria Benchmark Results${NC}"
echo -e "${BLUE}════════════════════════════════════════${NC}"
echo ""
printf "%-28s %10s %10s %10s %10s\n" "Test" "Median" "Min" "Max" "Mean"
printf "%-28s %10s %10s %10s %10s\n" "────────────────────────────" "──────────" "──────────" "──────────" "──────────"

total_median=0
valid_count=0
for i in "${!BENCH_NAMES[@]}"; do
    name="${BENCH_NAMES[$i]}"
    stats="${BENCH_STATS[$i]}"
    if [ "$stats" = "ERROR" ]; then
        printf "%-28s %10s\n" "$name" "ERROR"
        continue
    fi
    median=$(echo "$stats" | sed -n 's/.*median=\([0-9.]*\).*/\1/p')
    min=$(echo "$stats" | sed -n 's/.*min=\([0-9.]*\).*/\1/p')
    max=$(echo "$stats" | sed -n 's/.*max=\([0-9.]*\).*/\1/p')
    mean=$(echo "$stats" | sed -n 's/.*mean=\([0-9.]*\).*/\1/p')
    printf "%-28s %9sms %9sms %9sms %9sms\n" "$name" "$median" "$min" "$max" "$mean"
    if command -v bc >/dev/null 2>&1; then
        total_median=$(echo "$total_median + $median" | bc)
    else
        total_median=$(awk -v t="$total_median" -v m="$median" 'BEGIN { print t + m }')
    fi
    valid_count=$((valid_count + 1))
done

echo ""
printf "%-28s %10s\n" "Tests run" "$bench_count"
if [ "$valid_count" -gt 0 ]; then
    if command -v bc >/dev/null 2>&1; then
        avg_median=$(echo "scale=2; $total_median / $valid_count" | bc)
    else
        avg_median=$(awk -v t="$total_median" -v n="$valid_count" 'BEGIN { printf "%.2f", t / n }')
    fi
    printf "%-28s %9sms\n" "Sum of medians" "$total_median"
    printf "%-28s %9sms\n" "Avg median per test" "$avg_median"
fi

echo ""
echo -e "${BLUE}════════════════════════════════════════${NC}"
echo ""
echo -e "${GREEN}Results saved to: benchmarks/benchmark_results.txt${NC}"
