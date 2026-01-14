//! Memory-Mapped File Storage untuk Zero-Copy I/O
//!
//! Data di-mmap langsung ke virtual memory, memungkinkan:
//! - Zero-copy read: Data langsung dari page cache ke aplikasi
//! - Kernel-managed paging: OS menangani swap in/out
//! - Persistence: Data otomatis tersimpan ke disk

use memmap2::{MmapMut, MmapOptions};
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Header untuk mmap storage - menyimpan metadata
#[repr(C, align(64))]
struct StorageHeader {
    magic: u64,             // Magic number untuk validasi
    version: u32,           // Versi format
    capacity: u32,          // Kapasitas dalam bytes
    write_pos: AtomicUsize, // Posisi tulis saat ini
    read_pos: AtomicUsize,  // Posisi baca saat ini
}

const MAGIC: u64 = 0x4845524D45535F56; // "HERMES_V" in hex
const VERSION: u32 = 1;
const HEADER_SIZE: usize = std::mem::size_of::<StorageHeader>();

/// Mmap-backed storage untuk message persistence
pub struct MmapStorage {
    mmap: MmapMut,
    capacity: usize,
}

impl MmapStorage {
    /// Membuat atau membuka mmap storage
    ///
    /// # Arguments
    /// * `path` - Path ke file storage
    /// * `capacity` - Kapasitas dalam bytes (harus power of 2)
    pub fn open<P: AsRef<Path>>(path: P, capacity: usize) -> io::Result<Self> {
        assert!(capacity.is_power_of_two(), "Capacity must be power of 2");

        let total_size = HEADER_SIZE + capacity;

        // Fix clippy warning: explicit truncate(false) for clarity
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        // Set file size
        file.set_len(total_size as u64)?;

        // SAFETY: File sudah dibuka dengan read/write permission
        let mut mmap = unsafe { MmapOptions::new().len(total_size).map_mut(&file)? };

        // Initialize header jika file baru
        let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut StorageHeader) };

        if header.magic != MAGIC {
            header.magic = MAGIC;
            header.version = VERSION;
            header.capacity = capacity as u32;
            header.write_pos = AtomicUsize::new(0);
            header.read_pos = AtomicUsize::new(0);
        }

        Ok(Self { mmap, capacity })
    }

    /// Menulis data ke storage (zero-copy write)
    ///
    /// Returns offset dimana data ditulis, atau None jika tidak cukup ruang
    #[inline(always)]
    pub fn write(&mut self, data: &[u8]) -> Option<usize> {
        let capacity = self.capacity;
        let mmap_ptr = self.mmap.as_mut_ptr();

        // SAFETY: Header berada di awal mmap region
        let header = unsafe { &mut *(mmap_ptr as *mut StorageHeader) };
        let write_pos = header.write_pos.load(Ordering::Relaxed);
        let read_pos = header.read_pos.load(Ordering::Acquire);

        let available = capacity - (write_pos.wrapping_sub(read_pos));

        if data.len() > available {
            return None;
        }

        let offset = write_pos & (capacity - 1);

        // Zero-copy write langsung ke mmap region
        unsafe {
            let dst = mmap_ptr.add(HEADER_SIZE + offset);

            // Handle wraparound
            let first_part = (capacity - offset).min(data.len());
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, first_part);

            if first_part < data.len() {
                let second_part = data.len() - first_part;
                let wrap_dst = mmap_ptr.add(HEADER_SIZE);
                std::ptr::copy_nonoverlapping(data.as_ptr().add(first_part), wrap_dst, second_part);
            }
        }

        header
            .write_pos
            .store(write_pos.wrapping_add(data.len()), Ordering::Release);

        Some(offset)
    }

    /// Membaca data dari storage (zero-copy read via slice)
    ///
    /// Returns slice ke data di mmap region - TRUE zero-copy!
    #[inline(always)]
    pub fn read(&self, offset: usize, len: usize) -> Option<&[u8]> {
        if offset + len > self.capacity {
            return None; // Tidak support wraparound read untuk simplicity
        }

        unsafe {
            let ptr = self.mmap.as_ptr().add(HEADER_SIZE + offset);
            Some(std::slice::from_raw_parts(ptr, len))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_mmap_storage_basic() {
        let path = "test_storage.dat";

        {
            let mut storage = MmapStorage::open(path, 4096).unwrap();

            let data = b"Hello, Hermes!";
            let offset = storage.write(data).unwrap();

            let read_data = storage.read(offset, data.len()).unwrap();
            assert_eq!(read_data, data);
        }

        fs::remove_file(path).ok();
    }

    #[test]
    fn test_mmap_persistence() {
        let path = "test_persistence.dat";

        // Write data
        {
            let mut storage = MmapStorage::open(path, 4096).unwrap();
            storage.write(b"Persistent data").unwrap();
        }

        // Reopen and verify
        {
            let storage = MmapStorage::open(path, 4096).unwrap();
            let data = storage.read(0, 15).unwrap();
            assert_eq!(data, b"Persistent data");
        }

        fs::remove_file(path).ok();
    }
}
