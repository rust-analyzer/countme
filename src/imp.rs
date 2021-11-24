use std::{
    any::{type_name, TypeId},
    hash::BuildHasherDefault,
    os::raw::c_int,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use rustc_hash::FxHasher;

use crate::{AllCounts, Counts};

static ENABLE: AtomicBool = AtomicBool::new(cfg!(feature = "print_at_exit"));

type GlobalStore = DashMap<TypeId, Store, BuildHasherDefault<FxHasher>>;

#[inline]
fn global_store() -> &'static GlobalStore {
    static MAP: OnceCell<GlobalStore> = OnceCell::new();
    MAP.get_or_init(|| {
        if cfg!(feature = "print_at_exit") {
            extern "C" {
                fn atexit(f: extern "C" fn()) -> c_int;
            }
            extern "C" fn print_at_exit() {
                eprint!("{}", get_all());
            }
            unsafe {
                atexit(print_at_exit);
            }
        }

        GlobalStore::default()
    })
}

pub(crate) fn enable(yes: bool) {
    ENABLE.store(yes, Relaxed);
}

#[inline]
fn enabled() -> bool {
    ENABLE.load(Relaxed)
}

#[inline]
pub(crate) fn dec<T: 'static>() {
    if enabled() {
        do_dec(TypeId::of::<T>())
    }
}
#[inline(never)]
fn do_dec(key: TypeId) {
    if let Some(store) = global_store().get(&key) {
        store.value().dec();
    }
}

#[inline]
pub(crate) fn inc<T: 'static>() {
    if enabled() {
        do_inc(TypeId::of::<T>(), type_name::<T>())
    }
}
#[inline(never)]
fn do_inc(key: TypeId, name: &'static str) {
    global_store().entry(key).or_insert_with(|| Store { name, ..Store::default() }).value().inc();
}

pub(crate) fn get<T: 'static>() -> Counts {
    do_get(TypeId::of::<T>())
}
fn do_get(key: TypeId) -> Counts {
    global_store().entry(key).or_default().value().read()
}

pub(crate) fn get_all() -> AllCounts {
    let mut entries = global_store()
        .iter()
        .map(|entry| {
            let store = entry.value();
            (store.type_name(), store.read())
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(name, _counts)| *name);
    AllCounts { entries }
}

#[derive(Default)]
struct Store {
    total: AtomicUsize,
    max_live: AtomicUsize,
    live: AtomicUsize,
    name: &'static str,
}

impl Store {
    fn inc(&self) {
        self.total.fetch_add(1, Relaxed);
        let live = self.live.fetch_add(1, Relaxed) + 1;
        self.max_live.fetch_max(live, Relaxed);
    }

    fn dec(&self) {
        self.live.fetch_sub(1, Relaxed);
    }

    fn read(&self) -> Counts {
        Counts {
            total: self.total.load(Relaxed),
            max_live: self.max_live.load(Relaxed),
            live: self.live.load(Relaxed),
        }
    }

    fn type_name(&self) -> &'static str {
        self.name
    }
}
