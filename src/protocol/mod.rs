//! Protocol Layer: Zero-Copy Binary Encoding
//!
//! Prinsip desain:
//! - Flat Binary: Data bisa di-cast langsung tanpa parsing
//! - Fixed-size headers: Predictable memory layout
//! - No allocation: Encode/decode langsung ke/dari buffer

mod encoder;
mod message;

pub use encoder::{Decoder, Encoder};
pub use message::{MessageType, HEADER_SIZE};
