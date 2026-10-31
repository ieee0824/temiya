# Symbol constructor regression

Full run41188434089 shard6 found two known-pass regressions in built-ins/Symbol/is-constructor.js. Removing [[Construct]] was incorrect: Symbol is a valid newTarget/extends constructor but its own construction throws TypeError. Original gate and raw failing observations are preserved beside fresh Ast/Auto captures of the unchanged original file. No failing record is replaced. Full-corpus and performance qualification remain separate.
