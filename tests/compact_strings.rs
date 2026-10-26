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
