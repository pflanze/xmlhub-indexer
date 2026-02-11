use std::{
    fmt::Debug,
    fs::OpenOptions,
    io::Write,
    mem::transmute,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use memmap2::{MmapMut, MmapOptions};

type PollingSignalsAtomic = AtomicU64;

/// A filesystem path based cross-process atomic counter
pub struct IPCAtomicU64 {
    mmap: MmapMut,
}

impl Debug for IPCAtomicU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IPCAtomicU64").finish()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IPCAtomicError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("invalid length {0} of file contents")]
    InvalidFileContentsLength(u64),
}

impl IPCAtomicU64 {
    pub fn open(path: &Path, initial_value: u64) -> Result<Self, IPCAtomicError> {
        let mut opts = OpenOptions::new();
        opts.read(true);
        opts.write(true);
        opts.truncate(false);
        opts.create(true);
        opts.mode(0o600); // XX how to make portable?
        let mut file = opts.open(path)?;
        let m = file.metadata()?;
        let l = m.len();
        const PSA_SIZE: usize = size_of::<PollingSignalsAtomic>();
        const PSA_LEN: u64 = PSA_SIZE as u64;
        match l {
            0 => {
                let a = PollingSignalsAtomic::new(initial_value);
                let b: &[u8; PSA_SIZE] = unsafe { transmute(&a) };
                file.write_all(b)?;
            }
            PSA_LEN => (),
            _ => Err(IPCAtomicError::InvalidFileContentsLength(l))?,
        }
        let mmap = unsafe { MmapOptions::new().len(PSA_SIZE).map(&file)?.make_mut()? };
        Ok(Self { mmap })
    }

    #[inline]
    pub fn atomic(&self) -> &AtomicU64 {
        let Self { mmap } = self;
        let value: &[u8; size_of::<PollingSignalsAtomic>()] = (&(**mmap)
            [0..size_of::<PollingSignalsAtomic>()])
            .try_into()
            .expect("same size of PollingSignalsAtomic bytes");
        let ptr = value.as_ptr() as *const AtomicU64;
        unsafe { &*ptr }
    }

    #[inline]
    pub fn load(&self) -> u64 {
        self.atomic().load(Ordering::SeqCst)
    }

    #[inline]
    pub fn store(&self, val: u64) {
        self.atomic().store(val, Ordering::SeqCst)
    }

    #[inline]
    pub fn inc(&self) -> u64 {
        self.atomic().fetch_add(1, Ordering::SeqCst)
    }

    /// Returns the last-seen value from reading when `f` returns None
    /// as Err; returns Ok with the new value if `f` returned Some
    /// (and storing succeeded--if it failed, it retries until `f`
    /// returns None).
    #[inline]
    pub fn fetch_update(&self, f: impl FnMut(u64) -> Option<u64>) -> Result<u64, u64> {
        self.atomic()
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, f)
    }
}

/// A filesystem path based way to poll for 'signals'. Cloning makes
/// another receiver that checks independently from the original (but
/// both receive the same 'signals').
#[derive(Debug, Clone)]
pub struct PollingSignals {
    seen: u64,
    atomic: Arc<IPCAtomicU64>,
}

/// A filesystem path based way to poll for 'signals'. When receiving
/// a signal, it has to be confirmed explicitly, and the confirmation
/// is shared amongst all readers (usable for "work was done" on a
/// 'versioned' input; the work confirmed refers to the version of the
/// signal that was read; the confirmation with the highest signal
/// value is stored). Note that multiple readers might start doing the
/// same work at the same time. Cloning shares the same receiver and
/// sender.
#[derive(Debug, Clone)]
pub struct SharedPollingSignals {
    done: Arc<IPCAtomicU64>,
    atomic: Arc<IPCAtomicU64>,
}

/// Sending-only 'end' of a `PollingSignals` or
/// `SharedPollingSignals`. Can be cloned to give yet another sending
/// end.
#[derive(Debug, Clone)]
pub struct PollingSignalsSender {
    atomic: Arc<IPCAtomicU64>,
}

impl PollingSignals {
    pub fn open(path: &Path, initial_value: u64) -> Result<Self, IPCAtomicError> {
        let atomic = IPCAtomicU64::open(path, initial_value)?.into();
        let mut s = Self {
            seen: initial_value,
            atomic,
        };
        s.get_number_of_signals();
        Ok(s)
    }

    /// Check how many signals were received since the last check.
    pub fn get_number_of_signals(&mut self) -> u64 {
        let Self { seen, atomic } = self;
        let new_seen = atomic.load();
        let d = new_seen.wrapping_sub(*seen);
        *seen = new_seen;
        d
    }

    /// Check whether there were any signals (just
    /// `get_number_of_signals() > 0`)
    pub fn got_signals(&mut self) -> bool {
        self.get_number_of_signals() > 0
    }

    /// Send one signal. This is excluded from this `PollingSignals`
    /// instance, i.e. `get_number_of_signals()` will not report
    /// it. Returns the previous value.
    pub fn send_signal(&mut self) -> u64 {
        let Self { seen, atomic } = self;
        *seen = seen.wrapping_add(1);
        atomic.inc()
    }

    pub fn sender(&self) -> PollingSignalsSender {
        let atomic = self.atomic.clone();
        PollingSignalsSender { atomic }
    }
}

/// A received signal. This is a (dynamically-checked) 'linear type'
/// value: it must be confirmed via `confirm` or `ignore`; dropping it
/// panics!
#[derive(Debug)]
#[must_use]
pub struct Signal<'t> {
    seen: u64,
    done: &'t IPCAtomicU64,
    // careful: this struct is forgotten, do not add any non-Copy
    // types!
}

impl<'t> Drop for Signal<'t> {
    fn drop(&mut self) {
        panic!("{self:?} must be passed to the confirm or ignore methods but was dropped")
    }
}

impl<'t> Signal<'t> {
    /// Returns true if we prevailed, false if another actor updated
    /// past us (which is usually fine, too).
    pub fn confirm(self) -> bool {
        let Self { seen, done } = self;
        std::mem::forget(self);
        match done.fetch_update(|new_seen| if seen > new_seen { Some(seen) } else { None }) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn ignore(self) {
        std::mem::forget(self)
    }
}

impl SharedPollingSignals {
    pub fn open(
        channel_path: &Path,
        done_path: &Path,
        initial_value: u64,
    ) -> Result<Self, IPCAtomicError> {
        let atomic = IPCAtomicU64::open(channel_path, initial_value)?.into();
        let done = IPCAtomicU64::open(done_path, initial_value)?.into();
        Ok(Self { done, atomic })
    }

    /// If there were signals, returns a representation of the last
    /// one. `confirm` or `ignore` must be called on it when the
    /// action warranted by the signal has been carried out, dropping
    /// it will panic!
    pub fn get_latest_signal(&self) -> Option<Signal<'_>> {
        let Self { done, atomic } = self;
        // XX these are two separate loads. Still OK since loading
        // with SeqCst and we have a sequence?
        let seen = atomic.load();
        let done_value = done.load();
        if seen > done_value {
            Some(Signal {
                seen,
                done: &*self.done,
            })
        } else {
            None
        }
    }

    pub fn sender(&self) -> PollingSignalsSender {
        let atomic = self.atomic.clone();
        PollingSignalsSender { atomic }
    }

    // Do we really want a send_signal() method here? Just always use
    // sender().send_signal() instead?
}

impl PollingSignalsSender {
    /// Send one signal. Returns the previous value.
    pub fn send_signal(&self) -> u64 {
        self.atomic.inc()
    }
}
