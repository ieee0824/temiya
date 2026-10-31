// Inputs frozen and actual Boa oracles validated before implementation/timing.
export const classFixtureVersion = 1;
export const classManifestSha256 = '39acacb064e3f488378b70cd748212e063291640594abec871c86e375da7cf4d';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "iterator-array-spread",
    "setup": "let iteratorInput=[];for(let i=0;i<10000;i++)iteratorInput[i]=i+1;function bench(n){let values=[...iteratorInput];let total=0;for(let i=0;i<n;i++)total+=values[i];return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "b8855d63acbfed803efae65a3204a07f8e3352c9b93dc270276f693ca6ca167e",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "iterator-call-spread",
    "setup": "let iteratorInput=[];for(let i=0;i<10000;i++)iteratorInput[i]=i+1;function sum(){'use strict';let total=0;for(let i=0;i<arguments.length;i++)total+=arguments[i];return total;}function bench(n){return sum(...iteratorInput);}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "98b64e9e8a0178ebb6656b33491c61edd08744b590e28f6fd7ea065465c9078a",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "iterator-captured-next",
    "setup": "function bench(n){let index=0;let reads=0;let record={get next(){reads++;return function(){index++;if(index===1)Object.defineProperty(record,'next',{value:function(){throw 42;}});return {done:index>n,value:index};};}};let iterable={[Symbol.iterator](){return record;}};let values=[...iterable];if(reads!==1)throw 42;let total=0;for(let i=0;i<n;i++)total+=values[i];return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "f04165986fe3658086154e18d318d83ac91fbcddeec5baa7d6d2d518c9d12844",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "iterator-string-codepoints",
    "setup": "let iteratorText='';for(let i=0;i<10000;i++)iteratorText+='😀';function bench(n){let values=[...iteratorText];if(values.length!==n)throw 42;let total=0;for(let i=0;i<n;i++){if(values[i]!=='😀'||values[i].length!==2)throw 42;total+=i+1;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "f1b237e8901c9b59f0abc93af8eeb3781e96f26cdf480e113a0e085af8714f68",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "iterator-done-value-order",
    "setup": "function bench(n){let index=0;let doneReads=0;let valueReads=0;let state=0;let iterable={[Symbol.iterator](){return {next(){let current=++index;return {get done(){if(state!==0)throw 42;state=1;doneReads++;return current>n;},get value(){if(state!==1)throw 42;state=0;valueReads++;return current;}};}};}};let values=[...iterable];if(doneReads!==n+1||valueReads!==n||state!==1)throw 42;let total=0;for(let i=0;i<n;i++)total+=values[i];return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "c4671389b73ed69a46ff93976f0e93062ea785ce464149187ad2c12178b697ae",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "iterator-invalid-next-result",
    "setup": "function bench(n){let total=0;let iterable={[Symbol.iterator](){return {next(){return 0;},return:function(){throw 99;}};}};for(let i=0;i<n;i++){try{[...iterable];throw 99;}catch(e){if(!(e instanceof TypeError))throw e;total+=i+1;}}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "c0f86b34f34d39da71253cf613f6d391c669caabfc68f3d8d00b763b5fae95c4",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
