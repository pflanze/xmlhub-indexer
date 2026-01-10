use std::{num::NonZeroU32, thread::sleep, time::Duration};

use nix::sys::pthread::pthread_self;

use crate::xorshift::Xorshift128plus;

/// Rerun `f` until it returns Ok, only sleeps after 100 attempts,
/// panics after 200 attempts. Mostly useful for atomic
/// `compare_exchange`. I.e. do not use if `f` expects a value from
/// another thread; just do retries that should succeed except for bad
/// luck when another thread is making a change at the same time.
pub fn retry<R, E>(f: impl Fn() -> Result<R, E>) -> R {
    let mut tries_left: u32 = 200;
    let mut random = None;
    loop {
        match f() {
            Ok(r) => return r,
            Err(_) => (),
        }
        tries_left -= 1;
        if tries_left == 0 {
            panic!("can't seem to get this to succeed")
        }
        if tries_left < 100 {
            if random.is_none() {
                random = Some(Xorshift128plus::new(pthread_self()));
            }
            let r = random.as_mut().expect("initialized above").get();
            sleep(Duration::from_micros(r & 16383));
            eprintln!(
                "note: retrying with {tries_left} tries left via `retry` at {}:{}",
                file!(),
                line!()
            );
        }
    }
}

/// Rerun `f` until it returns Ok or `max_tries` has run out. Sleeps a
/// constant time between tries.
pub fn retry_n<R, E>(
    max_tries: NonZeroU32,
    sleep_ms: u64,
    f: impl Fn() -> Result<R, E>,
) -> Result<R, E> {
    let mut tries_left: u32 = max_tries.into();
    loop {
        match f() {
            Ok(r) => return Ok(r),
            Err(e) => {
                tries_left -= 1;
                if tries_left == 0 {
                    return Err(e);
                }
                sleep(Duration::from_millis(sleep_ms));
            }
        }
        eprintln!("note: retrying via `retry_n` at {}:{}", file!(), line!());
    }
}
