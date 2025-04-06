// COPY from evobench git.rs

use anyhow::{bail, Result};

pub fn decode_hex_digit(b: u8) -> Result<u8> {
    if b >= b'0' && b <= b'9' {
        Ok(b - b'0')
    } else if b >= b'a' && b <= b'f' {
        Ok(b - b'a' + 10)
    } else if b >= b'A' && b <= b'F' {
        Ok(b - b'A' + 10)
    } else {
        bail!("byte is not a hex digit: {b}")
    }
}

pub fn decode_hex<const N: usize>(input: &[u8], output: &mut [u8; N]) -> Result<()> {
    let n2 = 2 * N;
    if input.len() != n2 {
        bail!(
            "wrong number of hex digits, expect {n2}, got {}",
            input.len()
        )
    }
    for i in 0..N {
        output[i] = decode_hex_digit(input[i * 2])? * 16 + decode_hex_digit(input[i * 2 + 1])?;
    }
    Ok(())
}

// /COPY

fn unchecked_lc_encode_hex_digit(b: u8) -> u8 {
    if b < 10 {
        b + b'0'
    } else {
        b - 10 + b'a'
    }
}

pub fn encode_hex(input: &[u8], output: &mut Vec<u8>) -> () {
    for b in input {
        output.push(unchecked_lc_encode_hex_digit(b >> 4));
        output.push(unchecked_lc_encode_hex_digit(b & 15));
    }
}

pub fn to_hex_string(input: &[u8]) -> String {
    let mut out = Vec::new();
    encode_hex(input, &mut out);
    String::from_utf8(out).expect("producing valid ascii")
}

#[test]
fn t_encode_hex() {
    let t = to_hex_string;
    assert_eq!(t(b"Hello\0"), "48656c6c6f00");
    assert_eq!(t(&[9, 10, 11, 15, 16, 254, 255, 0]), "090a0b0f10feff00");
}
