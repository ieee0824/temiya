// Workload bytes frozen by commit 259ec5c, before observations.
export const classFixtureVersion = 1;
export const classManifestSha256 = '07f6e0ff810a8b3047a5b586716ef08bb9080410291cb0ed99d322313c85457c';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "assignment-array-members",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let obj={},a;[a,obj.value]=[i,i];if(obj.value!==i)throw 42;total+=a;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "e0e0ab74a051db548f433e12d436bc5d8081e758630ba1472a4a2f04dd4ed5d3",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "assignment-object-members",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let obj={},a;({value:a,nested:{number:obj.value}}={value:i,nested:{number:i}});if(obj.value!==i)throw 42;total+=a;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "2393c00fc6852a40f0c6bd1571ac1f80fc171200674a83841ac41c9a108d41e9",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "assignment-computed-default",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let trace='',obj={};function target(){trace+='T';return obj;}let source={get value(){trace+='G';return undefined;}};({[(trace+='K','value')]:target().value=(trace+='D',i)}=source);if(trace!=='KTGD'||obj.value!==i)throw 42;total+=obj.value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "d7da7a40c04c7b9d4fac251b733a157582cab478b81615d5519bd4f4b2fffd6a",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "assignment-rest-symbol",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let obj={},sym=Symbol(),a;({value:a,...obj.rest}={value:i,[sym]:i,other:i});if(obj.rest[sym]!==i||obj.rest.other!==i||'value' in obj.rest)throw 42;total+=a;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "179100ac5dc9d0e7460b46271e27ac5ee9b1ab5ccc86c9d97f39e0c93e4d7baa",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "assignment-close-priority",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let a=0,closed=0,marker={},caught=false;let obj={set value(v){throw marker;}};let input={[Symbol.iterator](){return {next(){return {value:i,done:false};},return(){closed++;throw 99;}};}};try{[a,obj.value]=input;}catch(e){caught=e===marker;}if(a!==i||closed!==1||!caught)throw 42;total+=a;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "70597181617cb1e4be5a24ac2317da8ce333827c35b818844c70581559a11723",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "assignment-const-failure",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let a=0;const fixed=0;let caught=false;try{[a,fixed]=[i,99];}catch(e){caught=e instanceof TypeError;}if(!caught||fixed!==0)throw 42;total+=a;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "498be1db686ec57dc1b693e74e1aa513b0426638347b5b88652c6598a0774b48",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
