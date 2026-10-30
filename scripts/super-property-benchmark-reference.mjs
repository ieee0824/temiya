// Fixed before issue156 implementation; actual Boa oracles validated separately.
export const classFixtureKind = "authored-super-properties";
export const classFixtureVersion = 3;
export const classManifestSha256 = "1c88d82040e01baffa30ddde304cab12abeb332a9bf5ca628851927282fbc806";
export const classReferenceRevision = "007556adccb8fb4329236932ca8175f8e8d119d2";
export const classWorkloads = [
  {
    "name": "super-object-call",
    "iterations": 10000,
    "oracle": "sum-one-to-n",
    "setup": "function bench(n) { let base = { step() { this.value += 1; return this.value; } }; let object = { __proto__: base, value: 0, step() { return super.step(); } }; let total = 0; for (let i = 0; i < n; i++) total += object.step(); return total; }",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "201dbd6b077f3536c8efde96c2f72813476ce469547cf629673083608d1e2832"
  },
  {
    "name": "super-class-call",
    "iterations": 10000,
    "oracle": "sum-one-to-n",
    "setup": "class Base { constructor() { this.value = 0; } step() { this.value += 1; return this.value; } } class Derived extends Base { step() { return super.step(); } } function bench(n) { let object = new Derived(); let total = 0; for (let i = 0; i < n; i++) total += object.step(); return total; }",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "0043545b5b1f34138ee0236b71f4da6e86b783159910870659ba18759e642545"
  },
  {
    "name": "super-accessor-receiver",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-receiver-state",
    "setup": "function bench(n) { let base = { get value() { return this.stored; }, set value(value) { this.stored = value; } }; let object = { __proto__: base, stored: 0, step() { let next = super.value + 1; super.value = next; return next; } }; let total = 0; for (let i = 0; i < n; i++) total += object.step(); if (object.stored !== n || base.stored !== undefined) throw 42; return total; }",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "db39f177da99948a2babaebbf24a1dc360aaf2df80be1e37dc4cea9da5e72d31"
  },
  {
    "name": "super-computed-update",
    "iterations": 10000,
    "oracle": "sum-one-to-n-single-key-expression-evaluation-and-receiver-state",
    "setup": "function bench(n) { let calls = 0; let base = { get value() { return this.stored; }, set value(value) { this.stored = value; } }; let object = { __proto__: base, stored: 0, step() { return ++super[(calls++, 'value')]; } }; let total = 0; for (let i = 0; i < n; i++) total += object.step(); if (calls !== n || object.stored !== n || base.stored !== undefined) throw 42; return total; }",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "8e5f3d1b8b8bb74ced965650cd13280c12bf92d595f75c28cfdbaf7bcd478cc1"
  },
  {
    "name": "super-arrow-dynamic-parent",
    "iterations": 10000,
    "oracle": "half-n-times42-plus-half-n-times84",
    "setup": "function bench(n) { let base = { read() { return this.value; } }; let next = { read() { return this.value * 2; } }; let object = { __proto__: base, value: 42, make() { return () => super.read(); } }; let read = object.make(); let total = 0; for (let i = 0; i < n; i++) { if (i === n / 2) Object.setPrototypeOf(object, next); total += read(); } return total; }",
    "invocation": "bench(10000);",
    "expected": 630000,
    "file_sha256": "78dea8fb4fa8bb177ed5f8ee668e6877af4e116c1e1291d43e7f6674cc322221"
  }
];
