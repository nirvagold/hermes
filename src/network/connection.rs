//! Connection handling dengan buffered I/O
//!
//! Pre-allocated buffers untuk zero-allocation pada hot path.
//!
//! Note: This module provides connection utilities for the mio-based server.
//! For production use, see src/bin/hermes_server.rs.

#![allow(dead_code)] // Connection module is for future async implementation

use std::io::{self, Read, Write};
use std::net::TcpStream;

/// Buffer sizes - tuned untuk typical message sizes
const READ_BUFFER_SIZE: usize = 64 * 1024; // 64KB
const WRITE_BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// High-performance connection wrapper
///
/// Menggunakan pre-allocated buffers untuk menghindari
/// alokasi pada setiap read/write.
pub struct Connection {
    stream: TcpStream,
    read_buffer: Box<[u8]>,
    write_buffer: Box<[u8]>,
    read_pos: usize,
    read_len: usize,
    write_pos: usize,
}

impl Connection {
    /// Wrap TcpStream dengan buffered I/O
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        // Set non-blocking mode
        stream.set_nonblocking(true)?;

        // Disable Nagle's algorithm untuk lower latency
        stream.set_nodelay(true)?;

        Ok(Self {
            stream,
            read_buffer: vec![0u8; READ_BUFFER_SIZE].into_boxed_slice(),
            write_buffer: vec![0u8; WRITE_BUFFER_SIZE].into_boxed_slice(),
            read_pos: 0,
            read_len: 0,
            write_pos: 0,
        })
    }

    /// Read data ke internal buffer
    ///
    /// Returns jumlah bytes yang tersedia untuk dibaca.
    #[inline]
    pub fn fill_read_buffer(&mut self) -> io::Result<usize> {
        // Compact buffer jika perlu
        if self.read_pos > 0 {
            let remaining = self.read_len - self.read_pos;
            if remaining > 0 {
                self.read_buffer
                    .copy_within(self.read_pos..self.read_len, 0);
            }
            self.read_len = remaining;
            self.read_pos = 0;
        }

        // Read dari socket
        match self.stream.read(&mut self.read_buffer[self.read_len..]) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "Connection closed",
            )),
            Ok(n) => {
                self.read_len += n;
                Ok(self.read_len)
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                Ok(self.read_len - self.read_pos)
            }
            Err(e) => Err(e),
        }
    }

    /// Get readable data slice (zero-copy)
    #[inline(always)]
    pub fn readable(&self) -> &[u8] {
        &self.read_buffer[self.read_pos..self.read_len]
    }

    /// Consume n bytes dari read buffer
    #[inline(always)]
    pub fn consume(&mut self, n: usize) {
        self.read_pos += n.min(self.read_len - self.read_pos);
    }

    /// Queue data untuk write (copy ke write buffer)
    #[inline]
    pub fn queue_write(&mut self, data: &[u8]) -> io::Result<()> {
        if self.write_pos + data.len() > self.write_buffer.len() {
            // Flush dulu jika buffer penuh
            self.flush_write_buffer()?;
        }

        if data.len() > self.write_buffer.len() {
            // Data terlalu besar, write langsung
            return self.stream.write_all(data);
        }

        self.write_buffer[self.write_pos..self.write_pos + data.len()].copy_from_slice(data);
        self.write_pos += data.len();

        Ok(())
    }

    /// Flush write buffer ke socket
    #[inline]
    pub fn flush_write_buffer(&mut self) -> io::Result<()> {
        if self.write_pos == 0 {
            return Ok(());
        }

        let mut written = 0;
        while written < self.write_pos {
            match self
                .stream
                .write(&self.write_buffer[written..self.write_pos])
            {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Failed to write to socket",
                    ));
                }
                Ok(n) => written += n,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Partial write, compact buffer
                    if written > 0 {
                        self.write_buffer.copy_within(written..self.write_pos, 0);
                        self.write_pos -= written;
                    }
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        }

        self.write_pos = 0;
        Ok(())
    }

    /// Get underlying stream untuk polling
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Bytes pending in write buffer
    #[inline(always)]
    pub fn write_pending(&self) -> usize {
        self.write_pos
    }
}

#[cfg(test)]
mod tests {
    // Network tests memerlukan actual socket, skip untuk unit test
}
