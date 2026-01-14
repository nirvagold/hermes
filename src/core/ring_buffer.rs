//! Lock-Free Single-Producer Single-Consumer (SPSC) Ring Buffer
//!
//! Implementasi menggunakan Lamport Queue dengan memory ordering yang tepat.
//! Tidak ada Mutex, tidak ada alokasi setelah inisialisasi.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Slot dalam ring buffer - menyimpan data dengan ukuran tetap
#[repr(C, align(64))] // Cache line alignment untuk menghindari false sharing
struct Slot<T> {
    data: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Slot<T> {
    const fn new() -> Self {
        Self {
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

/// Lock-Free SPSC Ring Buffer
///
/// Menggunakan separate cache lines untuk head dan tail
/// untuk menghindari false sharing antara producer dan consumer.
#[repr(C)]
pub struct RingBuffer<T, const N: usize> {
    // Producer side - cache line aligned
    head: CacheLinePadded<AtomicUsize>,
    // Consumer side - cache line aligned
    tail: CacheLinePadded<AtomicUsize>,
    // Pre-allocated buffer di heap - tidak ada alokasi setelah init
    buffer: Box<[Slot<T>]>,
    // Mask untuk operasi modulo yang cepat (N harus power of 2)
    mask: usize,
}

/// Padding untuk cache line isolation (64 bytes pada x86-64)
#[repr(C, align(64))]
struct CacheLinePadded<T> {
    value: T,
}

impl<T> CacheLinePadded<T> {
    const fn new(value: T) -> Self {
        Self { value }
    }
}

// SAFETY: RingBuffer aman untuk Send/Sync karena:
// - Hanya satu producer (menulis head)
// - Hanya satu consumer (menulis tail)
// - Atomic operations menjamin visibility
unsafe impl<T: Send, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

impl<T: Copy, const N: usize> Default for RingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    /// Membuat ring buffer baru. N HARUS power of 2.
    ///
    /// Alokasi hanya terjadi sekali saat inisialisasi.
    /// Setelah itu, tidak ada alokasi di hot path.
    ///
    /// # Panics
    /// Panic jika N bukan power of 2 atau N == 0
    pub fn new() -> Self {
        assert!(N > 0 && N.is_power_of_two(), "N must be power of 2");

        // Alokasi buffer di heap untuk menghindari stack overflow
        let mut buffer = Vec::with_capacity(N);
        for _ in 0..N {
            buffer.push(Slot::new());
        }

        Self {
            head: CacheLinePadded::new(AtomicUsize::new(0)),
            tail: CacheLinePadded::new(AtomicUsize::new(0)),
            buffer: buffer.into_boxed_slice(),
            mask: N - 1,
        }
    }

    /// Push data ke buffer (Producer side)
    ///
    /// Returns `true` jika berhasil, `false` jika buffer penuh.
    /// Zero-allocation, lock-free.
    #[inline(always)]
    pub fn push(&self, value: T) -> bool {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);

        // Cek apakah buffer penuh
        if head.wrapping_sub(tail) >= N {
            return false;
        }

        let slot = &self.buffer[head & self.mask];

        // SAFETY: Kita sudah memastikan slot ini tidak sedang dibaca
        unsafe {
            (*slot.data.get()).write(value);
        }

        // Release fence: pastikan write di atas visible sebelum head di-update
        self.head
            .value
            .store(head.wrapping_add(1), Ordering::Release);

        true
    }

    /// Pop data dari buffer (Consumer side)
    ///
    /// Returns `Some(T)` jika ada data, `None` jika buffer kosong.
    /// Zero-allocation, lock-free.
    #[inline(always)]
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.value.load(Ordering::Relaxed);
        let head = self.head.value.load(Ordering::Acquire);

        // Cek apakah buffer kosong
        if tail == head {
            return None;
        }

        let slot = &self.buffer[tail & self.mask];

        // SAFETY: Kita sudah memastikan slot ini sudah ditulis dan tidak sedang ditulis
        let value = unsafe { (*slot.data.get()).assume_init_read() };

        // Release fence: pastikan read di atas selesai sebelum tail di-update
        self.tail
            .value
            .store(tail.wrapping_add(1), Ordering::Release);

        Some(value)
    }

    /// Cek apakah buffer kosong
    #[inline(always)]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.value.load(Ordering::Acquire);
        let head = self.head.value.load(Ordering::Acquire);
        tail == head
    }

    /// Cek apakah buffer penuh
    #[inline(always)]
    #[allow(dead_code)]
    pub fn is_full(&self) -> bool {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        head.wrapping_sub(tail) >= N
    }

    /// Jumlah elemen dalam buffer
    #[inline(always)]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Kapasitas buffer
    #[inline(always)]
    #[allow(dead_code)]
    pub const fn capacity(&self) -> usize {
        N
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_push_pop() {
        let rb: RingBuffer<u64, 16> = RingBuffer::new();

        assert!(rb.is_empty());
        assert!(!rb.is_full());

        assert!(rb.push(42));
        assert!(!rb.is_empty());

        assert_eq!(rb.pop(), Some(42));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_full_buffer() {
        let rb: RingBuffer<u64, 4> = RingBuffer::new();

        assert!(rb.push(1));
        assert!(rb.push(2));
        assert!(rb.push(3));
        assert!(rb.push(4));

        assert!(rb.is_full());
        assert!(!rb.push(5)); // Should fail - buffer full

        assert_eq!(rb.pop(), Some(1));
        assert!(rb.push(5)); // Now should succeed
    }

    #[test]
    fn test_wraparound() {
        let rb: RingBuffer<u64, 4> = RingBuffer::new();

        // Fill and drain multiple times to test wraparound
        for round in 0..10 {
            for i in 0..4 {
                assert!(rb.push(round * 4 + i));
            }
            for i in 0..4 {
                assert_eq!(rb.pop(), Some(round * 4 + i));
            }
        }
    }
}
