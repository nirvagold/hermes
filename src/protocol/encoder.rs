//! Zero-Allocation Encoder/Decoder
//!
//! Encode dan decode langsung ke pre-allocated buffer.
//! Tidak ada alokasi setelah inisialisasi.

#![allow(dead_code)] // Batch encoding is part of the public API

use super::message::{crc32_fast, MessageHeader, MessageType, HEADER_SIZE, MAX_PAYLOAD_SIZE};

/// Pre-allocated encoder buffer
///
/// Semua operasi encode dilakukan ke buffer internal,
/// tidak ada alokasi dinamis.
pub struct Encoder {
    buffer: Box<[u8]>,
    write_pos: usize,
}

impl Encoder {
    /// Membuat encoder dengan buffer size tertentu
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity].into_boxed_slice(),
            write_pos: 0,
        }
    }

    /// Reset encoder untuk reuse
    #[inline(always)]
    pub fn reset(&mut self) {
        self.write_pos = 0;
    }

    /// Encode single message ke buffer
    ///
    /// Returns slice ke encoded data, atau None jika buffer penuh.
    #[inline(always)]
    pub fn encode(
        &mut self,
        msg_type: MessageType,
        sequence: u64,
        payload: &[u8],
    ) -> Option<&[u8]> {
        if payload.len() > MAX_PAYLOAD_SIZE {
            return None;
        }

        let total_size = HEADER_SIZE + payload.len();
        if self.write_pos + total_size > self.buffer.len() {
            return None;
        }

        let start = self.write_pos;

        // Buat header dengan checksum
        let checksum = crc32_fast(payload);
        let mut header = MessageHeader::new(msg_type, sequence, payload.len() as u32);
        header.checksum = checksum;

        // Copy header (zero-copy cast)
        self.buffer[start..start + HEADER_SIZE].copy_from_slice(header.as_bytes());

        // Copy payload
        self.buffer[start + HEADER_SIZE..start + total_size].copy_from_slice(payload);

        self.write_pos += total_size;

        Some(&self.buffer[start..self.write_pos])
    }

    /// Encode batch of messages
    ///
    /// Format batch:
    /// `[BatchHeader][Msg1][Msg2]...[MsgN]`
    #[inline(always)]
    pub fn encode_batch(
        &mut self,
        messages: &[(&[u8], u64)], // (payload, sequence)
    ) -> Option<&[u8]> {
        if messages.is_empty() {
            return None;
        }

        let start = self.write_pos;

        // Hitung total payload size untuk batch
        let mut total_payload_size = 0usize;
        for (payload, _) in messages {
            total_payload_size += HEADER_SIZE + payload.len();
        }

        // Batch header
        let batch_header = MessageHeader::new(
            MessageType::Batch,
            messages[0].1, // First sequence
            total_payload_size as u32,
        );

        if self.write_pos + HEADER_SIZE + total_payload_size > self.buffer.len() {
            return None;
        }

        // Write batch header
        self.buffer[self.write_pos..self.write_pos + HEADER_SIZE]
            .copy_from_slice(batch_header.as_bytes());
        self.write_pos += HEADER_SIZE;

        // Write individual messages
        for (payload, sequence) in messages {
            let checksum = crc32_fast(payload);
            let mut header =
                MessageHeader::new(MessageType::Publish, *sequence, payload.len() as u32);
            header.checksum = checksum;

            self.buffer[self.write_pos..self.write_pos + HEADER_SIZE]
                .copy_from_slice(header.as_bytes());
            self.write_pos += HEADER_SIZE;

            self.buffer[self.write_pos..self.write_pos + payload.len()].copy_from_slice(payload);
            self.write_pos += payload.len();
        }

        Some(&self.buffer[start..self.write_pos])
    }

    /// Get current buffer content
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.write_pos]
    }

    /// Available space in buffer
    #[inline(always)]
    pub fn available(&self) -> usize {
        self.buffer.len() - self.write_pos
    }
}

/// Zero-copy decoder
pub struct Decoder<'a> {
    buffer: &'a [u8],
    read_pos: usize,
}

impl<'a> Decoder<'a> {
    /// Membuat decoder dari buffer
    #[inline(always)]
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            read_pos: 0,
        }
    }

    /// Decode next message (zero-copy)
    #[inline(always)]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<(MessageHeader, &'a [u8])> {
        if self.read_pos + HEADER_SIZE > self.buffer.len() {
            return None;
        }

        // Zero-copy header read
        let header = unsafe { *(self.buffer.as_ptr().add(self.read_pos) as *const MessageHeader) };

        if !header.is_valid() {
            return None;
        }

        let payload_start = self.read_pos + HEADER_SIZE;
        let payload_end = payload_start + header.payload_len as usize;

        if payload_end > self.buffer.len() {
            return None;
        }

        // Verify checksum
        let payload = &self.buffer[payload_start..payload_end];
        if header.checksum != 0 && crc32_fast(payload) != header.checksum {
            return None; // Checksum mismatch
        }

        self.read_pos = payload_end;

        Some((header, payload))
    }

    /// Decode batch messages
    #[inline(always)]
    pub fn decode_batch(&mut self) -> Option<BatchIterator<'a>> {
        let (header, batch_payload) = self.next()?;

        if header.msg_type != MessageType::Batch as u8 {
            return None;
        }

        Some(BatchIterator {
            decoder: Decoder::new(batch_payload),
        })
    }

    /// Remaining bytes
    #[inline(always)]
    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.read_pos)
    }
}

/// Iterator untuk batch messages
pub struct BatchIterator<'a> {
    decoder: Decoder<'a>,
}

impl<'a> Iterator for BatchIterator<'a> {
    type Item = (MessageHeader, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        self.decoder.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_single() {
        let mut encoder = Encoder::new(4096);

        let payload = b"Hello, Hermes!";
        encoder.encode(MessageType::Publish, 1, payload).unwrap();

        let mut decoder = Decoder::new(encoder.as_bytes());
        let (header, decoded_payload) = decoder.next().unwrap();

        // Copy field to avoid unaligned reference
        let seq = header.sequence;
        assert_eq!(seq, 1);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_encode_decode_batch() {
        let mut encoder = Encoder::new(4096);

        let messages: Vec<(&[u8], u64)> =
            vec![(b"Message 1", 1), (b"Message 2", 2), (b"Message 3", 3)];

        encoder.encode_batch(&messages).unwrap();

        let mut decoder = Decoder::new(encoder.as_bytes());
        let batch_iter = decoder.decode_batch().unwrap();

        let decoded: Vec<_> = batch_iter.collect();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].1, b"Message 1");
        assert_eq!(decoded[1].1, b"Message 2");
        assert_eq!(decoded[2].1, b"Message 3");
    }

    #[test]
    fn test_encoder_reuse() {
        let mut encoder = Encoder::new(4096);

        encoder.encode(MessageType::Publish, 1, b"First").unwrap();
        encoder.reset();
        encoder.encode(MessageType::Publish, 2, b"Second").unwrap();

        let mut decoder = Decoder::new(encoder.as_bytes());
        let (header, _) = decoder.next().unwrap();

        // Copy field to avoid unaligned reference
        let seq = header.sequence;
        assert_eq!(seq, 2); // Should be second message after reset
    }
}
