use temiya::{Engine, runtime::Value};

fn check(source: &str) {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, source).unwrap();
    let result = engine.evaluate(&script).unwrap();
    assert!(
        matches!(result.value(), Value::Boolean(true)),
        "{source}\n{result:?}"
    );
}

#[test]
fn literal_constructor_and_array_prototype_are_connected() {
    check(
        "var a = []; var b = new Array(); \
        a.push(1, 2) === 2 && a.pop() === 2 && a.shift() === 1 && \
        a.unshift(3, 4) === 2 && a[0] === 3 && a[1] === 4 && \
        b.push(5) === 1 && b.pop() === 5 && \
        Object.getPrototypeOf(a) === Array.prototype && \
        Array.isArray(Array.prototype) && Array.prototype.length === 0;",
    );
    check("var a = []; a.push(a); a.pop() === a && a.length === 0;");
}

#[test]
fn generic_receivers_normalize_lengths_and_ignore_extra_pop_shift_arguments() {
    check(
        "var o = {0: 'a', 1: 'b', length: '2.9'}; \
        var n = Array.prototype.push.call(o, 'c'); \
        var a = Array.prototype.shift.call(o, 99); \
        var b = Array.prototype.pop.call(o, 99); \
        var m = Array.prototype.unshift.call(o, 'x', 'y'); \
        n === 3 && a === 'a' && b === 'c' && m === 3 && \
        o[0] === 'x' && o[1] === 'y' && o[2] === 'b' && o.length === 3;",
    );
    for len in ["undefined", "null", "NaN", "-Infinity", "-2", "-0", "0.9"] {
        check(&format!(
            "var o = {{length: {len}}}; \
            Array.prototype.pop.call(o) === undefined && 1/o.length === Infinity;"
        ));
    }
    check("var o = {}; Array.prototype.unshift.call(o) === 0 && o.length === 0;");
}

#[test]
fn primitive_receivers_are_boxed_but_nullish_receivers_are_rejected() {
    for receiver in ["true", "false", "42", "7n"] {
        check(&format!(
            "Array.prototype.push.call({receiver}, 1) === 1 && \
            Array.prototype.pop.call({receiver}) === undefined && \
            Array.prototype.shift.call({receiver}) === undefined && \
            Array.prototype.unshift.call({receiver}, 1) === 1;"
        ));
    }
    for name in ["push", "pop", "shift", "unshift"] {
        for receiver in ["null", "undefined", "''", "'abc'"] {
            check(&format!(
                "var caught = false; \
                try {{ Array.prototype.{name}.call({receiver}); }} \
                catch (e) {{ caught = e.constructor === TypeError; }} caught;"
            ));
        }
    }
    check(
        "Object.defineProperty(Boolean.prototype, 'length', {get() { \
        if (typeof this !== 'object') throw 1; return 2; }, \
        set(v) { if (typeof this !== 'object' || v !== 2) throw 2; }}); \
        Array.prototype.push.call(true) === 2;",
    );
}

#[test]
fn holes_and_inherited_indices_follow_has_property() {
    check(
        "var a = [, 'b', , 'd']; var first = a.shift(); \
        first === undefined && a.length === 3 && a[0] === 'b' && \
        !(1 in a) && a[2] === 'd' && !(3 in a);",
    );
    check(
        "var a = [, 'b', ]; a.unshift('x'); \
        a.length === 3 && a[0] === 'x' && !(1 in a) && a[2] === 'b';",
    );
    check(
        "var proto = {1: 'inherited'}; var o = Object.create(proto); \
        o.length = 3; o[0] = 'first'; o[2] = 'last'; \
        Array.prototype.shift.call(o) === 'first' && o[0] === 'inherited' && \
        o[1] === 'last' && !(2 in o) && o.length === 2;",
    );
    check(
        "var o = Object.create({2:'inherited'}); o.length = 3; \
        Array.prototype.pop.call(o) === 'inherited' && o.length === 2 && \
        o[2] === 'inherited';",
    );
}

#[test]
fn read_only_length_and_failed_deletion_throw_after_required_partial_changes() {
    for name in ["push", "pop", "shift", "unshift"] {
        check(&format!(
            "var a = []; Object.defineProperty(a, 'length', {{writable:false}}); \
            var caught = false; try {{ a.{name}(); }} \
            catch(e) {{ caught = e.constructor === TypeError; }} caught && a.length === 0;"
        ));
    }
    check(
        "var o = {0: 'a', 1: 'b'}; \
        Object.defineProperty(o, 'length', {value:2,writable:false}); \
        var caught = false; try { Array.prototype.pop.call(o); } \
        catch(e) { caught = e.constructor === TypeError; } \
        caught && !(1 in o) && o[0] === 'a' && o.length === 2;",
    );
    check(
        "var a = [1,2]; Object.defineProperty(a, '1', {configurable:false}); \
        var caught = false; try { a.pop(); } catch(e) { caught = e.constructor === TypeError; } \
        caught && a.length === 2 && a[1] === 2;",
    );
    check(
        "var a = [1,2]; Object.defineProperty(a, 'length', {writable:false}); \
        var caught = false; try { a.shift(); } catch(e) { caught = e.constructor === TypeError; } \
        caught && a.length === 2 && a[0] === 2 && !(1 in a);",
    );
}

#[test]
fn setter_and_getter_exceptions_preserve_values_and_stop_at_the_correct_index() {
    check(
        "var marker = {}; var o = {length:0}; \
        Object.defineProperty(o, '1', {set(v) { throw marker; }}); \
        var caught; try { Array.prototype.push.call(o, 'a','b','c'); } catch(e) { caught=e; } \
        caught === marker && o[0] === 'a' && !(2 in o) && o.length === 0;",
    );
    check(
        "var marker = {}; var a = [0,1,2,3]; \
        Object.defineProperty(a, '1', {set(v) {throw marker;}}); \
        var caught; try { a.shift(); } catch(e) {caught=e;} \
        caught === marker && a[0] === undefined && a[2] === 2 && a.length === 4;",
    );
    check(
        "var marker = {}; var o = {length:3,0:'a',1:'b',2:'c'}; \
        Object.defineProperty(o, '2', {set(v) {throw marker;},get(){return 'c';}}); \
        var caught; try { Array.prototype.unshift.call(o,'x'); } catch(e) {caught=e;} \
        caught === marker && o[3] === 'c' && o[1] === 'b' && o[0] === 'a' && o.length === 3;",
    );
}

#[test]
fn receiver_can_become_read_only_during_an_inherited_setter_or_getter() {
    for name in ["push", "unshift"] {
        check(&format!(
            "var a=[]; var calls=0; \
            Object.defineProperty(Array.prototype,'0',{{set(v){{ \
                Object.defineProperty(a,'length',{{writable:false}}); \
                Object.preventExtensions(a); calls++; }}}}); \
            var caught=false; try{{a.{name}(1);}}catch(e){{caught=e.constructor===TypeError;}} \
            caught && calls===1 && a.length===0 && Object.getOwnPropertyDescriptor(a,'0')===undefined;"
        ));
    }
    for name in ["pop", "shift"] {
        check(&format!(
            "var a=[1]; var calls=0; \
            Object.defineProperty(a,'0',{{get(){{ \
                Object.defineProperty(a,'0',{{configurable:false}}); \
                Object.defineProperty(a,'length',{{writable:false}}); \
                Object.preventExtensions(a); calls++; return 1; }}}}); \
            var caught=false; try{{a.{name}();}}catch(e){{caught=e.constructor===TypeError;}} \
            caught && calls===1 && a.length===1;"
        ));
    }
}

#[test]
fn length_conversion_calls_javascript_methods_once_before_writing() {
    check(
        "var log = ''; var o = {get length() {log += 'g'; \
        return {valueOf(){log += 'v'; return {};},toString(){log += 't'; return '2.9';}};}, \
        set length(v){log += 's'+v;}}; \
        Array.prototype.push.call(o,'x') === 3 && o[2] === 'x' && log === 'gvts3';",
    );
    check(
        "var marker={}; var o={length:{valueOf(){throw marker;}}}; \
        var caught; try { Array.prototype.push.call(o,1); } catch(e) {caught=e;} \
        caught === marker && !(0 in o);",
    );
    check(
        "var o={length:{[Symbol.toPrimitive](hint){ \
        if(hint !== 'number') throw 1; return 2;}, valueOf(){throw 2;}}}; \
        Array.prototype.push.call(o, 'x') === 3 && o[2] === 'x';",
    );
    check("var o={length:{}}; Array.prototype.push.call(o) === 0;");
    check(
        "var o={length:Object.create(null)}; var caught=false; \
        try {Array.prototype.push.call(o);} catch(e){caught=e.constructor===TypeError;} caught;",
    );
    for invalid in [
        "1n",
        "Symbol.toPrimitive",
        "{valueOf(){return {};},toString(){return {};}}",
        "{[Symbol.toPrimitive]: 1}",
        "{[Symbol.toPrimitive](){return {};}}",
    ] {
        check(&format!(
            "var o={{length:{invalid}}}; var caught=false; \
            try {{Array.prototype.push.call(o,1);}} catch(e) {{caught=e.constructor===TypeError;}} \
            caught && !(0 in o);"
        ));
    }
}

#[test]
fn embedding_conversion_hooks_remain_supported_for_length_values() {
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::Object;
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let engine = Engine::default();
    let length = Object::new_in(engine.allocation_context());
    length.set_value_of_hook(move || {
        observed.set(observed.get() + 1);
        Ok(Value::Number(2.0))
    });
    let realm = engine.create_realm();
    let handle = engine.value(&realm, Value::Object(length)).unwrap();
    engine.set_global(&realm, "hostLength", handle).unwrap();
    let script = engine
        .compile(
            &realm,
            "var o={length:hostLength}; Array.prototype.push.call(o,'x')===3 && o[2]==='x';",
        )
        .unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Boolean(true)
    ));
    assert_eq!(calls.get(), 1);
}

#[test]
fn proxy_traps_observe_order_and_abrupt_completions() {
    check(
        "var log=''; var p=new Proxy({0:'a',2:'c',length:3},{ \
        get(t,k){log+='g'+k+';';return Reflect.get(t,k);}, \
        has(t,k){log+='h'+k+';';return Reflect.has(t,k);}, \
        set(t,k,v){log+='s'+k+';';return Reflect.set(t,k,v);}, \
        deleteProperty(t,k){log+='d'+k+';';return Reflect.deleteProperty(t,k);} }); \
        var result=Array.prototype.shift.call(p); result==='a' && \
        log==='glength;g0;h1;d0;h2;g2;s1;d2;slength;';",
    );
    check(
        "var marker={}; var target={0:'a',length:1}; \
        var p=new Proxy(target,{deleteProperty(){throw marker;}}); var caught; \
        try{Array.prototype.pop.call(p);}catch(e){caught=e;} \
        caught===marker && target.length===1 && target[0]==='a';",
    );
    check(
        "var target={length:0}; var p=new Proxy(target,{set(){return false;}}); \
        var caught=false; try{Array.prototype.push.call(p,1);}catch(e){caught=e.constructor===TypeError;} \
        caught && target.length===0;",
    );
}

#[test]
fn safe_integer_limits_do_not_wrap_to_u32_or_write_before_overflow_check() {
    check(
        "var o={length:9007199254740990}; \
        Array.prototype.push.call(o,'last')===9007199254740991 && \
        o['9007199254740990']==='last' && Array.prototype.pop.call(o)==='last' && \
        o.length===9007199254740990;",
    );
    for name in ["push", "unshift"] {
        check(&format!(
            "var o={{length:Infinity}}; var count=0; \
            var p=new Proxy(o,{{set(){{count++;return true;}},has(){{count++;return false;}}}}); \
            var caught=false; try{{Array.prototype.{name}.call(p,1);}}catch(e){{caught=e.constructor===TypeError;}} \
            caught && count===0;"
        ));
    }
    check("var o={length:Infinity}; Array.prototype.unshift.call(o)===9007199254740991;");
    check(
        "var a=[]; a.length=4294967295; var caught=false; \
        try{a.push(1);}catch(e){caught=e.constructor===RangeError;} \
        caught && a.length===4294967295 && a['4294967295']===1;",
    );
}

#[test]
fn method_descriptors_and_non_constructibility_match_the_builtin_contract() {
    for (name, arity) in [("push", 1), ("pop", 0), ("shift", 0), ("unshift", 1)] {
        check(&format!(
            "var f=Array.prototype.{name}; \
            var d=Object.getOwnPropertyDescriptor(Array.prototype,'{name}'); \
            var n=Object.getOwnPropertyDescriptor(f,'name'); \
            var l=Object.getOwnPropertyDescriptor(f,'length'); \
            var caught=false; try{{new f();}}catch(e){{caught=e.constructor===TypeError;}} \
            caught && !('prototype' in f) && Object.getPrototypeOf(f)===Function.prototype && \
            f.name==='{name}' && f.length==={arity} && d.writable && !d.enumerable && d.configurable && \
            !n.writable && !n.enumerable && n.configurable && !l.writable && !l.enumerable && l.configurable;"
        ));
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus; see docs/runtime.md"]
fn pinned_test262_array_stack() {
    let selection = std::env::var("TEST262_CASE").unwrap_or_else(|_| {
        [
            "push/S15.4.4.7_A2_T3.js",
            "push/call-with-boolean.js",
            "push/length-near-integer-limit.js",
            "push/length-near-integer-limit-set-failure.js",
            "push/throws-if-integer-limit-exceeded.js",
            "push/throws-with-string-receiver.js",
            "push/set-length-array-length-is-non-writable.js",
            "pop/S15.4.4.6_A1.2_T1.js",
            "pop/call-with-boolean.js",
            "pop/clamps-to-integer-limit.js",
            "pop/length-near-integer-limit.js",
            "pop/throws-with-string-receiver.js",
            "pop/set-length-array-length-is-non-writable.js",
            "shift/S15.4.4.9_A1.2_T1.js",
            "shift/call-with-boolean.js",
            "shift/throws-when-this-value-length-is-writable-false.js",
            "shift/set-length-array-length-is-non-writable.js",
            "unshift/S15.4.4.13_A2_T3.js",
            "unshift/call-with-boolean.js",
            "unshift/clamps-to-integer-limit.js",
            "unshift/length-near-integer-limit.js",
            "unshift/throws-if-integer-limit-exceeded.js",
            "unshift/throws-with-string-receiver.js",
            "unshift/read-only-property.js",
            "unshift/set-length-array-length-is-non-writable.js",
        ]
        .join(",")
    });
    run_pinned_array_cases("prototype", &selection);
}

#[test]
#[ignore = "requires pinned Test262 corpus; see docs/runtime.md"]
fn pinned_test262_array_elements() {
    let selection = std::env::var("TEST262_CASE").unwrap_or_else(|_| {
        [
            "15.4.5.1-3.d-1.js",
            "15.4.5.1-3.d-2.js",
            "15.4.5.1-3.d-3.js",
            "S15.4.2.2_A2.1_T1.js",
            "S15.4.5.1_A1.1_T1.js",
            "S15.4.5.1_A1.2_T1.js",
            "S15.4.5.2_A3_T4.js",
            "define-own-prop-length-no-value-order.js",
            "define-own-prop-length-overflow-order.js",
        ]
        .join(",")
    });
    run_pinned_array_cases("length", &selection);
}

fn run_pinned_array_cases(directory: &str, selection: &str) {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};

    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let read = |path: &str| {
        let output = Command::new("git")
            .args(["-C", &corpus, "show", &format!("{TEST262_REVISION}:{path}")])
            .output()
            .expect("git show");
        assert!(output.status.success(), "missing pinned file {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let mut passed = 0;
    for name in selection.split(',') {
        let path = format!("test/built-ins/Array/{directory}/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.negative.is_none());
        assert!(
            !case
                .metadata
                .flags
                .iter()
                .any(|flag| flag == "async" || flag == "raw")
        );
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            let engine = Engine::default();
            let realm = engine.create_realm();
            for name in ["sta.js", "assert.js"]
                .into_iter()
                .chain(case.metadata.includes.iter().map(String::as_str))
            {
                let setup = engine
                    .compile(&realm, &read(&format!("harness/{name}")))
                    .unwrap_or_else(|e| panic!("harness {name} compile: {e}"));
                engine
                    .evaluate(&setup)
                    .unwrap_or_else(|e| panic!("harness {name} evaluate: {e:?}"));
            }
            let script = engine
                .compile(&realm, &case.source_for(variant))
                .unwrap_or_else(|e| panic!("{path} {variant:?} compile: {e}"));
            engine
                .evaluate(&script)
                .unwrap_or_else(|e| panic!("{path} {variant:?} evaluate: {e:?}"));
            passed += 1;
            println!("PASS {path} {variant:?}");
        }
    }
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped in selection");
}
