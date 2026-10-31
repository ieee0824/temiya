fn outer(xs: &[u8]) {
    for _ in xs {
        let _closure = || { for _ in xs { for _ in xs {} } };
    }
}
