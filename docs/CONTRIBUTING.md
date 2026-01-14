# Contributing to Hermes

> **Performance Target:** P99 < 50μs  
> **Architecture:** Zero-copy, Lock-free, No-allocation  
> **Status:** Production-Ready ✅

Thank you for your interest in contributing to Hermes! This document provides guidelines for contributing.

## Performance Philosophy

Hermes is designed for **ultra-low latency**. Every contribution must maintain or improve:
- ✅ P99 < 50μs latency
- ✅ Zero-allocation hot path
- ✅ Lock-free data structures
- ✅ 100% message delivery

## Code of Conduct

Be respectful, inclusive, and constructive. We're all here to build something great.

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch
4. Make your changes
5. Submit a pull request

## Development Setup

```bash
# Clone
git clone https://github.com/yourusername/hermes.git
cd hermes

# Build
cargo build --release

# Test
cargo test

# Benchmark
cargo bench
```

## Code Style

### Rust

- Follow standard Rust conventions (`rustfmt`)
- Use `clippy` for linting
- Document public APIs with doc comments
- Add `#[inline(always)]` for hot path functions (critical for <50μs latency)
- Use `Vec::with_capacity()` to avoid reallocation
- Batch atomic operations to reduce contention
- Avoid `thread::yield_now()` in hot paths

```bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
```

### Performance Guidelines

**Critical Rules for Hot Path:**

1. **No Allocations**
   ```rust
   // ❌ Bad: Allocates on every call
   fn process() -> Vec<u8> {
       vec![0; 1024]
   }
   
   // ✅ Good: Pre-allocated buffer
   fn process(&mut self, buf: &mut [u8]) {
       // Use existing buffer
   }
   ```

2. **Batch Atomic Operations**
   ```rust
   // ❌ Bad: O(n) atomic operations
   for item in items {
       counter.fetch_add(1, Ordering::Relaxed);
   }
   
   // ✅ Good: O(1) atomic operations
   let count = items.len();
   counter.fetch_add(count, Ordering::Relaxed);
   ```

3. **Inline Hot Functions**
   ```rust
   // ✅ Always inline hot path
   #[inline(always)]
   pub fn send(&mut self, data: &[u8]) -> Result<()> {
       // Critical path code
   }
   ```

4. **Avoid Thread Yields**
   ```rust
   // ❌ Bad: 10-20μs overhead
   if idle {
       thread::yield_now();
   }
   
   // ✅ Good: Busy poll or sleep only when no clients
   if no_clients {
       thread::sleep(Duration::from_micros(50));
   }
   ```

### Python

- Follow PEP 8
- Use type hints
- Document with docstrings

## Performance Guidelines

Hermes is a performance-critical system. Please follow these guidelines:

### DO ✅

- Pre-allocate buffers
- Use `#[inline(always)]` for hot path
- Prefer stack allocation over heap
- Use atomic operations for synchronization
- Benchmark before and after changes

### DON'T ❌

- Allocate in hot path
- Use `Mutex` or `RwLock` in hot path
- Add unnecessary abstractions
- Ignore benchmark regressions

## Pull Request Process

1. **Title**: Clear, descriptive title
2. **Description**: Explain what and why
3. **Tests**: Add tests for new functionality
4. **Benchmarks**: Include benchmark results if performance-related
5. **Documentation**: Update docs if needed

### PR Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Performance improvement
- [ ] Documentation

## Benchmarks (if applicable)
Before: X ns/op
After: Y ns/op
Change: Z%

## Checklist
- [ ] Tests pass
- [ ] Code formatted
- [ ] Documentation updated
```

## Areas for Contribution

### High Priority

- [ ] io_uring support (Linux)
- [ ] MPMC Ring Buffer
- [ ] Reliable UDP with NACK
- [ ] More language clients (Go, Java, C++)

### Medium Priority

- [ ] Metrics/observability
- [ ] Configuration file support
- [ ] TLS support
- [ ] Compression (optional)

### Documentation

- [ ] More examples
- [ ] Tutorial videos
- [ ] Performance tuning guide

## Testing

### Unit Tests
```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# With output
cargo test -- --nocapture
```

### Performance Testing

**Always verify P99 < 50μs after changes:**

```bash
# Terminal 1: Server
cargo run --release --bin hermes_server

# Terminal 2: Subscriber
cargo run --release --bin hermes_subscriber -- --duration 30

# Terminal 3: Benchmark
cargo run --release --example battle_test -- --tokens 1000 --rate 200
```

**Success Criteria:**
- ✅ P99 < 50μs
- ✅ 100% delivery rate
- ✅ No dropped messages
- ✅ No performance regression

## Benchmarking

Always benchmark performance-sensitive changes:

```bash
# Before changes
cargo bench -- --save-baseline before

# After changes
cargo bench -- --baseline before

# Check for regressions
# P99 should remain < 50μs
```

### Profiling

For performance analysis:

```bash
# Install flamegraph
cargo install flamegraph

# Profile server
cargo flamegraph --bin hermes_server

# Analyze hotspots
# Look for: allocations, atomic contention, syscalls
```

## Performance Checklist

Before submitting PR with performance changes:

- [ ] Verified P99 < 50μs maintained
- [ ] No new allocations in hot path
- [ ] Atomic operations batched where possible
- [ ] Hot functions marked `#[inline(always)]`
- [ ] Benchmarks show no regression
- [ ] Updated BENCHMARKS.md if improved

## Questions?

Open an issue with the `question` label.

---

*Thank you for helping make Hermes faster!*
