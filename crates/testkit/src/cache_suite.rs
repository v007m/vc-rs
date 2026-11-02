use vc_interfaces::CompileCacheStore;

/// Builds an empty store.
pub type MakeCompileCache<'a> = dyn FnMut() -> Box<dyn CompileCacheStore> + 'a;

/// The contract every [`CompileCacheStore`] meets (D112). `make` builds an empty store.
pub fn compile_cache_store_suite(make: &mut MakeCompileCache<'_>) {
    holds_nothing_at_first(make);
    gives_back_the_last_blob_saved(make);
}

fn load(store: &dyn CompileCacheStore) -> Option<Vec<u8>> {
    let result = store.load();
    assert_eq!(result.as_ref().err(), None, "load failed");
    result.unwrap_or_default()
}

fn save(store: &dyn CompileCacheStore, bytes: &[u8]) {
    assert_eq!(store.save(bytes), Ok(()), "save failed");
}

fn holds_nothing_at_first(make: &mut MakeCompileCache<'_>) {
    let store = make();
    assert_eq!(load(store.as_ref()), None);
}

fn gives_back_the_last_blob_saved(make: &mut MakeCompileCache<'_>) {
    let store = make();
    save(store.as_ref(), b"a first, longer blob");
    assert_eq!(load(store.as_ref()).as_deref(), Some(&b"a first, longer blob"[..]));
    save(store.as_ref(), b"short");
    assert_eq!(load(store.as_ref()).as_deref(), Some(&b"short"[..]), "a save replaces the whole blob");
    save(store.as_ref(), b"");
    assert_eq!(load(store.as_ref()), Some(Vec::new()), "an empty blob is still a blob");
}
