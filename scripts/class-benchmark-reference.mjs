// Frozen before either engine executes the authored class suite.
export const classFixtureVersion = 1;
export const classManifestSha256 = "13a897b0b38bdc9c5233c069d4e51d10b6481851bc1bae92bfbabafc8bdc3a97";
export const classReferenceRevision = "007556adccb8fb4329236932ca8175f8e8d119d2";
export const classWorkloads = [
  {
    "name": "class-construct",
    "iterations": 10000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "class Point { constructor(value) { this.value = value; } } function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let point = new Point(i); total += point.value; } return total; }",
    "invocation": "bench(10000);",
    "expected": 49995000,
    "file_sha256": "e9b06a3bf0fc4a584e5c9250a921cb670b520b8bff10901d5e2aeaa221849279"
  },
  {
    "name": "class-method",
    "iterations": 10000,
    "oracle": "n",
    "setup": "class Counter { constructor() { this.value = 0; } add(delta) { this.value += delta; return this.value; } } function bench(n) { let counter = new Counter(); for (let i = 0; i < n; i++) { counter.add(1); } return counter.value; }",
    "invocation": "bench(10000);",
    "expected": 10000,
    "file_sha256": "a244d6547f354e02a43ab37f8c48472ab5dca12b0c6fecb3e04b032b3254b841"
  },
  {
    "name": "class-static",
    "iterations": 100000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "class Accumulator { static total(n) { let sum = 0; for (let i = 0; i < n; i++) { sum += i; } return sum; } }",
    "invocation": "Accumulator.total(100000);",
    "expected": 4999950000,
    "file_sha256": "d06d2a6c10e48081fa1dac6bde586c8c59b37653d780b778405e787d0eff7f69"
  },
  {
    "name": "class-definition",
    "iterations": 1000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let Point = class { constructor(value) { this.value = value; } read() { return this.value; } }; total += new Point(i).read(); } return total; }",
    "invocation": "bench(1000);",
    "expected": 499500,
    "file_sha256": "d47289dc28c4ce5e615b6ab2fb4d29f5490fa2e34a1290df693dd4f1ad0a53ef"
  }
];
