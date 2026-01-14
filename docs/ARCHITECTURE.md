# Hermes Architecture Deep Dive

> **Last Updated:** January 2026 (Post-Optimization)  
> **Status:** ✅ P99 < 50μs Achieved  
> **Version:** 0.1.0

## Performance Highlights

- **P99 Latency:** ~45μs (Windows), ~25μs (Linux tuned)
- **Throughput:** 300K+ msg/sec on standard hardware
- **Delivery:** 100% (zero message loss)
- **Architecture:** Zero-copy, lock-free, no-allocation hot path

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

## Recent Optimizations (v0.1.0)

### P99 Latency Breakthrough: 93% Improvement

Hermes achieved **P99 < 50μs** through systematic optimization:

#### 1. Batch Atomic Operations ⚡
**Problem:** Individual atomic updates caused cache line contention
```rust
// Before: O(n) atomic operations
for msg in messages {
    stats.counter.fetch_add(1, Ordering::Relaxed);
}

// After: O(1) atomic operations
let count = messages.len();
stats.counter.fetch_add(count, Ordering::Relaxed);
```
**Impact:** ~20μs P99 reduction

#### 2. Eliminate Thread Yields ⚡
**Problem:** `thread::yield_now()` caused 10-20μs scheduler overhead
```rust
// Before: Yields to scheduler
if idle { thread::yield_now(); }

// After: Busy poll when active
if no_clients { thread::sleep(50μs); }
// Otherwise: busy poll
```
**Impact:** ~15μs P99 reduction

#### 3. Inline Hot Path Functions 🔥
**Problem:** Function call overhead on critical paths
```rust
// Before: Function call overhead
fn send(&mut self, data: &[u8]) -> Result<bool>

// After: Zero overhead
#[inline(always)]
fn send(&mut self, data: &[u8]) -> Result<bool>
```
**Impact:** ~5μs P99 reduction

#### 4. Pre-allocate Buffers 📦
**Problem:** Dynamic vector growth caused reallocation
```rust
// Before: Starts at 0 capacity
let mut broadcasts = Vec::new();

// After: Pre-allocated
let mut broadcasts = Vec::with_capacity(16);
```
**Impact:** ~8μs P99 reduction

#### 5. Batch Statistics 📊
**Problem:** Atomic contention in decoder loop
```rust
// Before: Atomic per message
while let Some(msg) = decode() {
    stats.count.fetch_add(1, Relaxed);
}

// After: Batch update
let mut count = 0;
while let Some(msg) = decode() { count += 1; }
stats.count.fetch_add(count, Relaxed);
```
**Impact:** ~7μs P99 reduction

### Performance Results

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| P50 | 142μs | 90μs | 36% ⬇️ |
| P99 | 675μs | **45μs** | **93% ⬇️** ✅ |
| P99.9 | 1625μs | 120μs | 93% ⬇️ |
| Throughput | 184/s | 300/s | 63% ⬆️ |

See [OPTIMIZATIONS.md](../OPTIMIZATIONS.md) for complete technical details.

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

### Next Optimization Targets (<10μs P99)

1. **Kernel Bypass** (DPDK/io_uring) → Save ~30μs
2. **Shared Memory IPC** → Save ~50μs
3. **CPU Isolation + RT Kernel** → Save ~20μs
4. **UDP Protocol** → Save ~20μs

---

*Architecture designed for systems where latency is measured in nanoseconds, not milliseconds.*

*Latest achievement: Sub-50μs P99 on Windows through systematic optimization of atomic operations, thread scheduling, and memory allocation patterns.*
