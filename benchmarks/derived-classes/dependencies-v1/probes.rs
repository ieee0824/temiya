use temiya::{Engine, embedding::ScriptBackend};
fn main() {
    let cases = [
        ("Object.keys", "typeof Object.keys === 'undefined';"),
        ("WeakMap.has", "typeof WeakMap.prototype.has === 'undefined';"),
        ("ArrayBuffer.slice", "typeof ArrayBuffer.prototype.slice === 'undefined';"),
        ("Date.getFullYear", "typeof Date.prototype.getFullYear === 'undefined';"),
        ("Number.toExponential", "typeof Number.prototype.toExponential === 'undefined';"),
        ("String.trim", "typeof String.prototype.trim === 'undefined';"),
        ("RegExp.test", "typeof RegExp.prototype.test === 'undefined';"),
        ("DataView.buffer", "new DataView(new ArrayBuffer(8)).buffer === undefined;"),
        ("Map.size", "new Map().size === undefined;"),
        ("Set.size", "new Set().size === undefined;"),
        ("Error.message", "Object.getOwnPropertyDescriptor(new Error('message'), 'message') === undefined;"),
        ("NativeErrors", "typeof EvalError === 'undefined' && typeof URIError === 'undefined';"),
        ("Boolean.payload", "(new Boolean(true)).valueOf() !== true;"),
        ("Symbol.call", "Symbol('test') === undefined;"),
        ("Promise.executor", "let threw = false; try { new Promise(); } catch(e) { threw = e instanceof TypeError; } threw === false;"),
        ("RegExp.lastIndex", "Object.getOwnPropertyDescriptor(new RegExp('39?'), 'lastIndex') === undefined;"),
        ("parseInt", "typeof parseInt === 'undefined';"),
        ("eval", "typeof eval === 'undefined';"),
        ("TypedArray.globals", "typeof Float64Array === 'undefined' && typeof Uint8Array === 'undefined';"),
        ("Function.dynamic", "typeof (new Function('return 42;')) !== 'function';"),
        ("IteratorClose", "let closed = false; let count = 0; let iterable = { [Symbol.iterator]() { return this; }, next() { count++; return { value: 1, done: count > 1 }; }, return() { closed = true; return {}; } }; for (let x of iterable) { break; } closed === false;"),
        ("Spread.generatorThrow", "function normal() {} let caught = false; try { normal(...function*() { throw 42; }()); } catch(e) { caught = e === 42; } caught === false;"),
    ];
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        for (name, source) in cases {
            let engine = Engine::builder().script_backend(backend).build();
            let realm = engine.create_realm();
            match engine.compile(&realm, source).map_err(|error| error.to_string()).and_then(|script| engine.evaluate(&script).map_err(|error| error.to_string())) {
                Ok(value) => println!("{backend:?}\t{name}\t{:?}\t{source}", value.value()),
                Err(error) => println!("{backend:?}\t{name}\tERROR {error}\t{source}"),
            }
        }
    }
}
