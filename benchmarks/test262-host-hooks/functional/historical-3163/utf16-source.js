/*---
flags: [noStrict]
---*/
var source = "'\uD800'";
var value = $262.evalScript(source);
assert.sameValue(value.length, 1);
assert.sameValue(value.charCodeAt(0), 0xD800);
