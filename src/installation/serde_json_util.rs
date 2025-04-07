use std::io::Read;

use anyhow::{bail, Result};
use serde::de::DeserializeOwned;

/// Read individual JSON items from a stream
pub fn serde_json_reader<T: DeserializeOwned, R: Read>(
    input: R,
) -> impl Iterator<Item = Result<T, serde_json::Error>> {
    serde_json::Deserializer::from_reader(input).into_iter()
}

/// Read individual JSON items from a stream
pub fn serde_json_maybe_read1<T: DeserializeOwned, R: Read>(
    input: R,
) -> Result<Option<T>, serde_json::Error> {
    let mut iter = serde_json_reader::<T, R>(input);
    iter.next().transpose()
}

/// Read exactly one JSON item from a stream
pub fn serde_json_read1<T: DeserializeOwned, R: Read>(input: R) -> Result<T> {
    if let Some(item) = serde_json_maybe_read1::<T, R>(input)? {
        Ok(item)
    } else {
        bail!("premature EOF reading JSON item")
    }
}
