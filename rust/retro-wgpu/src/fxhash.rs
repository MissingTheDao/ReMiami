//! A small, fast hasher for the renderer's caches (pipelines, samplers, bind groups, textures).
//! Their keys are small trusted values looked up several times per draw; std's SipHash showed up
//! at the top of profiles. This is the Fx hash (rotate, xor, multiply per word), as rustc uses.

use std::hash::{BuildHasher, Hasher};

#[derive(Clone, Copy, Default)]
pub struct FxBuild;

pub struct FxHasher(u64);

const K: u64 = 0xf135_7aea_2e62_a9c5;

impl FxHasher {
    #[inline]
    fn add(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(26) ^ x).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            self.add(u64::from_le_bytes(*c));
        }
        if !rest.is_empty() {
            let mut b = [0u8; 8];
            b[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(b) ^ ((rest.len() as u64) << 56));
        }
    }
    #[inline]
    fn write_u8(&mut self, x: u8) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u16(&mut self, x: u16) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u32(&mut self, x: u32) {
        self.add(x as u64);
    }
    #[inline]
    fn write_u64(&mut self, x: u64) {
        self.add(x);
    }
    #[inline]
    fn write_usize(&mut self, x: usize) {
        self.add(x as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

impl BuildHasher for FxBuild {
    type Hasher = FxHasher;
    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher(0)
    }
}

pub type HashMap<K, V> = std::collections::HashMap<K, V, FxBuild>;
pub type HashSet<K> = std::collections::HashSet<K, FxBuild>;
