// Frozen before the authored derived suite executes in either engine.
export const classFixtureKind = "authored-derived-classes";
export const classFixtureVersion = 1;
export const classManifestSha256 = "9d9f10054b33c7a8da0aec3bfa0c708ab5027cd29e4d1f7f7c6dcaa946cbb2cf";
export const classReferenceRevision = "007556adccb8fb4329236932ca8175f8e8d119d2";
export const classWorkloads = [
  {
    "name": "derived-explicit-construct",
    "iterations": 10000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "class Base { constructor(value) { this.value = value; } } class Derived extends Base { constructor(value) { super(value); } } function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let point = new Derived(i); total += point.value; } return total; }",
    "invocation": "bench(10000);",
    "expected": 49995000,
    "file_sha256": "cfc9d3b342c2c2d3186a9e2091c20aefe9de1f98d34e260eabfd3867ed61f65a"
  },
  {
    "name": "derived-default-construct",
    "iterations": 10000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "class Base { constructor(value) { this.value = value; } } class Derived extends Base {} function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let point = new Derived(i); total += point.value; } return total; }",
    "invocation": "bench(10000);",
    "expected": 49995000,
    "file_sha256": "241f767be745158830788224fda75a2116cdad4613446b83d95742f38d6835f7"
  },
  {
    "name": "derived-inherited-method",
    "iterations": 10000,
    "oracle": "n",
    "setup": "class Base { constructor() { this.value = 0; } add(delta) { this.value += delta; return this.value; } } class Counter extends Base {} function bench(n) { let counter = new Counter(); for (let i = 0; i < n; i++) { counter.add(1); } return counter.value; }",
    "invocation": "bench(10000);",
    "expected": 10000,
    "file_sha256": "5381b01db2e1c0cb6713339c8c9f15a629a33ce2ce325b16e88d1341bb28c7bb"
  },
  {
    "name": "derived-default-chain",
    "iterations": 10000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "class Base { constructor(value) { this.value = value; } } class Middle extends Base {} class Derived extends Middle {} function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let point = new Derived(i); total += point.value; } return total; }",
    "invocation": "bench(10000);",
    "expected": 49995000,
    "file_sha256": "b36253ca9da7c32d2b20623fb191d7448982f7e5aa155a8c45b1f11e98da1729"
  },
  {
    "name": "derived-definition",
    "iterations": 1000,
    "oracle": "sum-zero-to-n-minus-one",
    "setup": "function bench(n) { let total = 0; for (let i = 0; i < n; i++) { let Base = class { constructor(value) { this.value = value; } }; let Derived = class extends Base { constructor(value) { super(value); } read() { return this.value; } }; total += new Derived(i).read(); } return total; }",
    "invocation": "bench(1000);",
    "expected": 499500,
    "file_sha256": "b4f78e37d3b18741e191105675aa91730458e0b7010646241c8e29879272ad74"
  }
];
