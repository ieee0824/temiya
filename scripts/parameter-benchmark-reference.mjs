// Original eight inputs frozen before parameter performance observations.
export const classFixtureVersion = 1;
export const classManifestSha256 = '007499fc9cc80883b80009b16897af45480f86d7e8145416bd9ed6258c7e034e';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "parameter-env-default-closure",
    "setup": "let outer=0;function read(a=()=>outer){var outer=0;return a();}function bench(n){let total=0;for(let i=1;i<=n;i++){outer=i;total+=read();}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "1ca1722872b75f3d16a0fdf4d890484573c3d0cab23527c6450baf002881d07a",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-var-copy",
    "setup": "function read(value=0){var value;return value;}function bench(n){let total=0;for(let i=1;i<=n;i++){total+=read(i);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "a91470ba964718b601cfb0f8320fd04123d90e7d30c64db4d61409a2e42f8725",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-nested-default",
    "setup": "let outer=0;function read({value,read=()=>outer}){var outer=0;if(value!==read())throw 42;return value;}function bench(n){let total=0;for(let i=1;i<=n;i++){outer=i;total+=read({value:i});}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "bbd4256617a85f4df3865831c3114c4e84ba3b710eac2532b5e25a0b0e8e513f",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-computed-key",
    "setup": "let key='answer';function read({[key]:value}){var key='missing';return value;}function bench(n){let total=0;for(let i=1;i<=n;i++){total+=read({answer:i});}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "e09c76f48179f04d8b4d93c9ee2bb8a03c7a133df18132e3dff94ac46d9ecb7c",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-rest-copy",
    "setup": "function read(value=0,...rest){var value;if(rest[0]!==value)throw 42;return value;}function bench(n){let total=0;for(let i=1;i<=n;i++){total+=read(i,i);}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "cc6954d9d433a0eb63e1201541b04d80ad0ce7b52500ac3f49b2ada374707a7b",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-constructor",
    "setup": "function Item(value=0){var value;this.value=value;}function bench(n){let total=0;for(let i=1;i<=n;i++){total+=new Item(i).value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "22f252ab7d47793ddbb7c1a7dea2f46dab067bf3250e00b463c945b8fcd07d60",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-generator",
    "setup": "let outer=0;function* read(a=()=>outer){var outer=0;yield a();}function bench(n){let total=0;for(let i=1;i<=n;i++){outer=i;let iterator=read();total+=iterator.next().value;if(!iterator.next().done)throw 42;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "2b96f07718eaf65c18544a82af3b74fab0371263f9a9356f17add9df495746c9",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "parameter-env-async",
    "setup": "let outer=0,result=0;async function read(a=()=>outer){var outer=0;result=a();}function bench(n){let total=0;for(let i=1;i<=n;i++){outer=i;read();if(result!==i)throw 42;total+=result;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "c124da4ad805c15c09b087c068fa78906e6ed3ea2995300f7e1eab6b20cdd579",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
