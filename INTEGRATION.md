# Hermes Integration Guide

## Quick Start

### Mengirim Pesan ke Hermes

```rust
use hermes::protocol::{Encoder, MessageType};

// Pre-allocate encoder (sekali saat startup)
let mut encoder = Encoder::new(64 * 1024); // 64KB buffer

// Encode pesan (zero-allocation)
let payload = b"{'symbol':'BTC','price':50000}";
let encoded = encoder.encode(MessageType::Publish, sequence, payload).unwrap();

// Kirim via TCP
socket.write_all(encoded)?;
encoder.reset(); // Reuse buffer
```

### Menerima Pesan dari Hermes

```rust
use hermes::protocol::{Decoder, HEADER_SIZE};

// Buffer untuk receive
let mut buf = [0u8; 65536];
let n = socket.read(&mut buf)?;

// Decode (zero-copy)
let mut decoder = Decoder::new(&buf[..n]);
while let Some((header, payload)) = decoder.next() {
    // payload adalah slice langsung ke buf - tidak ada copy!
    process_message(header.sequence, payload);
}
```

### Batch Messages (High Throughput)

```rust
// Kumpulkan messages untuk batch
let messages: Vec<(&[u8], u64)> = vec![
    (b"msg1", 1),
    (b"msg2", 2),
    (b"msg3", 3),
];

// Encode batch (lebih efisien untuk NIC)
let batch = encoder.encode_batch(&messages).unwrap();
socket.write_all(batch)?;
```

## Performance Tips

1. **Pre-allocate semua buffer saat startup**
2. **Gunakan `set_nodelay(true)`** untuk disable Nagle's algorithm
3. **Batch messages** jika throughput > latency
4. **Reuse encoder** dengan `reset()` - jangan buat baru

## Benchmark Results

| Component | Latency | Throughput |
|-----------|---------|------------|
| Ring Buffer | ~12 ns | 84M ops/sec |
| Mmap Write | ~48 ns | 1.3 GB/sec |
| Protocol Encode | ~75 ns | 13M msgs/sec |
| Batch Encode | ~64 ns/msg | 15M msgs/sec |
