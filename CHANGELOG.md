# Changelog

All notable changes are documented here. Temiya follows Semantic Versioning;
until 1.0, a minor version may contain breaking API changes.

## [Unreleased]

### Added

- Bootstrap ECMAScript engine components and stable embedding boundaries.
- Shared tracing heap for Engine Realms, allocation safepoints, and explicit
  `Engine::collect_garbage` / `AllocationContext::collect_garbage` hooks.
- Explicit `Object::new_in`, `native_function_in`, and `native_constructor_in`
  factories for host objects in an Engine's allocation context.

### Changed

- JSON parsing, JobQueue, and standalone Promise Rust factories require an
  allocation context; JobQueue no longer implements `Default`.
- Declarative and module Environment child factories require a parent;
  parentless records use `declarative_in` / `module_in` with a context.

- Map, Set, WeakMap, WeakSet, Array, ArrayBuffer, String, Number, BigInt, Date,
  and RegExp Rust constructors require
  an explicit allocation context; collection `Default` implementations are removed.

- The 0.2 Rust boundary rejects host objects from a different allocation context.
  Contextless low-level constructors remain legacy during migration; use the
  Engine's allocation context when supplying objects to its APIs.
- Realm handle destruction may defer host callback destruction until collection
  when another owner keeps the shared heap alive.

## [0.1.0] - 2026-10-26

- Establish the initial crate, C ABI, interpreter infrastructure, and tooling.

[Unreleased]: https://github.com/ieee0824/temiya/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ieee0824/temiya/releases/tag/v0.1.0
