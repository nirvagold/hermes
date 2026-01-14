//! Core module: Lock-Free Ring Buffer dengan Mmap backing
//!
//! Prinsip desain:
//! - Zero-Copy: Data langsung di-mmap, tidak ada copy ke user space
//! - Lock-Free: Hanya atomic operations, tidak ada Mutex/RwLock
//! - No-Allocation: Semua buffer pre-allocated saat init

mod mmap_storage;
mod ring_buffer;

pub use mmap_storage::MmapStorage;
pub use ring_buffer::RingBuffer;
