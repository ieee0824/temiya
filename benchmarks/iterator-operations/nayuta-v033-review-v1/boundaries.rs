fn outer(xs: &[u8]) {
    for _ in xs {
        let _closure = || { for _ in xs {} };
        let _future = async { for _ in xs {} };
        fn local(ys: &[u8]) { for _ in ys {} }
        for _ in xs {}
    }
}
