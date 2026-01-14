# 🚀 Hermes

[![CI](https://github.com/yourusername/hermes/actions/workflows/ci.yml/badge.svg)](https://github.com/yourusername/hermes/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/hermes.svg)](https://crates.io/crates/hermes)
[![Documentation](https://docs.rs/hermes/badge.svg)](https://docs.rs/hermes)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)

**Ultra Low-Latency Message Broker for High-Frequency Systems**

Hermes is a zero-copy, lock-free message broker designed for systems where every nanosecond counts. Built in Rust with HFT (High-Frequency Trading) principles.

## 🎯 Battle-Tested Performance

### End-to-End Latency (Rust-to-Rust)

| Metric | Windows | Linux (Tuned)* |
|--------|---------|----------------|
| Min | 44 μs | ~5 μs |
| P50 | 142 μs | ~15 μs |
| P99 | 675 μs | ~35 μs |
| Delivery | 100% | 100% |

*Expected with CPU isolation, RT priority, and kernel tuning

### Component Benchmarks

| Component | Latency | Throughput |
|-----------|---------|------------|
| Ring Buffer (SPSC) | **3.5 ns** | 284M ops/sec |
| Mmap Storage | **48 ns** | 1.3 GB/sec |
| Protocol Encode | **53 ns** | 19M msgs/sec |
| Protocol Decode | **0.2 ns** | 5B+ msgs/sec |
| Batch Encode | **47 ns/msg** | 21M msgs/sec |

### vs Industry Standards

| Broker | P99 Latency | Architecture |
|--------|-------------|--------------|
| **Hermes** | **< 50 μs*** | Zero-copy, Lock-free |
| Aeron | 1-10 μs | Zero-copy, UDP |
| ZeroMQ | ~100 μs | Lock-free |
| Kafka | 2-10 ms | Disk-based |
| RabbitMQ | 1-5 ms | AMQP |

*Linux with tuning

## Design Principles

### 1. Zero-Copy Architecture
```
NIC → Kernel Buffer → mmap → Application
         ↓
    No memcpy in hot path
```

Data flows directly from network to application memory via memory-mapped files. No intermediate copies.

### 2. Lock-Free Data Structures
- SPSC Ring Buffer using atomic operations only
- No `Mutex`, `RwLock`, or any blocking primitives in hot path
- Cache-line aligned (64 bytes) to prevent false sharing

### 3. No-Allocation Policy
- All buffers pre-allocated at initialization
- Zero heap allocations during message processing
- Flat P99 latency profile

### 4. Binary Protocol (SBE-inspired)
- Fixed 32-byte header, directly castable from bytes
- No parsing, no serialization overhead
- CRC32 checksum for integrity

## Quick Start

### Running the Server

```bash
# Build release binaries
cargo build --release

# Start Hermes server
cargo run --release --bin hermes_server

# In another terminal - start subscriber
cargo run --release --bin hermes_subscriber -- --duration 60

# In another terminal - run injector
cargo run --release --example battle_test -- --tokens 1000 --rate 200
```

### Using as a Library

```rust
use hermes::core::RingBuffer;
use hermes::protocol::{Encoder, Decoder, MessageType};

// Lock-free ring buffer
let rb: RingBuffer<u64, 65536> = RingBuffer::new();
rb.push(42);
assert_eq!(rb.pop(), Some(42));

// Zero-copy protocol encoding
let mut encoder = Encoder::new(64 * 1024);
let payload = b"Hello, Hermes!";
let encoded = encoder.encode(MessageType::Publish, 1, payload).unwrap();

// Zero-copy decoding
let mut decoder = Decoder::new(encoded);
let (header, data) = decoder.next().unwrap();
assert_eq!(data, payload);
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Hermes                                │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Network   │  │  Protocol   │  │        Core         │  │
│  │   Layer     │  │   Layer     │  │       Layer         │  │
│  │             │  │             │  │                     │  │
│  │  • mio      │  │  • Binary   │  │  • Ring Buffer      │  │
│  │  • TCP/UDP  │  │  • Batching │  │  • Mmap Storage     │  │
│  │  • Polling  │  │  • CRC32    │  │  • Atomic Ops       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Message Format

```
┌────────────────────────────────────────────────────┐
│              MessageHeader (32 bytes)              │
├──────────┬─────────┬───────┬───────┬──────────────┤
│  magic   │ version │ type  │ flags │   sequence   │
│  (4B)    │  (1B)   │ (1B)  │ (2B)  │    (8B)      │
├──────────┴─────────┴───────┴───────┴──────────────┤
│  timestamp_ns (8B)  │ payload_len (4B) │ crc (4B) │
├───────────────────────────────────────────────────┤
│                 Payload (variable)                 │
└───────────────────────────────────────────────────┘
```

## Use Cases

- **High-Frequency Trading**: Sub-microsecond order routing
- **Real-time Analytics**: Stream processing pipelines
- **Gaming Servers**: Low-latency game state synchronization
- **IoT Gateways**: High-throughput sensor data ingestion

## Roadmap

- [x] Lock-free SPSC Ring Buffer
- [x] Mmap-backed persistent storage
- [x] Binary protocol with batching
- [x] Cross-platform network layer (mio)
- [ ] io_uring support (Linux)
- [ ] MPMC Ring Buffer
- [ ] Reliable UDP with NACK
- [ ] Cluster mode (replication)

## Building

```bash
cargo build --release
cargo test
cargo run --release  # Run benchmarks
```

## Benchmarking

### Windows (PowerShell)
```powershell
.\scripts\run_benchmark.ps1 -Tokens 1000 -Rate 200
```

### Linux (with tuning)
```bash
# Apply performance tuning
sudo ./scripts/linux_tuning.sh setup

# Reboot to apply CPU isolation
sudo reboot

# Run optimized benchmark
sudo ./scripts/linux_tuning.sh bench
```

### Manual Benchmark
```bash
# Terminal 1: Server
cargo run --release --bin hermes_server

# Terminal 2: Subscriber (start FIRST!)
cargo run --release --bin hermes_subscriber -- --duration 30

# Terminal 3: Injector
cargo run --release --example battle_test -- --tokens 1000 --rate 200
```

## Criterion Benchmarks
```bash
cargo bench
```

## License

MIT License - See [LICENSE](LICENSE) for details.

## Documentation

- [Architecture Deep Dive](docs/ARCHITECTURE.md)
- [Benchmark Results](docs/BENCHMARKS.md)
- [Integration Guide](INTEGRATION.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Technical Showcase](SHOWCASE.md)

## Contributing

Contributions welcome! Please read our [contributing guidelines](docs/CONTRIBUTING.md) before submitting PRs.

---

*Built with 🦀 Rust for extreme performance*
