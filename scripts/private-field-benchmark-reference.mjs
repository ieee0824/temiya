// Private field inputs frozen before first candidate timing; actual Boa oracles checked.
export const classFixtureKind = 'authored-private-instance-fields';
export const classFixtureVersion = 1;
export const classManifestSha256 = '73d6796fc6c1c7ce043c18d10c6556005f2d4e6fa049a55a971457875f5cff79';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "private-construct",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "class Box{#value;constructor(v){this.#value=v;}read(){return this.#value;}}function bench(n){let total=0;for(let i=0;i<n;i++){let box=new Box(i+1);total+=box.read();}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "c4000037e0dc4d2404bda221e93581e98f3552c58e0675aab1a64df7e7559299"
  },
  {
    "name": "private-update",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "class Counter{#value=0;next(){return ++this.#value;}}function bench(n){let total=0;let counter=new Counter();for(let i=0;i<n;i++){total+=counter.next();}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "ef50a1f0ba8b4487d6f917f1d81037fd2ab93cc309d51bf826814297cd457a34"
  },
  {
    "name": "private-brand",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "class Box{#value;static owns(o){return #value in o;}}function bench(n){let total=0;let good=new Box();let bad=Object.create(good);for(let i=0;i<n;i++){total+=Box.owns(good)&&!Box.owns(bad)?i+1:0;}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "65f5e7847198f6af1e7e986dba0bbd6cae522b1f1b32a355d0efb79ce28c8774"
  },
  {
    "name": "private-lexical",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "class Counter{#value=0;reader(){return ()=>++this.#value;}}function bench(n){let total=0;let next=new Counter().reader();for(let i=0;i<n;i++){total+=next();}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "a38c526c1e13371216c238adae7ee839d0854fc39849214eaeffaa45a6acb949"
  },
  {
    "name": "private-invalid-receiver",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "class Box{#value;constructor(v){this.#value=v;}read(o){return o.#value;}}function bench(n){let total=0;for(let i=0;i<n;i++){let box=new Box(i+1);try{total+=box.read(i===n-1?{}:box);}catch(e){if(!(e instanceof TypeError))throw e;total+=n;}}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "0ec064a5157b753b3a5d23d2f4e2c9f50d5d5b83a29ab71036380a0174b5b61f"
  }
];
