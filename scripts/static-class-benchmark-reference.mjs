// Inputs frozen and actual Boa oracles checked before issue158 implementation.
export const classFixtureKind = 'authored-static-class-elements';
export const classFixtureVersion = 2;
export const classManifestSha256 = '6d60783f6266be2cb28480a984abed083a65d8783adb174cfcf4ef92d0431031';
export const classReferenceRevision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const classWorkloads = [
  {
    "name": "static-order",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let total=0;for(let i=0;i<n;i++){let log='';class C{static first=(log+='a',i+1);static{log+='b';this.second=this.first;}static third=(log+='c',this.second);}if(log!=='abc'||C.first!==C.third)throw 42;total+=C.third;}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "6922dfbcd0800d115667f658c70ab48f636bc8dffe08f02358f375937529503b"
  },
  {
    "name": "static-computed-key",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let total=0;let keys=0;for(let i=0;i<n;i++){let log='';class C{static [(keys++,log+='K','x')]=(log+='A',i+1);static [(keys++,log+='K','y')]=(log+='B',this.x);}if(log!=='KKAB'||C.x!==C.y)throw 42;total+=C.y;}if(keys!==2*n)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "66a1ec9ccc141c1ee309e1741f48d25129dc3bf1bf20edb8cfa2def3ec00aec2"
  },
  {
    "name": "static-super",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let total=0;for(let i=0;i<n;i++){class A{static seed=i;}class B extends A{static value=super.seed+1;static{this.read=()=>this.value;}}if(B.read()!==i+1||A.hasOwnProperty('value'))throw 42;total+=B.read();}return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "3defec2010d2cf1bfb4db568021a110149824029b7280cbe552b9a153e702eba"
  },
  {
    "name": "static-define-own",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let calls=0;class A{static set value(v){calls++;}}let total=0;for(let i=0;i<n;i++){class B extends A{static value=i+1;}let d=Object.getOwnPropertyDescriptor(B,'value');if(!d.writable||!d.enumerable||!d.configurable)throw 42;total+=B.value;}if(calls!==0)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "d5e9f571dd320a616c0251e494e8875d3eb87904c2845a092daed52a446ed503"
  },
  {
    "name": "static-abrupt",
    "iterations": 10000,
    "oracle": "sum-one-to-n-and-per-case-invariants",
    "setup": "function bench(n){let total=0;let effects=0;let marker={};for(let i=0;i<n;i++){let caught=false;try{class C{static first=(effects++,i+1);static{if(i===n-1)throw marker;}static later=effects++;}}catch(error){caught=error===marker;}if(caught!==(i===n-1))throw 42;total+=i+1;}if(effects!==2*n-1)throw 42;return total;}",
    "invocation": "bench(10000);",
    "expected": 50005000,
    "file_sha256": "655fb685db079e28a95d5656d2d8c7e025793e6a3580399643dd33a635382621"
  }
];
