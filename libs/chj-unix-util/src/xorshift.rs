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
