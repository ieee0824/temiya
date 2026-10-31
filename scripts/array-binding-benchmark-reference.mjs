// Manifest materialized from preimplementation hashes; workload bytes unchanged.
export const classFixtureVersion = 1;
export const classManifestSha256 = 'd42878d6ea48ccb018934f4fc3e7a0231ada65af42d0a970468344f3eb1dda12';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "array-binding-declaration",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let [value,[nested]]=[i,[i]];if(nested!==i)throw 42;total+=value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "990568bfeb2d2d66a211f318ce77874d4c4fe4808a7204ba68cb4be6dbb073a7",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "array-binding-parameters",
    "setup": "function read([first,second=first]){if(second!==first)throw 42;return first;}function bench(n){let total=0;for(let i=1;i<=n;i++)total+=read([i]);return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "492fe84f9743422793d99f5b4fe54f134e27442b3bc565e5955e8be1ae53189e",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "array-binding-elision-default",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let index=0;let input={[Symbol.iterator](){return {next(){index++;if(index===1)return {done:false,get value(){throw 42;}};return {done:index>2,value:undefined};}};}};let [,value=i]=input;total+=value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "7de19893b146bddd30b07f61c4c7c2257a9fd24440a051966e429b593255a92b",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "array-binding-rest",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let [first,...rest]=[i,i+1,i+2];if(!Array.isArray(rest)||rest.length!==2||rest[0]!==i+1||rest[1]!==i+2)throw 42;total+=first;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "d54824fa44ee1b706a75135fefd94205f784d62ff9f2aa673aca7f84b5efcb45",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "array-binding-close",
    "setup": "function bench(n){let total=0;let closes=0;for(let i=1;i<=n;i++){let iterator={next(){return {value:i,done:false};},return(){if(this!==iterator)throw 42;closes++;return {};}};let input={[Symbol.iterator](){return iterator;}};let [value]=input;total+=value;}if(closes!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "a4a133cea06cd892ea428ffd908660b5d1de849add0c76403846a7482d41f5c5",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "array-binding-tdz-close",
    "setup": "function bench(n){let total=0;let closes=0;for(let i=1;i<=n;i++){let iterator={next(){return {value:undefined,done:false};},return(){if(this!==iterator)throw 42;closes++;throw 99;}};let input={[Symbol.iterator](){return iterator;}};try{let [value=value]=input;throw 42;}catch(error){if(!(error instanceof ReferenceError))throw error;total+=i;}}if(closes!==n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "215022b13317a4b05c0894c0a7664d6180778a3be532471a92190019554536f7",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
