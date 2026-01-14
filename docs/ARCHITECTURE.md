# Hermes Architecture Deep Dive

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           HERMES ECOSYSTEM                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ┌──────────────┐     ┌──────────────┐     ┌──────────────────────┐   │
│   │   Producer   │     │    Hermes    │     │      Consumer        │   │
│   │  (Python)    │────▶│    Broker    │────▶│   (Rust/Python)      │   │
│   │              │     │    (Rust)    │     │                      │   │
│   │  Sniper Bot  │     │              │     │  Analytics Engine    │   │
│   │  Risk Engine │     │  Ring Buffer │     │  Trading Executor    │   │
│   └──────────────┘     │  Mmap Store  │     └──────────────────────┘   │
│                        └──────────────┘                                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Lock-Free Ring Buffer

The heart of Hermes. A Single-Producer Single-Consumer (SPSC) queue using only atomic operations.

```
┌─────────────────────────────────────────────────────────────┐
│                    Ring Buffer Layout                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Cache Line 0 (64B)     Cache Line 1 (64B)                 │
│   ┌─────────────────┐    ┌─────────────────┐                │
│   │  HEAD (AtomicU) │    │  TAIL (AtomicU) │                │
│   │  + padding      │    │  + padding      │                │
│   └─────────────────┘    └─────────────────┘                │
│           │                       │                          │
│           ▼                       ▼                          │
│   ┌───┬───┬───┬───┬───┬───┬───┬───┐                         │
│   │ 0 │ 1 │ 2 │ 3 │ 4 │ 5 │ 6 │ 7 │  ... N-1               │
│   └───┴───┴───┴───┴───┴───┴───┴───┘                         │
│       ▲                   ▲                                  │
│       │                   │                                  │
│     TAIL               HEAD                                  │
│   (Consumer)         (Producer)                              │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Key Design Decisions:**

1. **Cache Line Separation**: HEAD and TAIL are on separate 64-byte cache lines to prevent false sharing between producer and consumer cores.

2. **Power-of-2 Size**: Buffer size must be 2^N, enabling fast modulo via bitwise AND (`index & mask`).

3. **Memory Ordering**:
   - Producer: `Relaxed` load of HEAD, `Acquire` load of TAIL, `Release` store of HEAD
   - Consumer: `Relaxed` load of TAIL, `Acquire` load of HEAD, `Release` store of TAIL

4. **No Mutex**: Zero blocking primitives in the hot path.

### 2. Memory-Mapped Storage

Zero-copy persistence using OS page cache.

```
┌─────────────────────────────────────────────────────────────┐
│                    Mmap Storage Layout                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   File on Disk                    Virtual Memory             │
│   ┌─────────────┐                ┌─────────────┐            │
│   │   Header    │◄──── mmap ────▶│   Header    │            │
│   │   (64B)     │                │   (64B)     │            │
│   ├─────────────┤                ├─────────────┤            │
│   │             │                │             │            │
│   │    Data     │◄──── mmap ────▶│    Data     │            │
│   │   Region    │                │   Region    │            │
│   │             │                │             │            │
│   └─────────────┘                └─────────────┘            │
│                                         │                    │
│                                         ▼                    │
│                                  Application                 │
│                                  (Direct Access)             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Benefits:**
- **Zero-Copy Read**: Data accessed directly from page cache
- **Lazy Loading**: OS loads pages on-demand
- **Automatic Persistence**: OS handles flushing to disk
- **Shared Memory**: Multiple processes can map same file

### 3. Binary Protocol

SBE-inspired flat binary encoding for zero-parsing overhead.

```
┌─────────────────────────────────────────────────────────────┐
│                   Message Wire Format                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Byte:  0    4    5    6    8         16        24    28   │
│         ┌────┬────┬────┬────┬──────────┬─────────┬────┬────┐│
│         │MAGIC│VER│TYPE│FLAG│ SEQUENCE │TIMESTAMP│ LEN│ CRC││
│         │ 4B  │1B │ 1B │ 2B │    8B    │   8B    │ 4B │ 4B ││
│         └────┴────┴────┴────┴──────────┴─────────┴────┴────┘│
│         │◄─────────────── 32 bytes ──────────────────────▶│ │
│                                                              │
│         ┌────────────────────────────────────────────────┐  │
│         │                   PAYLOAD                       │  │
│         │              (variable, max 64KB)               │  │
│         └────────────────────────────────────────────────┘  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Why This Format:**
- **Fixed Header**: 32 bytes, directly castable from byte buffer
- **No Parsing**: `*(buffer as *const Header)` - single pointer cast
- **Alignment**: Header fields naturally aligned for fast access
- **Integrity**: CRC32 checksum for corruption detection

## Data Flow

### Producer Path (Hot Path)

```
1. Encode Message
   ┌─────────────────────────────────────────┐
   │ payload → Encoder.encode()              │
   │         → memcpy to pre-allocated buf   │
   │         → return slice (zero-alloc)     │
   └─────────────────────────────────────────┘
                      │
                      ▼
2. Push to Ring Buffer
   ┌─────────────────────────────────────────┐
   │ load HEAD (Relaxed)                     │
   │ load TAIL (Acquire)                     │
   │ check: HEAD - TAIL < N ?                │
   │ write data to buffer[HEAD & mask]       │
   │ store HEAD+1 (Release)                  │
   └─────────────────────────────────────────┘
                      │
                      ▼
3. Persist (Optional)
   ┌─────────────────────────────────────────┐
   │ memcpy to mmap region                   │
   │ (OS handles actual disk write)          │
   └─────────────────────────────────────────┘
```

### Consumer Path (Hot Path)

```
1. Pop from Ring Buffer
   ┌─────────────────────────────────────────┐
   │ load TAIL (Relaxed)                     │
   │ load HEAD (Acquire)                     │
   │ check: TAIL != HEAD ?                   │
   │ read data from buffer[TAIL & mask]      │
   │ store TAIL+1 (Release)                  │
   └─────────────────────────────────────────┘
                      │
                      ▼
2. Decode Message (Zero-Copy)
   ┌─────────────────────────────────────────┐
   │ header = *(buf as *const Header)        │
   │ payload = &buf[32..32+header.len]       │
   │ verify CRC (optional)                   │
   │ return (header, payload_slice)          │
   └─────────────────────────────────────────┘
```

## Performance Analysis

### Latency Breakdown

```
┌────────────────────────────────────────────────────────────┐
│                    Latency Budget (Rust)                    │
├────────────────────────────────────────────────────────────┤
│                                                             │
│   Operation              Time        % of Total             │
│   ─────────────────────────────────────────────            │
│   Ring Buffer Push       11.85 ns    12.6%                 │
│   Protocol Encode        75.00 ns    79.8%                 │
│   Mmap Write            48.00 ns     (async, not in path)  │
│   ─────────────────────────────────────────────            │
│   Total Hot Path         ~87 ns      100%                  │
│                                                             │
│   Protocol Decode        0.35 ns     (zero-copy cast)      │
│   Ring Buffer Pop        ~12 ns      (similar to push)     │
│                                                             │
└────────────────────────────────────────────────────────────┘
```

### Throughput Capacity

```
┌────────────────────────────────────────────────────────────┐
│                  Theoretical Throughput                     │
├────────────────────────────────────────────────────────────┤
│                                                             │
│   Component          Single Core      Notes                 │
│   ─────────────────────────────────────────────            │
│   Ring Buffer        84M ops/sec      Lock-free SPSC       │
│   Protocol Encode    13M msgs/sec     64B payload          │
│   Mmap Write         1.3 GB/sec       Sequential write     │
│   Network (10Gbps)   ~1.2 GB/sec      Wire speed limit     │
│                                                             │
│   Bottleneck: Network I/O (as expected)                    │
│                                                             │
└────────────────────────────────────────────────────────────┘
```

## Deployment Topology

### Recommended: Co-Location Setup

```
┌─────────────────────────────────────────────────────────────┐
│                    Same Server / Data Center                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   ┌─────────────┐    localhost    ┌─────────────┐           │
│   │  Sniper Bot │◄──────────────▶│   Hermes    │           │
│   │  (Python)   │    < 1 μs      │   Broker    │           │
│   └─────────────┘                └─────────────┘           │
│          │                              │                    │
│          │                              │                    │
│          ▼                              ▼                    │
│   ┌─────────────┐                ┌─────────────┐           │
│   │   Exchange  │                │  Analytics  │           │
│   │     API     │                │   Engine    │           │
│   └─────────────┘                └─────────────┘           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Why Co-Location Matters

| Scenario | Latency | Hermes Advantage |
|----------|---------|------------------|
| Same Process | < 100 ns | Full benefit |
| Same Server (localhost) | < 10 μs | High benefit |
| Same Data Center | 100-500 μs | Moderate benefit |
| Public Internet | 10-100 ms | Minimal benefit |

## Future Enhancements

### Planned Features

1. **io_uring Support** (Linux)
   - Kernel-bypass for network I/O
   - Expected: 2-5x throughput improvement

2. **MPMC Ring Buffer**
   - Multiple producers, multiple consumers
   - For fan-out scenarios

3. **Reliable UDP**
   - NACK-based retransmission
   - Maintains lock-free design

4. **Cluster Mode**
   - Replication across nodes
   - Leader election

---

*Architecture designed for systems where latency is measured in nanoseconds, not milliseconds.*
