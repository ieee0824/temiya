// Authored normal-case RegExp workloads, separate from the original eleven.
// Expected results use string lengths and counts, never engine output.
export const regexpFixtureVersion = 1;
export const regexpWorkloads = [
  {
    name: 'regexp-construction', group: 'regexp', iterations: 128, expected: 1920,
    setup: "function __temiyaRegexpBody(count){var total=0;for(var i=0;i<count;i++){var pattern='([a-z]+)-([0-9]+)(?:x{'+i+'})?';var result=new RegExp(pattern).exec('alpha-42');if(result===null)return -1;total+=result[0].length+result[1].length+result[2].length;}return total;}",
    invocation: '__temiyaRegexpBody(128);',
    oracle: '128 matches, each full match length 8 plus captures of lengths 5 and 2: 128*15=1920. Pattern text varies; construction may use engine caches.',
  },
  {
    name: 'regexp-global-exec', group: 'regexp', iterations: 2048, expected: 18432,
    setup: "var regexpGlobal=new RegExp('[a-z]+','g');function __temiyaRegexpBody(count){var total=0;for(var i=0;i<count;i++){var result;while((result=regexpGlobal.exec('abc 12 defg 345 hi'))!==null){total+=result[0].length;}}if(regexpGlobal.exec('1234')!==null)return -1;return total;}",
    invocation: '__temiyaRegexpBody(2048);',
    oracle: 'Three matches of lengths 3,4,2 give 9*2048=18432. A failed exec terminates each scan and resets global state; a final non-match is checked.',
  },
  {
    name: 'regexp-unicode-captures', group: 'regexp', iterations: 2048, expected: 6144,
    setup: "var regexpUnicode=new RegExp('(.)','u');function __temiyaRegexpBody(count){var total=0;for(var i=0;i<count;i++){var input=i%2===0?'😀':'a';var result=regexpUnicode.exec(input);if(result===null)return -1;total+=result[0].length+result[1].length;}return total;}",
    invocation: '__temiyaRegexpBody(2048);',
    oracle: '1024 astral matches contribute 2+2 UTF-16 units; 1024 ASCII matches contribute 1+1: 1024*6=6144.',
  },
];
