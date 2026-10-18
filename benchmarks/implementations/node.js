function fibonacci(n) {
  if (n <= 1) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

function run(workload, n) {
  switch (workload) {
    case "arithmetic": {
      let result = 0;
      for (let i = 0; i < n; i++) result = result + i * 3 + 1;
      return result;
    }
    case "branching": {
      let count = 0;
      for (let i = 0; i < n; i++) count += i < n / 2 ? 1 : 2;
      return count;
    }
    case "nested-loops": {
      let sum = 0;
      for (let i = 0; i < n; i++)
        for (let j = 0; j < n; j++) sum = sum + i + j;
      return sum;
    }
    case "function-calls": {
      const add = (a, b) => a + b;
      let sum = 0;
      for (let i = 0; i < n; i++) sum += add(i, 3);
      return sum;
    }
    case "recursion": {
      let sum = 0;
      for (let i = 0; i < n; i++) sum += fibonacci(i);
      return sum;
    }
    case "closures": {
      const createMultiplier = (factor) => (value) => value * factor;
      const multiply = createMultiplier(3);
      let sum = 0;
      for (let i = 0; i < n; i++) sum += multiply(i);
      return sum;
    }
    case "arrays": {
      const values = [];
      for (let i = 0; i < n; i++) values.push(i);
      let sum = 0;
      for (const value of values) sum += value;
      return sum;
    }
    case "objects": {
      const item = { a: 0, b: 1, c: 3 };
      let sum = 0;
      for (let i = 0; i < n; i++) {
        item.a++;
        item.b++;
        sum += item.a + item.b + item.c;
      }
      return sum;
    }
    case "strings": {
      let text = "";
      for (let i = 0; i < n; i++) text += "x";
      return text === "" ? 0 : n;
    }
    default:
      throw new Error(`Unknown workload: ${workload}`);
  }
}

const [workload, rawIterations] = process.argv.slice(2);
const iterations = Number(rawIterations);
if (!workload || !Number.isSafeInteger(iterations) || iterations <= 0) {
  throw new Error("Usage: node.js <workload> <positive-iterations>");
}
const started = process.hrtime.bigint();
const result = run(workload, iterations);
const elapsedNs = process.hrtime.bigint() - started;
console.log(result);
console.error(`BENCH_EXECUTION_NS=${elapsedNs}`);
