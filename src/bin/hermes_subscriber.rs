//! Hermes Rust Subscriber - Zero-Allocation High-Performance Client
//!
//! Pure Rust subscriber untuk benchmark latency yang akurat.
//! Menggunakan:
//! - TCP_NODELAY untuk minimal latency
//! - Pre-allocated buffers (zero allocation di hot path)
//! - High-resolution timing via std::time::Instant (uses QueryPerformanceCounter on Windows)
//! - Lock-free statistics collection
//!
//! # Usage
//!
//! ```text
//! cargo run --release --bin hermes_subscriber -- --host 127.0.0.1:9999 --duration 60
//! ```
//!
//! # Options
//!
//! - `--host ADDR` - Server address (default: 127.0.0.1:9999)
//! - `--duration SEC` - Test duration in seconds (default: 60)

use std::io::{self, Read};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hermes::protocol::{Decoder, MessageType, HEADER_SIZE};

/// High-resolution timestamp in nanoseconds
#[inline(always)]
fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Pre-allocated latency histogram for zero-allocation stats
/// Buckets: 0-1μs, 1-2μs, 2-5μs, 5-10μs, 10-20μs, 20-50μs, 50-100μs, 100-500μs, 500μs-1ms, >1ms
struct LatencyHistogram {
    buckets: [AtomicU64; 12],
    min_ns: AtomicU64,
    max_ns: AtomicU64,
    sum_ns: AtomicU64,
    count: AtomicU64,
    // Store raw samples for percentile calculation (circular buffer)
    samples: Box<[AtomicU64; 100_000]>,
    sample_idx: AtomicU64,
}

impl LatencyHistogram {
    fn new() -> Self {
        // Initialize samples array
        let samples: Box<[AtomicU64; 100_000]> = {
            let mut v = Vec::with_capacity(100_000);
            for _ in 0..100_000 {
                v.push(AtomicU64::new(0));
            }
            v.into_boxed_slice().try_into().unwrap()
        };

        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            min_ns: AtomicU64::new(u64::MAX),
            max_ns: AtomicU64::new(0),
            sum_ns: AtomicU64::new(0),
            count: AtomicU64::new(0),
            samples,
            sample_idx: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    fn record(&self, latency_ns: u64) {
        // Bucket index based on latency
        let bucket = match latency_ns {
            0..=999 => 0,            // 0-1μs
            1000..=1999 => 1,        // 1-2μs
            2000..=4999 => 2,        // 2-5μs
            5000..=9999 => 3,        // 5-10μs
            10000..=19999 => 4,      // 10-20μs
            20000..=49999 => 5,      // 20-50μs
            50000..=99999 => 6,      // 50-100μs
            100000..=499999 => 7,    // 100-500μs
            500000..=999999 => 8,    // 500μs-1ms
            1000000..=4999999 => 9,  // 1-5ms
            5000000..=9999999 => 10, // 5-10ms
            _ => 11,                 // >10ms
        };

        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.sum_ns.fetch_add(latency_ns, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);

        // Update min (CAS loop)
        let mut current = self.min_ns.load(Ordering::Relaxed);
        while latency_ns < current {
            match self.min_ns.compare_exchange_weak(
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
        let mut current = self.max_ns.load(Ordering::Relaxed);
        while latency_ns > current {
            match self.max_ns.compare_exchange_weak(
                current,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        // Store sample for percentile calculation
        let idx = self.sample_idx.fetch_add(1, Ordering::Relaxed) as usize % 100_000;
        self.samples[idx].store(latency_ns, Ordering::Relaxed);
    }

    fn percentile(&self, p: f64) -> u64 {
        let count = self.count.load(Ordering::Relaxed) as usize;
        if count == 0 {
            return 0;
        }

        let sample_count = count.min(100_000);
        let mut samples: Vec<u64> = (0..sample_count)
            .map(|i| self.samples[i].load(Ordering::Relaxed))
            .filter(|&x| x > 0)
            .collect();

        if samples.is_empty() {
            return 0;
        }

        samples.sort_unstable();
        let idx = ((samples.len() as f64 * p / 100.0) as usize).min(samples.len() - 1);
        samples[idx]
    }

    fn print_report(&self) {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            println!("  No samples collected");
            return;
        }

        let min = self.min_ns.load(Ordering::Relaxed);
        let max = self.max_ns.load(Ordering::Relaxed);
        let sum = self.sum_ns.load(Ordering::Relaxed);
        let avg = sum / count;

        println!("\n📊 LATENCY REPORT (Rust-to-Rust)");
        println!("================================");
        println!("  Samples:    {}", count);
        println!("  Min:        {:.2} μs ({} ns)", min as f64 / 1000.0, min);
        println!("  Max:        {:.2} μs ({} ns)", max as f64 / 1000.0, max);
        println!("  Avg:        {:.2} μs ({} ns)", avg as f64 / 1000.0, avg);

        // Percentiles
        let p50 = self.percentile(50.0);
        let p90 = self.percentile(90.0);
        let p95 = self.percentile(95.0);
        let p99 = self.percentile(99.0);
        let p999 = self.percentile(99.9);

        println!("\n  Percentiles:");
        println!("    P50:      {:.2} μs", p50 as f64 / 1000.0);
        println!("    P90:      {:.2} μs", p90 as f64 / 1000.0);
        println!("    P95:      {:.2} μs", p95 as f64 / 1000.0);
        println!("    P99:      {:.2} μs", p99 as f64 / 1000.0);
        println!("    P99.9:    {:.2} μs", p999 as f64 / 1000.0);

        // Histogram
        println!("\n  Histogram:");
        let bucket_names = [
            "0-1μs",
            "1-2μs",
            "2-5μs",
            "5-10μs",
            "10-20μs",
            "20-50μs",
            "50-100μs",
            "100-500μs",
            "500μs-1ms",
            "1-5ms",
            "5-10ms",
            ">10ms",
        ];

        for (i, name) in bucket_names.iter().enumerate() {
            let bucket_count = self.buckets[i].load(Ordering::Relaxed);
            if bucket_count > 0 {
                let pct = bucket_count as f64 / count as f64 * 100.0;
                let bar_len = (pct / 2.0) as usize;
                let bar: String = "█".repeat(bar_len.min(40));
                println!("    {:12} {:6} ({:5.1}%) {}", name, bucket_count, pct, bar);
            }
        }

        // Verdict
        println!();
        if p99 < 10_000 {
            println!("  🏆 P99 < 10μs - EXCEPTIONAL! HFT-GRADE PERFORMANCE!");
        } else if p99 < 20_000 {
            println!("  ✅ P99 < 20μs - EXCELLENT! Production ready for HFT.");
        } else if p99 < 50_000 {
            println!("  ✅ P99 < 50μs - GREAT! Suitable for most trading systems.");
        } else if p99 < 100_000 {
            println!("  ⚠️  P99 < 100μs - GOOD, but room for improvement.");
        } else {
            println!("  ❌ P99 >= 100μs - Needs optimization.");
        }
    }
}

/// Token Analysis structure (must match battle_test.rs)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TokenAnalysis {
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

const TOKEN_ANALYSIS_SIZE: usize = std::mem::size_of::<TokenAnalysis>();

impl TokenAnalysis {
    #[inline(always)]
    unsafe fn from_bytes(data: &[u8]) -> Option<&Self> {
        unsafe {
            if data.len() < TOKEN_ANALYSIS_SIZE {
                return None;
            }
            Some(&*(data.as_ptr() as *const Self))
        }
    }
}

/// Subscriber configuration
struct SubscriberConfig {
    host: String,
    duration_secs: u64,
    verbose: bool,
}

impl Default for SubscriberConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1:9999".to_string(),
            duration_secs: 60,
            verbose: false,
        }
    }
}

/// Run the subscriber
fn run_subscriber(config: SubscriberConfig) -> io::Result<()> {
    println!("🦀 HERMES RUST SUBSCRIBER - Zero-Allocation Benchmark");
    println!("=====================================================\n");

    println!("Configuration:");
    println!("  Server:     {}", config.host);
    println!("  Duration:   {}s", config.duration_secs);
    println!();

    // Connect to server
    println!("🔌 Connecting to Hermes...");
    let mut stream = TcpStream::connect(&config.host)?;

    // CRITICAL: TCP_NODELAY
    stream.set_nodelay(true)?;
    // Use non-blocking mode instead of timeout
    stream.set_nonblocking(true)?;

    // Increase socket buffer
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = stream.as_raw_fd();
        unsafe {
            let optval: libc::c_int = 256 * 1024;
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }

    println!("   Connected! TCP_NODELAY=true\n");

    // Pre-allocate receive buffer (ZERO ALLOCATION in hot path)
    let mut recv_buffer = vec![0u8; 256 * 1024]; // 256KB
    let mut buffer_pos = 0usize;

    // Statistics (lock-free)
    let histogram = Arc::new(LatencyHistogram::new());
    let messages_received = Arc::new(AtomicU64::new(0));
    let honeypots_detected = Arc::new(AtomicU64::new(0));
    let running = Arc::new(AtomicBool::new(true));

    // Duration tracking
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(config.duration_secs);

    println!("📡 Listening for {} seconds...\n", config.duration_secs);

    // Main receive loop
    while Instant::now() < end_time && running.load(Ordering::Relaxed) {
        // Non-blocking read
        match stream.read(&mut recv_buffer[buffer_pos..]) {
            Ok(0) => {
                // Connection closed
                println!("Connection closed by server");
                break;
            }
            Ok(n) => {
                let recv_time_ns = now_ns();
                buffer_pos += n;

                // Process complete messages
                let mut consumed = 0;
                while consumed + HEADER_SIZE <= buffer_pos {
                    // Decode header
                    let mut decoder = Decoder::new(&recv_buffer[consumed..buffer_pos]);

                    match decoder.next() {
                        Some((header, payload)) => {
                            let msg_size = HEADER_SIZE + payload.len();
                            consumed += msg_size;

                            // Only process Publish messages
                            if header.msg_type != MessageType::Publish as u8 {
                                continue;
                            }

                            messages_received.fetch_add(1, Ordering::Relaxed);

                            // Parse token analysis (zero-copy)
                            if let Some(analysis) = unsafe { TokenAnalysis::from_bytes(payload) } {
                                // Calculate E2E latency
                                let analysis_ts = analysis.analysis_timestamp_ns;
                                let latency_ns = recv_time_ns.saturating_sub(analysis_ts);

                                histogram.record(latency_ns);

                                if analysis.honeypot_status == 1 {
                                    honeypots_detected.fetch_add(1, Ordering::Relaxed);
                                }

                                // Verbose output
                                if config.verbose {
                                    let count = messages_received.load(Ordering::Relaxed);
                                    if count % 100 == 0 {
                                        println!(
                                            "  [{}] Latency: {:.2}μs Risk:{} HP:{}",
                                            count,
                                            latency_ns as f64 / 1000.0,
                                            analysis.risk_score,
                                            analysis.honeypot_status == 1
                                        );
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }

                // Shift remaining data
                if consumed > 0 && consumed < buffer_pos {
                    recv_buffer.copy_within(consumed..buffer_pos, 0);
                    buffer_pos -= consumed;
                } else if consumed == buffer_pos {
                    buffer_pos = 0;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No data available - minimal yield to prevent 100% CPU
                std::hint::spin_loop();
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                // Timeout - yield and continue
                std::hint::spin_loop();
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }

        // Progress indicator every 5 seconds
        let elapsed = start_time.elapsed();
        if elapsed.as_secs() % 5 == 0 && elapsed.subsec_millis() < 100 {
            let count = messages_received.load(Ordering::Relaxed);
            let rate = count as f64 / elapsed.as_secs_f64();
            print!(
                "\r  Progress: {}s | {} msgs | {:.1} msg/s    ",
                elapsed.as_secs(),
                count,
                rate
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
    }

    let total_duration = start_time.elapsed();

    // Print results
    println!("\n\n");
    println!("═══════════════════════════════════════════════════════");
    println!("📊 RUST-TO-RUST BENCHMARK RESULTS");
    println!("═══════════════════════════════════════════════════════");

    let total_msgs = messages_received.load(Ordering::Relaxed);
    let honeypots = honeypots_detected.load(Ordering::Relaxed);

    println!("\nReception Summary:");
    println!("  Duration:      {:.2}s", total_duration.as_secs_f64());
    println!("  Messages:      {}", total_msgs);
    println!(
        "  Honeypots:     {} ({:.1}%)",
        honeypots,
        if total_msgs > 0 {
            honeypots as f64 / total_msgs as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Throughput:    {:.1} msg/sec",
        total_msgs as f64 / total_duration.as_secs_f64()
    );

    histogram.print_report();

    println!("\n═══════════════════════════════════════════════════════");

    Ok(())
}

fn parse_args() -> SubscriberConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = SubscriberConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    config.host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--duration" | "-d" => {
                if i + 1 < args.len() {
                    config.duration_secs = args[i + 1].parse().unwrap_or(60);
                    i += 1;
                }
            }
            "--verbose" | "-v" => {
                config.verbose = true;
            }
            "--help" => {
                println!("Hermes Rust Subscriber - Zero-Allocation Benchmark\n");
                println!("Usage: hermes_subscriber [OPTIONS]\n");
                println!("Options:");
                println!("  -h, --host <ADDR>      Server address (default: 127.0.0.1:9999)");
                println!("  -d, --duration <SEC>   Test duration (default: 60)");
                println!("  -v, --verbose          Verbose output");
                println!("      --help             Show this help");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    config
}

fn main() {
    let config = parse_args();

    if let Err(e) = run_subscriber(config) {
        eprintln!("❌ Subscriber error: {}", e);
        eprintln!("\n💡 Make sure Hermes server is running:");
        eprintln!("   cargo run --release --bin hermes_server");
        std::process::exit(1);
    }
}
