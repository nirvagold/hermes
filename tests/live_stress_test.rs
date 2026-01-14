//! Live Stress Test - High-Frequency Token Injection
//!
//! Simulasi trafik mempool yang padat untuk stress testing Hermes.
//! Target: 100+ tokens per detik dengan data lengkap.
//!
//! Usage:
//!   cargo test --release --test live_stress_test -- --nocapture

use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Token data structure (matches battle_test.rs)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TokenData {
    contract_address: [u8; 32],
    chain_id: u32,
    risk_score: u8,
    honeypot_status: u8,
    buy_tax: u8,
    sell_tax: u8,
    analysis_timestamp_ns: u64,
    liquidity_usd: u64,
    holder_count: u32,
    _reserved: [u8; 4],
}

const TOKEN_DATA_SIZE: usize = std::mem::size_of::<TokenData>();

impl TokenData {
    fn random(seed: u64) -> Self {
        let mut hash = seed;
        for _ in 0..10 {
            hash = hash.wrapping_mul(6364136223846793005).wrapping_add(1);
        }

        let mut ca = [0u8; 32];
        let ca_str = format!("0x{:064x}", hash);
        ca.copy_from_slice(&ca_str.as_bytes()[..32]);

        Self {
            contract_address: ca,
            chain_id: [1, 8453, 501][(hash % 3) as usize], // ETH, Base, Solana
            risk_score: (hash % 100) as u8,
            honeypot_status: if hash % 10 < 2 { 1 } else { 0 },
            buy_tax: ((hash >> 8) % 30) as u8,
            sell_tax: ((hash >> 16) % 50) as u8,
            analysis_timestamp_ns: now_ns(),
            liquidity_usd: (hash % 1_000_000) * 100,
            holder_count: ((hash >> 24) % 10000) as u32,
            _reserved: [0; 4],
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, TOKEN_DATA_SIZE) }
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Hermes protocol constants
const MAGIC: u32 = 0x48524D53;
const VERSION: u8 = 1;
const MSG_PUBLISH: u8 = 1;
const HEADER_SIZE: usize = 32;

/// Encode message manually (untuk test independence)
fn encode_message(buffer: &mut [u8], sequence: u64, payload: &[u8]) -> usize {
    let payload_len = payload.len() as u32;
    let timestamp = now_ns();

    // Simple checksum
    let mut checksum: u32 = 1;
    for &b in payload {
        checksum = checksum.wrapping_add(b as u32);
    }

    // Pack header (little-endian)
    buffer[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buffer[4] = VERSION;
    buffer[5] = MSG_PUBLISH;
    buffer[6..8].copy_from_slice(&0u16.to_le_bytes()); // flags
    buffer[8..16].copy_from_slice(&sequence.to_le_bytes());
    buffer[16..24].copy_from_slice(&timestamp.to_le_bytes());
    buffer[24..28].copy_from_slice(&payload_len.to_le_bytes());
    buffer[28..32].copy_from_slice(&checksum.to_le_bytes());

    // Copy payload
    buffer[HEADER_SIZE..HEADER_SIZE + payload.len()].copy_from_slice(payload);

    HEADER_SIZE + payload.len()
}

/// Statistics collector
struct StressStats {
    sent: AtomicU64,
    errors: AtomicU64,
    total_latency_ns: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
}

impl StressStats {
    fn new() -> Self {
        Self {
            sent: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
        }
    }

    fn record_send(&self, latency_ns: u64) {
        self.sent.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        // Update min (CAS loop)
        let mut current = self.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current {
            match self.min_latency_ns.compare_exchange_weak(
                current,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        // Update max (CAS loop)
        let mut current = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current {
            match self.max_latency_ns.compare_exchange_weak(
                current,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }
    }

    fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn print_report(&self, duration: Duration) {
        let sent = self.sent.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        let total_latency = self.total_latency_ns.load(Ordering::Relaxed);
        let min_latency = self.min_latency_ns.load(Ordering::Relaxed);
        let max_latency = self.max_latency_ns.load(Ordering::Relaxed);

        let avg_latency = if sent > 0 { total_latency / sent } else { 0 };
        let rate = sent as f64 / duration.as_secs_f64();

        println!("\n📊 STRESS TEST RESULTS");
        println!("======================");
        println!("  Duration:      {:.2}s", duration.as_secs_f64());
        println!("  Sent:          {}", sent);
        println!("  Errors:        {}", errors);
        println!("  Rate:          {:.1} tokens/sec", rate);
        println!("\nLatency (Send):");
        println!("  Min:           {:.2} μs", min_latency as f64 / 1000.0);
        println!("  Max:           {:.2} μs", max_latency as f64 / 1000.0);
        println!("  Avg:           {:.2} μs", avg_latency as f64 / 1000.0);

        if errors == 0 && rate >= 100.0 {
            println!(
                "\n✅ STRESS TEST PASSED - No drops at {} tokens/sec",
                rate as u32
            );
        } else if errors > 0 {
            println!("\n⚠️  PACKET DROPS DETECTED - {} errors", errors);
        }
    }
}

/// Single-threaded stress injector
fn stress_injector(
    host: &str,
    tokens_per_sec: u32,
    duration_secs: u32,
    stats: Arc<StressStats>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut stream = match TcpStream::connect(host) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect: {}", e);
            return;
        }
    };

    stream.set_nodelay(true).ok();
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .ok();

    let mut buffer = vec![0u8; HEADER_SIZE + TOKEN_DATA_SIZE];
    let interval = Duration::from_nanos(1_000_000_000 / tokens_per_sec as u64);
    let mut sequence = 0u64;
    let mut next_send = Instant::now();
    let end_time = Instant::now() + Duration::from_secs(duration_secs as u64);

    while Instant::now() < end_time && !stop_flag.load(Ordering::Relaxed) {
        // Rate limiting
        let now = Instant::now();
        if now < next_send {
            thread::sleep(next_send - now);
        }
        next_send = Instant::now() + interval;

        // Generate token data
        let token = TokenData::random(sequence);

        // Encode and send
        let send_start = now_ns();
        let msg_len = encode_message(&mut buffer, sequence, token.as_bytes());

        match stream.write_all(&buffer[..msg_len]) {
            Ok(_) => {
                let latency = now_ns() - send_start;
                stats.record_send(latency);
            }
            Err(_) => {
                stats.record_error();
            }
        }

        sequence += 1;
    }
}

/// Multi-threaded stress test
fn multi_threaded_stress(
    host: &str,
    threads: u32,
    tokens_per_thread: u32,
    duration_secs: u32,
) -> Arc<StressStats> {
    let stats = Arc::new(StressStats::new());
    let stop_flag = Arc::new(AtomicBool::new(false));

    println!(
        "🔥 Starting {} injector threads ({} tokens/sec each)...",
        threads, tokens_per_thread
    );

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let host = host.to_string();
            let stats = Arc::clone(&stats);
            let stop = Arc::clone(&stop_flag);
            thread::spawn(move || {
                stress_injector(&host, tokens_per_thread, duration_secs, stats, stop);
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().ok();
    }

    stats
}

#[test]
fn test_stress_100_tokens_per_sec() {
    println!("\n🧪 LIVE STRESS TEST - 100 tokens/sec");
    println!("=====================================\n");

    let host = std::env::var("HERMES_HOST").unwrap_or_else(|_| "127.0.0.1:9999".to_string());

    // Check if server is running
    match TcpStream::connect(&host) {
        Ok(_) => println!("✅ Hermes server is running at {}\n", host),
        Err(e) => {
            println!("⚠️  Cannot connect to Hermes server at {}: {}", host, e);
            println!("   Start server with: cargo run --release --bin hermes_server");
            println!("   Skipping test.\n");
            return;
        }
    }

    let start = Instant::now();
    let stats = multi_threaded_stress(&host, 1, 100, 10); // 100 tokens/sec for 10 seconds
    let duration = start.elapsed();

    stats.print_report(duration);

    // Assert no errors
    let errors = stats.errors.load(Ordering::Relaxed);
    assert_eq!(errors, 0, "Packet drops detected!");
}

#[test]
fn test_stress_500_tokens_per_sec() {
    println!("\n🧪 LIVE STRESS TEST - 500 tokens/sec (5 threads x 100)");
    println!("======================================================\n");

    let host = std::env::var("HERMES_HOST").unwrap_or_else(|_| "127.0.0.1:9999".to_string());

    match TcpStream::connect(&host) {
        Ok(_) => println!("✅ Hermes server is running at {}\n", host),
        Err(e) => {
            println!("⚠️  Cannot connect to Hermes server at {}: {}", host, e);
            println!("   Skipping test.\n");
            return;
        }
    }

    let start = Instant::now();
    let stats = multi_threaded_stress(&host, 5, 100, 10); // 5 threads x 100 = 500 tokens/sec
    let duration = start.elapsed();

    stats.print_report(duration);
}

#[test]
fn test_burst_injection() {
    println!("\n🧪 BURST INJECTION TEST - 1000 tokens as fast as possible");
    println!("=========================================================\n");

    let host = std::env::var("HERMES_HOST").unwrap_or_else(|_| "127.0.0.1:9999".to_string());

    let mut stream = match TcpStream::connect(&host) {
        Ok(s) => {
            println!("✅ Connected to {}\n", host);
            s
        }
        Err(e) => {
            println!("⚠️  Cannot connect: {}. Skipping.\n", e);
            return;
        }
    };

    stream.set_nodelay(true).ok();

    let mut buffer = vec![0u8; HEADER_SIZE + TOKEN_DATA_SIZE];
    let mut latencies = Vec::with_capacity(1000);

    let start = Instant::now();

    for i in 0..1000u64 {
        let token = TokenData::random(i);
        let send_start = now_ns();
        let msg_len = encode_message(&mut buffer, i, token.as_bytes());
        stream.write_all(&buffer[..msg_len]).ok();
        latencies.push(now_ns() - send_start);
    }

    let duration = start.elapsed();

    // Calculate stats
    latencies.sort_unstable();
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let avg: u64 = latencies.iter().sum::<u64>() / latencies.len() as u64;
    let p50 = latencies[500];
    let p99 = latencies[990];

    println!("📊 BURST TEST RESULTS");
    println!("=====================");
    println!("  Tokens:    1000");
    println!("  Duration:  {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!(
        "  Rate:      {:.0} tokens/sec",
        1000.0 / duration.as_secs_f64()
    );
    println!("\nLatency:");
    println!("  Min:       {:.2} μs", min as f64 / 1000.0);
    println!("  Max:       {:.2} μs", max as f64 / 1000.0);
    println!("  Avg:       {:.2} μs", avg as f64 / 1000.0);
    println!("  P50:       {:.2} μs", p50 as f64 / 1000.0);
    println!("  P99:       {:.2} μs", p99 as f64 / 1000.0);

    if p99 < 50_000 {
        println!("\n✅ P99 < 50μs - EXCELLENT!");
    }
}
