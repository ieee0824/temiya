// Authored component contracts. Native Heap diagnostics are not Engine GC.
export const componentVersion=1;
export const parserSource='function 足す(a,b){return a+b;} 足す(20,22);';
export const objectSetup="function objects(){var total=0;for(var i=0;i<256;i++){var o={value:i};o.value+=1;Object.defineProperty(o,'double',{get:function(){return this.value*2;}});total+=o.double;delete o.value;total+=('value' in o)?1:0;}return total;}";
export const componentCases=[
 {name:'parser-script',group:'parse',batch:1024,expected:1024*2,source:parserSource,scope:'parse_script plus AST checks and drop; no evaluation'},
 {name:'engine-compile',group:'parse',batch:128,expected:128*(20+22),source:parserSource,scope:'sum of Engine::compile intervals; evaluation and Script drop excluded'},
 {name:'engine-startup',group:'startup',batch:64,expected:64*(40+2),source:'',scope:'sum of Engine construction plus create_realm intervals; correctness probe and teardown excluded'},
 {name:'objects-properties',group:'objects',batch:256,expected:256*(256+1),source:objectSetup,scope:'parse+evaluate objects() invocation and Script drop; function setup excluded'},
 {name:'gc-full-cycle',group:'gc',batch:16,expected:16*256,source:'',scope:'standalone Heap graph allocation, rooted tracing, cycle reclamation and validation; not Engine GC'},
 {name:'gc-minor-barrier',group:'gc',batch:64,expected:64*2,source:'',scope:'standalone Heap old-to-young barrier, minor/full collection and validation; not Engine GC'},
];
export function validateComponent(m,contract,samples){
 const fail=message=>{throw Error(contract.name+': '+message);};
 for(const key of ['name','group','batch','expected','source','scope'])if(m[key]!==contract[key])fail('contract '+key);
 if(m.engine!=='Temiya'||m.profile!=='release'||!Array.isArray(m.features)||m.features.length!==0)fail('release worker');
 if(!['x86_64','aarch64'].includes(m.architecture))fail('recorded supported measurement target');
 if(m.validated_passes!==samples+1||m.warmup_result!==contract.expected||!Number.isSafeInteger(m.warmup_ns)||m.warmup_ns<=0)fail('checked warmup');
 if(m.samples_ns?.length!==samples||m.observed_results?.length!==samples||!m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0)||!m.observed_results.every(n=>n===contract.expected))fail('checked samples');
 if(m.gc_api!==(contract.group==='gc'?'temiya::gc::Heap (standalone)':null))fail('GC scope');
}
export const median=values=>{const sorted=[...values].sort((a,b)=>a-b),mid=Math.floor(sorted.length/2);return sorted.length%2?sorted[mid]:Math.floor((sorted[mid-1]+sorted[mid])/2);};
export const statistics=values=>{const median_ns=median(values),mad_ns=median(values.map(n=>Math.abs(n-median_ns)));return {median_ns,mad_ns,relative_mad:mad_ns/median_ns};};
