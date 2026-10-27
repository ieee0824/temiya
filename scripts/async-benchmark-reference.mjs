// Additional authored workloads, separate from the unchanged omoikane eleven.
// Numeric oracles follow independent count/sum formulas, not engine output.
export const asyncFixtureVersion = 1;
export const jobsPerPass = 2048;
export const asyncWorkloads = [
  {
    name:'promise-chain', group:'async', iterations:jobsPerPass,
    expected:jobsPerPass, before_drain_expected:-1,
    setup:`async function seed(value){return value;}
var completed=-1;
function step(value){return value+1;}
function record(value){completed=value;}
function launch(count){completed=-1;var result=seed(0);for(var i=0;i<count;i++)result=result.then(step);result.then(record);return completed;}`,
    invocation:`launch(${jobsPerPass});`, completion:'completed;',
    oracle:'2048 chained increments from zero; completion remains -1 before the checkpoint',
  },
  {
    name:'promise-fanout', group:'async', iterations:jobsPerPass,
    expected:jobsPerPass*(jobsPerPass-1)/2, before_drain_expected:0,
    setup:`async function seed(value){return value;}
var completed=0,total=0;
function collect(value){total+=value;completed++;}
function launch(count){completed=0;total=0;for(var i=0;i<count;i++)seed(i).then(collect);return completed;}`,
    invocation:`launch(${jobsPerPass});`, completion:`completed===${jobsPerPass}?total:-1;`,
    oracle:'Sum of integers 0..2047 is 2048*2047/2, with exactly 2048 completed reactions',
  },
  {
    name:'promise-rejection-chain', group:'async', iterations:jobsPerPass,
    expected:jobsPerPass, before_drain_expected:-1,
    setup:`async function seed(value){return value;}
var completed=-1;
function rejectNext(value){throw value+1;}
function recover(reason){return reason;}
function record(value){completed=value;}
function launch(count){completed=-1;var result=seed(0);for(var i=0;i<count;i++)result=result.then(rejectNext).then(undefined,recover);result.then(record);return completed;}`,
    invocation:`launch(${jobsPerPass});`, completion:'completed;',
    oracle:'2048 increments carried as numeric rejection reasons and recovered unchanged',
  },
];
