//! Network Layer: High-Performance Async I/O
//!
//! Menggunakan mio untuk cross-platform async I/O.
//! Pada Linux, bisa di-upgrade ke io_uring untuk performa maksimal.
//!
//! Fitur:
//! - Non-blocking I/O dengan epoll/kqueue/IOCP
//! - Connection pooling
//! - Batching untuk mengurangi syscall overhead
//!
//! Note: For production server, see src/bin/hermes_server.rs

mod connection;
mod server;

// Re-exports for library users (mio-based implementation)
#[allow(unused_imports)]
pub use connection::Connection;
