use std::{
    any::type_name,
    hash::BuildHasherDefault,
    os::raw::c_int,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use rustc_hash::FxHasher;

use crate::{AllCounts, Counts};

static ENABLE: AtomicBool = AtomicBool::new(true);

type GlobalStore = DashMap<&'static str, Store, BuildHasherDefault<FxHasher>>;

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
pub(crate) fn dec<T>() {
    if enabled() {
        do_dec(type_name::<T>())
    }
}
#[inline(never)]
fn do_dec(key: &'static str) {
    global_store().entry(&key).or_default().value().dec();
}

#[inline]
pub(crate) fn inc<T>() {
    if enabled() {
        do_inc(type_name::<T>())
    }
}
#[inline(never)]
fn do_inc(key: &'static str) {
    global_store().entry(&key).or_default().value().inc();
}

pub(crate) fn get<T>() -> Counts {
    do_get(type_name::<T>())
}
fn do_get(key: &'static str) -> Counts {
    global_store().entry(&key).or_default().value().read()
}

pub(crate) fn get_all() -> AllCounts {
    let entries = global_store().iter().map(|entry| (*entry.key(), entry.value().read())).collect();
    AllCounts { entries }
}

#[derive(Default)]
struct Store {
    total: AtomicUsize,
    max_live: AtomicUsize,
    live: AtomicUsize,
}

impl Store {
    fn inc(&self) {
        self.total.fetch_add(1, Relaxed);
        let live = self.live.fetch_add(1, Relaxed) + 1;

        let mut max = 0;
        loop {
            max = match self.max_live.compare_exchange_weak(max, live, Relaxed, Relaxed) {
                Err(max) if max < live => max,
                Err(_) | Ok(_) => break,
            };
        }
    }

    fn dec(&self) -> bool {
        let live = self.live.fetch_sub(1, Relaxed) - 1;
        live == 0
    }

    fn read(&self) -> Counts {
        Counts {
            total: self.total.load(Relaxed),
            max_live: self.max_live.load(Relaxed),
            live: self.live.load(Relaxed),
        }
    }
}
