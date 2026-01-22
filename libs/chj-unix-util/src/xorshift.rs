//! Avoid dependency on `rand` crate

// struct Xorshift64(pub u64);

// impl Xorshift64 {
//     pub fn new(seed: u64) -> Self {
//         if seed == 0 {
//             panic!("seed must not be 0")
//         }
//         Self(seed)
//     }

//     pub fn get(&mut self) -> u64 {
//         self.0 ^= self.0 << 13;
//         self.0 ^= self.0 >> 17;
//         self.0 ^= self.0 << 5;
//         self.0
//     }
// }

// https://handwiki.org/wiki/Xorshift

pub struct Splitmix64(pub u64);

impl Splitmix64 {
    pub fn get(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut result = self.0;
        result = (result ^ (result >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        result = (result ^ (result >> 27)).wrapping_mul(0x94D049BB133111EB);
        result ^ (result >> 31)
    }
}

pub struct Xorshift128plus(pub [u64; 2]);

impl Xorshift128plus {
    pub fn new(seed: u64) -> Self {
        let mut s = Splitmix64(seed);
        Self([s.get(), s.get()])
    }

    pub fn get(&mut self) -> u64 {
        let mut t = self.0[0];
        let s = self.0[1];
        self.0[0] = s;
        t ^= t << 23;
        t ^= t >> 18;
        t ^= s ^ (s >> 5);
        self.0[1] = t;
        t.wrapping_add(s)
    }
}

#[test]
fn t_splitmix64() {
    let mut s = Splitmix64(0);
    assert_eq!(s.get(), 16294208416658607535);
    assert_eq!(s.get(), 7960286522194355700);
    assert_eq!(s.get(), 487617019471545679);
    assert_eq!(s.get(), 17909611376780542444);
    for _ in 0..1000000 - 4 {
        s.get();
    }
    assert_eq!(s.get(), 14850574393604363050);
    assert_eq!(s.get(), 1562119273537874705);
    assert_eq!(s.get(), 1986060996022186059);
    assert_eq!(s.get(), 17599710064536246394);
    assert_eq!(s.get(), 9868005493142914005);
}

#[test]
fn t_xorshift128plus() {
    let mut s = Xorshift128plus::new(0);
    assert_eq!(s.get(), 148304652509113927);
    assert_eq!(s.get(), 6897519897668720478);
    assert_eq!(s.get(), 8466708535677759538);
    assert_eq!(s.get(), 4573841993332567017);
    for _ in 0..1000000 - 4 {
        s.get();
    }
    assert_eq!(s.get(), 7831888472505485542);
    assert_eq!(s.get(), 9546132825291831751);
    assert_eq!(s.get(), 10667650326493356499);
    assert_eq!(s.get(), 13170528539071654594);
    assert_eq!(s.get(), 16110659944319132175);
}

/* Yields the same results as:

#include <cassert>

// https://handwiki.org/wiki/Xorshift

#include <stdint.h>

struct splitmix64_state {
    uint64_t s;
};

uint64_t splitmix64(struct splitmix64_state *state) {
    uint64_t result = (state->s += 0x9E3779B97F4A7C15);
    result = (result ^ (result >> 30)) * 0xBF58476D1CE4E5B9;
    result = (result ^ (result >> 27)) * 0x94D049BB133111EB;
    return result ^ (result >> 31);
}


struct xorshift128p_state {
    uint64_t x[2];
};

/* The state must be seeded so that it is not all zero */
uint64_t xorshift128p(struct xorshift128p_state *state)
{
    uint64_t t = state->x[0];
    uint64_t const s = state->x[1];
    state->x[0] = s;
    t ^= t << 23;       // a
    t ^= t >> 18;       // b -- Again, the shifts and the multipliers are tunable
    t ^= s ^ (s >> 5);  // c
    state->x[1] = t;
    return t + s;
}

int main() {

    struct splitmix64_state smstate = {0};

    uint64_t tmp1 = splitmix64(&smstate);
    uint64_t tmp2 = splitmix64(&smstate);

    assert(tmp1 == 16294208416658607535ull);
    assert(tmp2 == 7960286522194355700ull);

    struct xorshift128p_state pstate = {{tmp1, tmp2}};

    uint64_t a;
    a = xorshift128p(&pstate); assert(a == 148304652509113927ull);
    a = xorshift128p(&pstate); assert(a == 6897519897668720478ull);
    a = xorshift128p(&pstate); assert(a == 8466708535677759538ull);
    a = xorshift128p(&pstate); assert(a == 4573841993332567017ull);

    for (int i = 0; i < 1000000 - 4; i++) {
        xorshift128p(&pstate);
    }

    a = xorshift128p(&pstate); assert(a == 7831888472505485542ull);
    a = xorshift128p(&pstate); assert(a == 9546132825291831751ull);
    a = xorshift128p(&pstate); assert(a == 10667650326493356499ull);
    a = xorshift128p(&pstate); assert(a == 13170528539071654594ull);
    a = xorshift128p(&pstate); assert(a == 16110659944319132175ull);

    return 0;
}
 */
