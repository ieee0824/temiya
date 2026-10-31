// Inputs frozen and Boa oracles checked before issue157 runtime implementation.
export const classFixtureKind = "authored-instance-fields";
export const classFixtureVersion = 1;
export const classManifestSha256 = "278d01437abf4a0af9ce766e73ac34836ad276f16451828c772f6dcd76a536ca";
export const classReferenceRevision = "007556adccb8fb4329236932ca8175f8e8d119d2";
export const classWorkloads = [
  {
    "name": "field-base",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let serial=0;class C{value=++serial;}let total=0;for(let i=0;i<n;i++)total+=new C().value;if(serial!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "bf8d19a411a80f51561a6638d7c8aee9258be76dc24c6259b6ead6dd796a5b90"
  },
  {
    "name": "field-derived",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){class A{constructor(value){this.seed=value;}}class C extends A{value=this.seed;}let total=0;for(let i=1;i<=n;i++)total+=new C(i).value;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "9e81a5d39a589efc7c677f4e6f2cb6b43f899f937349ceee87d9e5344cadec15"
  },
  {
    "name": "field-computed-key",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let keys=0;let serial=0;class C{[(keys++,\"value\")]=++serial;}let total=0;for(let i=0;i<n;i++)total+=new C().value;if(keys!==1||serial!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "e909330f460273ed9f8967bf836d41f4c2de93afc35610e34c603e12d6ad116e"
  },
  {
    "name": "field-define-own",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let serial=0;class A{set value(value){throw 42;}}class C extends A{value=++serial;}let total=0;for(let i=0;i<n;i++)total+=new C().value;if(serial!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "32873ff7c80431b8fe1ee2f7fc430448ac69ee37a82b86be265740dbd859e116"
  },
  {
    "name": "field-arrow-initializer",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let serial=0;class C{value=++serial;read=()=>this.value;}let first;let last;let total=0;for(let i=0;i<n;i++){let object=new C();if(i===0)first=object;last=object;total+=object.read();}if(first.read()!==1||last.read()!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "a4a9e08caf8200fd6a486d9f5b8e6334811a15c85fbf543b1528516bf4875cc1"
  }
];
