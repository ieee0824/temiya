// Actual native worker checks plus rejected protocol/data regressions.
import {execFileSync,spawnSync} from 'node:child_process';
import {componentCases,validateComponent} from './component-benchmark-reference.mjs';
const executable=process.argv[2];if(!executable)throw Error('usage: check-component-workers.mjs WORKER');
let checked=0;
for(const contract of componentCases){
 const measurement=JSON.parse(execFileSync(executable,[contract.name,'1'],{encoding:'utf8',timeout:180000}));
 validateComponent(measurement,contract,1);checked+=measurement.validated_passes;
 for(const mutation of [m=>m.expected++,m=>m.observed_results[0]++,m=>m.warmup_result++,m=>m.scope='Engine GC',m=>m.samples_ns[0]=0,m=>m.validated_passes=0,m=>m.profile='debug']){
  const bad=structuredClone(measurement);mutation(bad);let rejected=false;
  try{validateComponent(bad,contract,1);}catch{rejected=true;}
  if(!rejected)throw Error('accepted invalid measurement '+contract.name);
 }
}
for(const args of [[],['absent','1'],['parser-script','0'],['parser-script','1001'],['parser-script','bad'],['parser-script','1','extra']]){
 const result=spawnSync(executable,args,{encoding:'utf8',timeout:10000});
 if(result.status===0||result.error||result.signal)throw Error('invalid worker command did not fail normally');
}
console.log(JSON.stringify({cases:componentCases.length,checked_results:checked,rejected_measurement_mutations:componentCases.length*7,rejected_commands:6,qualification:'Correctness and protocol validation, not speed acceptance'}));
