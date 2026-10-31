# Object binding execution

Object patterns share declaration, parameter, catch and supported-loop binding
initialization with array patterns. BoundNames exclude computed-key/default
expressions. Names are predeclared before lexical or non-simple parameter
initialization so nested defaults retain TDZ and argument shadowing.

Property keys evaluate left to right and are converted once. Identifier targets
are resolved before GetV and defaults; nested patterns receive the property value
before their own coercion/default processing. Null and undefined throw even for
empty patterns, before computed keys execute. Property reads use the original
primitive receiver and the active Realm's primitive prototypes.

Rest creates a fresh ordinary object. Excluded string and symbol keys are skipped
before descriptor/get traps. Remaining own enumerable properties are read and
created as writable, enumerable, configurable data properties, without invoking
prototype setters. String primitives copy UTF-16 index values. Source values,
key symbols and partial results remain rooted across reentrant JavaScript/GC.

The foundational Symbol call creates unique symbol values after description
coercion. Symbol has a throwing `[[Construct]]`: it remains a valid constructor
for `extends` and `Reflect.construct` newTarget checks, but `new Symbol()` and
derived super calls throw TypeError. Symbol registry and remaining Symbol.prototype
APIs remain #204; this is not completion of that issue. Destructuring assignment,
complete for-of protocol, async and generator integration remain separate work.

The implementation follows ECMAScript 2026
[PropertyBindingInitialization, RestBindingInitialization and KeyedBindingInitialization](https://tc39.es/ecma262/2026/multipage/ecmascript-language-statements-and-declarations.html#sec-destructuring-binding-patterns).
Public Engine Ast/Auto regression tests are in tests/object_binding.rs. The fixed
selection records 8,807 files / 17,415 variants, including cross-feature failures.
Complete correctness captures, stable/MSRV/target checks, security and independent
two-job native Boa speed gates remain required before merging.
