/*---
description: Authored reproduction of for-let header binding capture, not original Test262.
flags: [raw]
---*/
class C { static run(n) { let a; let b; for (let i = 0; i < n; i++) { if (i === 0) a = function() { return i; }; else b = function() { return i; }; } return a() === 0 && b() === 1; } }
if (C.run(2) !== true) throw new Error('for-let captures do not retain their iteration values');
