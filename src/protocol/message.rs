//! Zero-Copy Message Format (SBE-inspired)
//!
//! Layout:
//! ┌─────────────────────────────────────────────────────┐
//! │ MessageHeader (32 bytes, fixed)                     │
//! ├─────────────────────────────────────────────────────┤
//! │ Payload (variable, max 64KB)                        │
//! └─────────────────────────────────────────────────────┘
//!
//! Header dapat di-cast langsung dari byte buffer tanpa parsing.

#![allow(dead_code)] // All message types are part of the protocol API

use std::mem;

/// Tipe pesan dalam Hermes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Data publish dari producer
    Publish = 1,
    /// Subscribe request dari consumer
    Subscribe = 2,
    /// Acknowledgment
    Ack = 3,
    /// Heartbeat untuk connection keep-alive
    Heartbeat = 4,
    /// Batch of messages
    Batch = 5,
}

impl MessageType {
    #[inline(always)]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Publish),
            2 => Some(Self::Subscribe),
            3 => Some(Self::Ack),
            4 => Some(Self::Heartbeat),
            5 => Some(Self::Batch),
            _ => None,
        }
    }
}

/// Message Header - Fixed 32 bytes, cache-line friendly
///
/// Dapat di-cast langsung dari raw bytes (zero-copy read).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    /// Magic number untuk validasi (0x48524D53 = "HRMS")
    pub magic: u32,
    /// Versi protokol
    pub version: u8,
    /// Tipe pesan
    pub msg_type: u8,
    /// Flags (reserved untuk future use)
    pub flags: u16,
    /// Sequence number untuk ordering
    pub sequence: u64,
    /// Timestamp dalam nanoseconds (epoch)
    pub timestamp_ns: u64,
    /// Panjang payload dalam bytes
    pub payload_len: u32,
    /// CRC32 checksum payload
    pub checksum: u32,
}

pub const HEADER_SIZE: usize = mem::size_of::<MessageHeader>();
pub const MAGIC: u32 = 0x48524D53; // "HRMS"
pub const VERSION: u8 = 1;
pub const MAX_PAYLOAD_SIZE: usize = 65536; // 64KB max payload

impl MessageHeader {
    /// Membuat header baru
    #[inline(always)]
    pub fn new(msg_type: MessageType, sequence: u64, payload_len: u32) -> Self {
        Self {
            magic: MAGIC,
            version: VERSION,
            msg_type: msg_type as u8,
            flags: 0,
            sequence,
            timestamp_ns: Self::now_ns(),
            payload_len,
            checksum: 0, // Akan diisi saat encode
        }
    }

    /// Timestamp saat ini dalam nanoseconds
    #[inline(always)]
    fn now_ns() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    /// Validasi header
    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
            && self.version == VERSION
            && self.payload_len as usize <= MAX_PAYLOAD_SIZE
    }

    /// Cast dari raw bytes (ZERO-COPY!)
    ///
    /// # Safety
    /// Buffer harus berisi data valid dan aligned
    #[inline(always)]
    pub unsafe fn from_bytes(buf: &[u8]) -> Option<&Self> {
        if buf.len() < HEADER_SIZE {
            return None;
        }
        let header = &*(buf.as_ptr() as *const Self);
        if header.is_valid() {
            Some(header)
        } else {
            None
        }
    }

    /// Convert ke bytes (ZERO-COPY!)
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, HEADER_SIZE) }
    }

    /// Total message size (header + payload)
    #[inline(always)]
    pub fn total_size(&self) -> usize {
        HEADER_SIZE + self.payload_len as usize
    }
}

/// Complete message dengan header dan payload reference
#[derive(Debug)]
pub struct Message<'a> {
    pub header: MessageHeader,
    pub payload: &'a [u8],
}

impl<'a> Message<'a> {
    /// Parse message dari buffer (zero-copy untuk payload)
    #[inline(always)]
    pub fn from_bytes(buf: &'a [u8]) -> Option<Self> {
        if buf.len() < HEADER_SIZE {
            return None;
        }

        let header = unsafe { *(buf.as_ptr() as *const MessageHeader) };

        if !header.is_valid() {
            return None;
        }

        let payload_end = HEADER_SIZE + header.payload_len as usize;
        if buf.len() < payload_end {
            return None;
        }

        Some(Self {
            header,
            payload: &buf[HEADER_SIZE..payload_end],
        })
    }
}

/// CRC32 checksum (simple, fast)
#[inline(always)]
pub fn crc32_fast(data: &[u8]) -> u32 {
    // Simple Adler-32 variant untuk speed
    let mut a: u32 = 1;
    let mut b: u32 = 0;

    for &byte in data {
        a = a.wrapping_add(byte as u32);
        b = b.wrapping_add(a);
    }

    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        // Pastikan header size sesuai ekspektasi
        assert_eq!(HEADER_SIZE, 32);
    }

    #[test]
    fn test_header_roundtrip() {
        let header = MessageHeader::new(MessageType::Publish, 42, 100);
        let bytes = header.as_bytes();

        let parsed = unsafe { MessageHeader::from_bytes(bytes) }.unwrap();
        // Copy fields to avoid unaligned reference
        let seq = parsed.sequence;
        let len = parsed.payload_len;
        assert_eq!(seq, 42);
        assert_eq!(len, 100);
    }

    #[test]
    fn test_message_parse() {
        let mut buf = vec![0u8; 64];
        let header = MessageHeader::new(MessageType::Publish, 1, 10);

        // Copy header ke buffer
        buf[..HEADER_SIZE].copy_from_slice(header.as_bytes());
        // Isi payload
        buf[HEADER_SIZE..HEADER_SIZE + 10].copy_from_slice(b"HelloWorld");

        let msg = Message::from_bytes(&buf).unwrap();
        assert_eq!(msg.payload, b"HelloWorld");
    }
}
