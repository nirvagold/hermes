# Contributing to Hermes

Thank you for your interest in contributing to Hermes! This document provides guidelines for contributing.

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
- Add `#[inline(always)]` for hot path functions

```bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
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

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# With output
cargo test -- --nocapture
```

## Benchmarking

Always benchmark performance-sensitive changes:

```bash
# Before changes
cargo bench -- --save-baseline before

# After changes
cargo bench -- --baseline before
```

## Questions?

Open an issue with the `question` label.

---

*Thank you for helping make Hermes faster!*
