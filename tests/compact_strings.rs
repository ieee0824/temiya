use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    mem::size_of,
};
use temiya::{
    Engine,
    runtime::{JsString, PropertyKey, Symbol, Value},
};

fn hash(value: &impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

#[test]
fn constructors_and_clones_preserve_exact_units_at_storage_boundaries() {
    for length in 0..=128 {
        let mut units = (0..length)
            .map(|i| [0, 0x61, 0xD800, 0xDC00, 0xFFFF][i % 5])
            .collect::<Vec<_>>();
        let owned = JsString::from_code_units(units.clone());
        let borrowed = JsString::from_code_units_slice(&units);
        let clone = owned.clone();
        assert_eq!(owned.code_units(), units);
        assert_eq!(owned, borrowed);
        assert_eq!(hash(&owned), hash(&borrowed));
        assert_eq!(hash(&owned), hash(&units));
        assert_eq!(owned.is_empty(), length == 0);
        assert_eq!(owned.to_string_lossy(), String::from_utf16_lossy(&units));
        units.fill(7);
        drop(owned);
        assert_eq!(clone, borrowed);
        assert_eq!(clone.code_units().len(), length);
    }
    for input in [
        "",
        "123456789",
        "1234567890",
        "12345678901",
        "01234567😀",
        "012345678😀",
        "αβγδεζηθικλ",
        "a\0b",
    ] {
        let units = input.encode_utf16().collect::<Vec<_>>();
        assert_eq!(JsString::from(input), JsString::from_code_units(units));
        assert_eq!(JsString::from(input.to_owned()), JsString::from(input));
    }
}

#[test]
// Only the contiguous representation cache mutates; UTF-16 key contents cannot.
#[allow(clippy::mutable_key_type)]
fn equality_hash_and_order_use_utf16_contents() {
    let units = [
        vec![],
        vec![0],
        vec![0x61],
        vec![0x61; 10],
        vec![0x61; 11],
        vec![0xD800],
        vec![0xD800, 0xDC00],
        vec![0xDC00],
        vec![0xE000],
        vec![0xFFFF; 12],
    ];
    let mut map = HashMap::new();
    for (i, raw) in units.iter().enumerate() {
        map.insert(PropertyKey::from(JsString::from_code_units(raw.clone())), i);
    }
    for (i, left) in units.iter().enumerate() {
        let string = JsString::from_code_units_slice(left);
        assert_eq!(map.get(&PropertyKey::from(string.clone())), Some(&i));
        for right in &units {
            let other = JsString::from_code_units(right.clone());
            assert_eq!(string.cmp(&other), left.cmp(right));
            assert_eq!(string == other, left == right);
        }
    }
    assert_eq!(JsString::default(), JsString::from(""));
    assert!(JsString::from("😀") < JsString::from("\u{E000}"));
    let rope = JsString::from("a".repeat(256)).concat(JsString::from("b"));
    map.insert(PropertyKey::from(rope.clone()), 42);
    assert_eq!(rope.code_units().len(), 257);
    assert_eq!(map.get(&PropertyKey::from(rope)), Some(&42));
    assert_eq!(
        map.get(&PropertyKey::from(JsString::from(format!(
            "{}b",
            "a".repeat(256)
        )))),
        Some(&42)
    );
}

#[test]
fn leaf_strings_remain_send_sync_and_symbols_keep_distinct_identity() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<JsString>();
    let string = JsString::from("shared string longer than inline storage");
    let alias = string.clone();
    let units = std::thread::spawn(move || alias.code_units().to_vec())
        .join()
        .unwrap();
    assert_eq!(string.code_units(), units);
    let first = Symbol::new(Some(string.clone()));
    let second = Symbol::new(Some(string));
    assert_ne!(first, second);
    assert_eq!(first.description(), second.description());
    assert_eq!(first, first.clone());
    assert_eq!(size_of::<JsString>(), size_of::<Vec<u16>>());
}

#[test]
fn public_engine_retains_aliases_slices_and_surrogate_keys() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine
        .compile(
            &realm,
            r#"var original = '0123456789\uD800\u0000\uDC00';
        var alias = original; var part = alias.slice(8, 12);
        var object = {}; object[alias] = 42; original += 'changed';
        var capture = function() {return alias;};
        alias.length === 13 && capture() === alias && original.length === 20 &&
        object['0123456789\uD800\u0000\uDC00'] === 42 &&
        alias.charCodeAt(10) === 55296 && alias.codePointAt(12) === 56320 &&
        part === '89\uD800\u0000' && alias !== original;"#,
        )
        .unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Boolean(true)
    ));
}

#[test]
fn concatenations_preserve_units_aliases_hashes_and_threaded_flattening() {
    let chunk = JsString::from_code_units_slice(&[0xD800, 0, 0xDC00, 0x61]);
    let mut string = JsString::default();
    let mut expected = Vec::new();
    let mut aliases = Vec::new();
    for index in 0..10_000 {
        string = string.concat(chunk.clone());
        expected.extend_from_slice(chunk.code_units());
        assert_eq!(string.len(), expected.len());
        if index % 1000 == 0 {
            aliases.push((string.clone(), expected.clone()));
        }
    }
    let shared = string.clone();
    let concurrent = std::thread::spawn(move || shared.code_units().to_vec());
    assert_eq!(string.code_units(), expected);
    assert_eq!(concurrent.join().unwrap(), expected);
    assert_eq!(hash(&string), hash(&expected));
    assert_eq!(string, JsString::from_code_units(expected));
    for (alias, expected) in aliases {
        assert_eq!(alias.code_units(), expected);
    }
    let right_deep = (0..10_000).fold(JsString::default(), |tail, _| chunk.clone().concat(tail));
    assert_eq!(right_deep, string);
}

#[test]
fn concatenation_checks_all_inline_boundaries_and_preserves_evaluation_order() {
    for left in 0..=20 {
        for right in 0..=20 {
            let a = vec![0xD800; left];
            let b = vec![0xDC00; right];
            let combined =
                JsString::from_code_units_slice(&a).concat(JsString::from_code_units_slice(&b));
            assert_eq!(combined.code_units(), [a, b].concat());
        }
    }
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine
        .compile(
            &realm,
            r#"
        var order = '';
        var left = function() {order += 'L'; return 'a';};
        var right = function() {order += 'R'; return 'b';};
        var joined = left() + right();
        var failure = function() {order += 'X'; throw 17;};
        var caught = false;
        try {failure() + right();} catch (e) {caught = e === 17;}
        joined === 'ab' && order === 'LRX' && caught;
    "#,
        )
        .unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Boolean(true)
    ));
}

#[test]
fn engine_observes_long_concatenations_through_string_apis_and_property_keys() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let script = engine.compile(&realm, r#"
        var joined = '';
        for (var i = 0; i < 1000; i++) joined += '\uD800\u0000\uDC00';
        var alias = joined;
        var object = {};
        object[joined] = 42;
        joined += 'end';
        var independentlyBuilt = '';
        for (var j = 0; j < 1000; j++) independentlyBuilt = '\uD800\u0000\uDC00' + independentlyBuilt;
        alias.length === 3000 && joined.length === 3003 &&
        independentlyBuilt === alias && object[independentlyBuilt] === 42 &&
        alias.charCodeAt(2997) === 55296 && alias.charCodeAt(2999) === 56320 &&
        joined.slice(2997) === '\uD800\u0000\uDC00end' &&
        new String(alias).length === 3000 && alias < joined;
    "#).unwrap();
    assert!(matches!(
        engine.evaluate(&script).unwrap().value(),
        Value::Boolean(true)
    ));
}
