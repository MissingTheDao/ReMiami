//! reVC audio building blocks that don't depend on the C++ game, so they can be unit-tested
//! on their own. Device output lives in the `revc` crate (src/audio/mod.rs).

pub mod al;
pub mod mixer;
