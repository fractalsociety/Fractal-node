//! revm integration + native precompile bridge (M4+). M3: address routing + calldata decode.

mod precompile;

pub use precompile::{decode_native_calldata, is_fractal_native_precompile, native_opcode};
