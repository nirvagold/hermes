//! Hermes - Ultra Low-Latency Message Broker
//!
//! Arsitektur:
//! - Zero-Copy: Mmap-backed storage
//! - Lock-Free: Atomic-only ring buffer
//! - No-Allocation: Pre-allocated buffers
//! - Binary Protocol: SBE-inspired flat encoding

mod core;
mod network;
mod protocol;

use crate::core::{MmapStorage, RingBuffer};
use crate::protocol::{Decoder, Encoder, MessageType};
use std::time::Instant;

fn main() {
    println!("🚀 Hermes Message Broker - PoC v0.2");
    println!("====================================\n");

    // Benchmark Ring Buffer
    benchmark_ring_buffer();

    // Benchmark Mmap Storage
    benchmark_mmap_storage();

    // Benchmark Protocol Encoding
    benchmark_protocol();

    println!("\n✅ All benchmarks complete!");
    println!("\nTo start server: cargo run --release -- server 0.0.0.0:9999");
}

fn benchmark_ring_buffer() {
    println!("📊 Ring Buffer Benchmark (Lock-Free SPSC)");
    println!("-----------------------------------------");

    const ITERATIONS: usize = 1_000_000;
    let rb: RingBuffer<u64, 65536> = RingBuffer::new();

    // Warm up
    for i in 0..1000 {
        rb.push(i);
    }
    for _ in 0..1000 {
        rb.pop();
    }

    // Benchmark push
    let start = Instant::now();
    for i in 0..ITERATIONS {
        while !rb.push(i as u64) {
            rb.pop();
        }
    }
    let push_duration = start.elapsed();

    // Drain
    while rb.pop().is_some() {}

    // Benchmark pop
    for i in 0..ITERATIONS {
        rb.push(i as u64);
    }

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        rb.pop();
    }
    let pop_duration = start.elapsed();

    let push_ns = push_duration.as_nanos() as f64 / ITERATIONS as f64;
    let pop_ns = pop_duration.as_nanos() as f64 / ITERATIONS as f64;

    println!("  Operations: {}", ITERATIONS);
    println!(
        "  Push latency: {:.2} ns/op ({:.3} μs/op)",
        push_ns,
        push_ns / 1000.0
    );
    println!(
        "  Pop latency:  {:.2} ns/op ({:.3} μs/op)",
        pop_ns,
        pop_ns / 1000.0
    );
    println!(
        "  Throughput:   {:.2} M ops/sec\n",
        ITERATIONS as f64 / push_duration.as_secs_f64() / 1_000_000.0
    );
}

fn benchmark_mmap_storage() {
    println!("📊 Mmap Storage Benchmark (Zero-Copy)");
    println!("-------------------------------------");

    const ITERATIONS: usize = 100_000;
    const MSG_SIZE: usize = 64;

    let path = "hermes_bench.dat";
    let mut storage = MmapStorage::open(path, 64 * 1024 * 1024).unwrap();

    let msg = [0u8; MSG_SIZE];

    // Benchmark write
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        storage.write(&msg);
    }
    let write_duration = start.elapsed();

    // Benchmark read
    let start = Instant::now();
    for i in 0..ITERATIONS {
        let offset = (i * MSG_SIZE) % (64 * 1024 * 1024 - MSG_SIZE);
        storage.read(offset, MSG_SIZE);
    }
    let read_duration = start.elapsed();

    let write_ns = write_duration.as_nanos() as f64 / ITERATIONS as f64;
    let read_ns = read_duration.as_nanos() as f64 / ITERATIONS as f64;

    println!("  Message size: {} bytes", MSG_SIZE);
    println!("  Operations: {}", ITERATIONS);
    println!(
        "  Write latency: {:.2} ns/op ({:.3} μs/op)",
        write_ns,
        write_ns / 1000.0
    );
    println!(
        "  Read latency:  {:.2} ns/op ({:.3} μs/op)",
        read_ns,
        read_ns / 1000.0
    );
    println!(
        "  Write throughput: {:.2} MB/sec\n",
        (ITERATIONS * MSG_SIZE) as f64 / write_duration.as_secs_f64() / 1_000_000.0
    );

    std::fs::remove_file(path).ok();
}

fn benchmark_protocol() {
    println!("📊 Protocol Benchmark (Binary Encoding)");
    println!("---------------------------------------");

    const ITERATIONS: usize = 1_000_000;
    const PAYLOAD_SIZE: usize = 64;

    let mut encoder = Encoder::new(1024 * 1024); // 1MB buffer
    let payload = vec![0u8; PAYLOAD_SIZE];

    // Benchmark encode
    let start = Instant::now();
    for i in 0..ITERATIONS {
        if encoder.available() < 128 {
            encoder.reset();
        }
        encoder.encode(MessageType::Publish, i as u64, &payload);
    }
    let encode_duration = start.elapsed();

    // Prepare buffer untuk decode benchmark
    encoder.reset();
    for i in 0..10000 {
        encoder.encode(MessageType::Publish, i as u64, &payload);
    }
    let encoded_data = encoder.as_bytes().to_vec();

    // Benchmark decode
    let start = Instant::now();
    for _ in 0..100 {
        let mut decoder = Decoder::new(&encoded_data);
        while decoder.next().is_some() {}
    }
    let decode_duration = start.elapsed();

    let encode_ns = encode_duration.as_nanos() as f64 / ITERATIONS as f64;
    let decode_ns = decode_duration.as_nanos() as f64 / (100 * 10000) as f64;

    println!("  Payload size: {} bytes", PAYLOAD_SIZE);
    println!("  Encode ops: {}", ITERATIONS);
    println!(
        "  Encode latency: {:.2} ns/op ({:.3} μs/op)",
        encode_ns,
        encode_ns / 1000.0
    );
    println!(
        "  Decode latency: {:.2} ns/op ({:.3} μs/op)",
        decode_ns,
        decode_ns / 1000.0
    );
    println!(
        "  Encode throughput: {:.2} M msgs/sec",
        ITERATIONS as f64 / encode_duration.as_secs_f64() / 1_000_000.0
    );

    // Benchmark batch encoding
    println!("\n  Batch Encoding (10 messages/batch):");
    encoder.reset();
    let messages: Vec<(&[u8], u64)> = (0..10).map(|i| (payload.as_slice(), i as u64)).collect();

    let start = Instant::now();
    for _ in 0..100_000 {
        encoder.reset();
        encoder.encode_batch(&messages);
    }
    let batch_duration = start.elapsed();

    let batch_ns = batch_duration.as_nanos() as f64 / 100_000.0;
    let per_msg_ns = batch_ns / 10.0;

    println!(
        "  Batch latency: {:.2} ns/batch ({:.2} ns/msg)",
        batch_ns, per_msg_ns
    );
    println!(
        "  Batch throughput: {:.2} M msgs/sec",
        (100_000 * 10) as f64 / batch_duration.as_secs_f64() / 1_000_000.0
    );
}
