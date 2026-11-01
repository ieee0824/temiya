// Workload bytes pinned by preimplementation freeze 75d6b2a.
export const classFixtureVersion = 1;
export const classManifestSha256 = 'd9ff97bf282a825af6300e3eeac03c0cd07f2e463a4e01d2ebdf048dcf32a788';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "object-binding-declaration",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let {value,nested:{number}}={value:i,nested:{number:i}};if(number!==i)throw 42;total+=value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "77d40107200b47d470b041045e1006c8c8a28c3943fbecaebe043cfd9a5b3589",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "object-binding-parameters",
    "setup": "function read({value,missing=value,nested:{number}= {number:value}}){if(missing!==value||number!==value)throw 42;return value;}function bench(n){let total=0;for(let i=1;i<=n;i++)total+=read({value:i});return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "673ab1ce162b188086e582ccd935a325878346403e25c4ca1da5fbc9a3756922",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "object-binding-computed-default",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let order=\"\";let key={toString(){order+=\"k\";return \"value\";}};let {[key]:value=(order+=\"d\",i)}={};if(order!==\"kd\")throw 42;total+=value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "a12aabe59269494e444635f62eae7ef55b74904d08dd02fc2cdf628833f9da38",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "object-binding-rest-symbol",
    "setup": "let key=Symbol(\"remaining\");function bench(n){let total=0;for(let i=1;i<=n;i++){let input={drop:0,keep:i,[key]:i};Object.defineProperty(input,\"hidden\",{value:99,enumerable:false});let {drop,...rest}=input;if(drop!==0||rest[key]!==i||\"drop\" in rest||\"hidden\" in rest||Reflect.ownKeys(rest).length!==2)throw 42;total+=rest.keep;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "99d0ec0232f455521776b5222515f919093a01c4ab5a3194e5201254161e7389",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "object-binding-getter-order",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let order=\"\";let input={get first(){order+=\"a\";return i;},get second(){order+=\"b\";return i;}};let {first:value,...rest}=input;if(order!==\"ab\"||rest.second!==i||\"first\" in rest)throw 42;total+=value;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "c30dd8995095d7f383784388da6020a2468b895304d477af111d92ce1875e836",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  },
  {
    "name": "object-binding-abrupt-coercion",
    "setup": "function bench(n){let total=0;for(let i=1;i<=n;i++){let sentinel={};try{let {value}={get value(){throw sentinel;}};throw 42;}catch(error){if(error!==sentinel)throw error;}try{let {}=null;throw 42;}catch(error){if(!(error instanceof TypeError))throw error;}try{let {...rest}=undefined;throw 42;}catch(error){if(!(error instanceof TypeError))throw error;}total+=i;}return total;}",
    "invocation": "bench(10000);",
    "iterations": 10000,
    "expected": 50005000,
    "file_sha256": "cd50e27d157b95dedad95140620cee04d4b9ffe709b72cab94598c74da3cd3ac",
    "oracle": "sum-one-to-n-and-per-case-invariants"
  }
];
