import sys
from time import perf_counter_ns


def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)


def run(workload, n):
    if workload == "arithmetic":
        result = 0
        for i in range(n):
            result = result + i * 3 + 1
        return result
    if workload == "branching":
        count = 0
        for i in range(n):
            count += 1 if i < n / 2 else 2
        return count
    if workload == "nested-loops":
        total = 0
        for i in range(n):
            for j in range(n):
                total = total + i + j
        return total
    if workload == "function-calls":
        def add(a, b):
            return a + b

        total = 0
        for i in range(n):
            total += add(i, 3)
        return total
    if workload == "recursion":
        total = 0
        for i in range(n):
            total += fibonacci(i)
        return total
    if workload == "closures":
        def create_multiplier(factor):
            return lambda value: value * factor

        multiply = create_multiplier(3)
        total = 0
        for i in range(n):
            total += multiply(i)
        return total
    if workload == "arrays":
        values = []
        for i in range(n):
            values.append(i)
        total = 0
        for value in values:
            total += value
        return total
    if workload == "objects":
        item = {"a": 0, "b": 1, "c": 3}
        total = 0
        for _ in range(n):
            item["a"] += 1
            item["b"] += 1
            total += item["a"] + item["b"] + item["c"]
        return total
    if workload == "strings":
        text = ""
        for _ in range(n):
            text += "x"
        return 0 if text == "" else n
    raise ValueError(f"Unknown workload: {workload}")


if len(sys.argv) != 3:
    raise SystemExit("Usage: python.py <workload> <positive-iterations>")
iterations = int(sys.argv[2])
if iterations <= 0:
    raise SystemExit("iterations must be positive")
started = perf_counter_ns()
result = run(sys.argv[1], iterations)
elapsed_ns = perf_counter_ns() - started
print(result)
print(f"BENCH_EXECUTION_NS={elapsed_ns}", file=sys.stderr)
