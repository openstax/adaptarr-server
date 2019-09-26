use std::{sync::atomic::{AtomicUsize, Ordering}, marker::PhantomData};

/// Structure holding possibly uninitialized data.
///
/// This differs from other similar types found on crates.io in that it doesn't
/// lock or synchronise access in any way, instead assuming it is safe to
/// initialize the value multiple times, and only keep one result.
#[derive(Debug)]
pub struct SingleInit<T> {
    cell: AtomicUsize,
    _type: PhantomData<T>,
}

impl<T> SingleInit<T> {
    /// Create a new uninitialized atomic cell.
    pub const fn uninit() -> Self {
        SingleInit {
            cell: AtomicUsize::new(0),
            _type: PhantomData,
        }
    }
}

impl<T> SingleInit<T>
where
    T: Sync,
    Self: 'static,
{
    /// Get stored value, or `None` if it hasn't been initialized yet.
    pub fn get(&self) -> Option<&'static T> {
        let ptr = self.cell.load(Ordering::Relaxed);

        if ptr != 0 {
            Some(unsafe { &*(ptr as *const T) })
        } else{
            None
        }
    }

    /// Get stored value, initializing it if necessary.
    pub fn get_or_init<F>(&self, init: F) -> &'static T
    where
        F: FnOnce() -> T,
    {
        self.get_or_try_init::<(), _>(|| Ok(init())).unwrap()
    }

    /// Same as [`get_or_init`] except that initialisation function can fail.
    ///
    /// If initialisation function fails, the value will be unchanged and
    /// another thread (or the same thread) can safely attempt to initialise it
    /// again.
    pub fn get_or_try_init<E, F>(&self, init: F) -> Result<&'static T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let ptr = self.cell.load(Ordering::Relaxed);

        if ptr != 0 {
            return Ok(unsafe { &*(ptr as *const T) });
        }

        // Create a new value, place it on heap, obtain reference to it, and
        // prevent destructor from running.
        let value = Box::leak(Box::new(init()?)) as *mut T;

        // Try to update cell.
        let old = self.cell.compare_and_swap(ptr, value as usize, Ordering::Relaxed);

        if old == ptr {
            // Update succeeded, value is now the value of cell.
            Ok(unsafe { &*value })
        } else {
            // Update failed, cell was initialised by another thread. In this
            // case we drop value and return old.
            std::mem::drop(unsafe { Box::from_raw(value) });
            Ok(unsafe { &*(old as *const T) })
        }
    }
}
