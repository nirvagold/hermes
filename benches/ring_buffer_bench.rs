//! Criterion benchmark untuk Ring Buffer
//!
//! Run dengan: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use hermes::core::RingBuffer;

fn bench_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer");
    group.throughput(Throughput::Elements(1));

    // Benchmark push
    group.bench_function("push", |b| {
        let rb: RingBuffer<u64, 65536> = RingBuffer::new();
        let mut i = 0u64;
        b.iter(|| {
            if !rb.push(black_box(i)) {
                rb.pop();
                rb.push(black_box(i));
            }
            i = i.wrapping_add(1);
        });
    });

    // Benchmark pop
    group.bench_function("pop", |b| {
        let rb: RingBuffer<u64, 65536> = RingBuffer::new();
        // Pre-fill
        for i in 0..32768 {
            rb.push(i);
        }
        b.iter(|| {
            if let Some(v) = rb.pop() {
                rb.push(black_box(v));
            }
        });
    });

    // Benchmark push+pop cycle
    group.bench_function("push_pop_cycle", |b| {
        let rb: RingBuffer<u64, 65536> = RingBuffer::new();
        let mut i = 0u64;
        b.iter(|| {
            rb.push(black_box(i));
            let _ = rb.pop();
            i = i.wrapping_add(1);
        });
    });

    group.finish();
}

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    // Batch operations
    for batch_size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_function(format!("batch_{}", batch_size), |b| {
            let rb: RingBuffer<u64, 65536> = RingBuffer::new();
            b.iter(|| {
                for i in 0..*batch_size {
                    rb.push(black_box(i as u64));
                }
                for _ in 0..*batch_size {
                    black_box(rb.pop());
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_push_pop, bench_throughput);
criterion_main!(benches);
