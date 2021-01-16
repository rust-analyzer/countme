//! A library to quickly get the live/total/max counts of allocated instances.
//!
//! To use:
//!
//! * Add a [`Token<T>`] to your type. [`Token<T>`] is a RAII guard that
//!   manipulates counts in `new` / `drop`
//! * Implement [`CountMe`] trait for your type, to define an atomic storage for
//!   the type-specific counts.
//! * Optionally, override [`CountMe::on_new_max`] / [`CountMe::on_zero_live`]
//!   hooks to, eg, print the counts.
//! * Use [`countme::get::<T>()`][`get`] function to fetch the counts.
//!
//! To disable the counting, use the `no-op` feature of the crate.
//!
//! # Example
//!
//! ```
//! #[derive(Default)]
//! struct Widget {
//!   _t: countme::Token<Self>,
//! }
//!
//! impl countme::CountMe for Widget {
//!     fn store() -> &'static countme::Store {
//!         static S: countme::Store = countme::Store::new();
//!         &S
//!     }
//! }
//!
//! let w1 = Widget::default();
//! let w2 = Widget::default();
//! let w3 = Widget::default();
//! drop(w1);
//!
//! let counts = countme::get::<Widget>();
//! assert_eq!(counts.live, 2);
//! assert_eq!(counts.max, 3);
//! assert_eq!(counts.total, 3);
//! ```

use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Counts {
    /// The number of tokens which were created, but are not destroyed yet.
    pub live: usize,
    /// The total number of tokens created.
    pub total: usize,
    /// The historical maximum of the `live` count.
    pub max: usize,
}

/// Returns the counts for the `T` type.
pub fn get<T: CountMe>() -> Counts {
    let store = T::store();
    Counts {
        live: store.live.load(Relaxed),
        total: store.total.load(Relaxed),
        max: store.max.load(Relaxed),
    }
}

/// Implement this for a type you wish to count.
pub trait CountMe {
    /// Override this to return a reference to a local static:
    ///
    /// ```
    /// # struct Widget;
    ///
    /// impl countme::CountMe for Widget {
    ///     fn store() -> &'static countme::Store {
    ///         static S: countme::Store = countme::Store::new();
    ///         &S
    ///     }
    /// }
    /// ```
    fn store() -> &'static Store;

    /// Override this to get notified when the maximum number of concurrent
    /// instances increases.
    #[inline]
    fn on_new_max() {}

    /// Override this to get notified when all instances are dead.
    ///
    /// Useful to print stats at the end of the program.
    #[inline]
    fn on_zero_live() {}
}

pub struct Store {
    live: AtomicUsize,
    total: AtomicUsize,
    max: AtomicUsize,
}

impl Store {
    pub const fn new() -> Store {
        Store { live: AtomicUsize::new(0), total: AtomicUsize::new(0), max: AtomicUsize::new(0) }
    }
}

/// Store this inside your struct as `_t: countme::Token<Self>`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token<T: CountMe> {
    ghost: PhantomData<fn(T)>,
}

impl<T: CountMe> Default for Token<T> {
    fn default() -> Self {
        Token::alloc()
    }
}

impl<T: CountMe> Clone for Token<T> {
    fn clone(&self) -> Self {
        Self::alloc()
    }
}

impl<T: CountMe> Token<T> {
    pub fn alloc() -> Token<T> {
        #[cfg(not(feature = "no-op"))]
        {
            let store = T::store();
            store.total.fetch_add(1, Relaxed);
            let live = store.live.fetch_add(1, Relaxed) + 1;
            let mut max = 0;
            loop {
                max = match store.max.compare_exchange_weak(max, live, Relaxed, Relaxed) {
                    Ok(_) => {
                        T::on_new_max();
                        break;
                    }
                    Err(max) if live <= max => break,
                    Err(max) => max,
                };
            }
        }
        Token { ghost: PhantomData }
    }
}

#[cfg(not(feature = "no-op"))]
impl<T: CountMe> Drop for Token<T> {
    fn drop(&mut self) {
        let store = T::store();
        let live = store.live.fetch_sub(1, Relaxed) - 1;
        if live == 0 {
            T::on_zero_live()
        }
    }
}
