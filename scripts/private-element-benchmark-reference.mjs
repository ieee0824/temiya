// Inputs frozen before implementation; no workload changes after timing.
export const classFixtureKind = 'authored-private-class-elements';
export const classFixtureVersion = 1;
export const classManifestSha256 = 'dfd5e4d026f3307d76e6fe150ef700ab5faf58e9b2a03bcadf9fb003eb97d0c4';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "private-method-call",
    "setup": "class C{#m(v){return v+1;}call(v){return this.#m(v);}}function bench(n){let c=new C();let total=0;for(let i=0;i<n;i++){total+=c.call(i);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "f50546e20deeef64c567880264c5d01807d37b62940b3cdefd98a92e9985ffb1",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "private-accessor",
    "setup": "class C{#x=0;get #v(){return this.#x;}set #v(v){this.#x=v;}put(v){this.#v=v;return this.#v;}}function bench(n){let c=new C();let total=0;for(let i=0;i<n;i++){total+=c.put(i+1);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "ea9be8ce2d0808aa0a6930f4076d8a13eb87bd52806851d5a79f42fb45b26d35",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "static-private-field",
    "setup": "class C{static #x;static put(v){this.#x=v;return this.#x;}}function bench(n){let total=0;for(let i=0;i<n;i++){total+=C.put(i+1);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "23b8ff3317a81006e25a36da4fc5793f8d80971066c23047c52f75d1d8616054",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "static-private-method",
    "setup": "class C{static #m(v){return v+1;}static call(v){return this.#m(v);}}function bench(n){let total=0;for(let i=0;i<n;i++){total+=C.call(i);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "6a1faf6bf6e5a5a86383494aadc77284cf4e60cae36ceabd81b1635d95d43714",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "static-private-accessor",
    "setup": "class C{static #x=0;static get #v(){return this.#x;}static set #v(v){this.#x=v;}static put(v){this.#v=v;return this.#v;}}function bench(n){let total=0;for(let i=0;i<n;i++){total+=C.put(i+1);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "ed9fd6a2ebd7682f8d69e3a64102962354ca1b393d60cba1cf73b449b8db5895",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "private-definition-brand",
    "setup": "function bench(n){let total=0;for(let i=0;i<n;i++){class C{#m(){}has(o){return #m in o;}static #x;static has(o){return #x in o;}}let c=new C();if(!c.has(c)||c.has({})||!C.has(C)||C.has(c))throw 42;total+=i+1;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "2e03c20e1955da3dffcbebed574c9f63e0693c4bcf3da5431907c15be732777f",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
