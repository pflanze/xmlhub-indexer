use std::{
    fmt::Debug,
    fs::OpenOptions,
    io::Write,
    mem::transmute,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
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
}

/// A filesystem path based way to poll for 'signals';
pub struct PollingSignals {
    seen: u64,
    atomic: IPCAtomicU64,
}

impl PollingSignals {
    pub fn open(path: &Path, initial_value: u64) -> Result<Self, IPCAtomicError> {
        let atomic = IPCAtomicU64::open(path, initial_value)?;
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

    /// Send one signal. This is excluded from this `PollingSignals`
    /// instance, i.e. `get_number_of_signals()` will not report
    /// it. Returns the previous value.
    pub fn send_signal(&mut self) -> u64 {
        let Self { seen, atomic } = self;
        *seen = seen.wrapping_add(1);
        atomic.inc()
    }

    /// Send one signal. This is *not* excluded from this
    /// `PollingSignals` instance, i.e. `get_number_of_signals()` will
    /// report it. Returns the previous value.
    pub fn send_signal_out(&self) -> u64 {
        self.atomic.inc()
    }
}
