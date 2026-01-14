//! Battle Test - Live Combat Simulation
//!
//! Simulasi End-to-End untuk mengukur latency riil antara:
//! - Ruster Shield (Analisis) -> Hermes -> Sniper Bot (Eksekusi)
//!
//! Skenario:
//! 1. Simulasi REVM dummy (beban kerja analisis)
//! 2. Kirim hasil ke Hermes dengan timestamp nanodetik
//! 3. Python client mencatat waktu terima untuk hitung E2E latency
//!
//! Usage:
//!   cargo run --release --example battle_test -- [options]
//!
//! Options:
//!   --tokens <N>     Jumlah token untuk simulasi (default: 1000)
//!   --rate <N>       Token per detik (default: 100)
//!   --host <addr>    Hermes server address (default: 127.0.0.1:9999)

use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Import Hermes protocol
use hermes::protocol::{Encoder, MessageType};

/// Token Analysis Result - Data yang dikirim ke Hermes
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TokenAnalysis {
    /// Contract Address (32 bytes, hex-encoded first 32 chars)
    pub contract_address: [u8; 32],
    /// Chain ID (1 = ETH, 8453 = Base, 501 = Solana)
    pub chain_id: u32,
    /// Risk Score (0-100, higher = more risky)
    pub risk_score: u8,
    /// Honeypot Status (0 = Safe, 1 = Honeypot, 2 = Unknown)
    pub honeypot_status: u8,
    /// Buy Tax (0-100%)
    pub buy_tax: u8,
    /// Sell Tax (0-100%)
    pub sell_tax: u8,
    /// Timestamp saat analisis selesai (nanoseconds)
    pub analysis_timestamp_ns: u64,
    /// Liquidity dalam USD (scaled by 100)
    pub liquidity_usd: u64,
    /// Holder count
    pub holder_count: u32,
    /// Reserved for future use
    pub _reserved: [u8; 4],
}

const TOKEN_ANALYSIS_SIZE: usize = std::mem::size_of::<TokenAnalysis>();

impl TokenAnalysis {
    #[allow(clippy::too_many_arguments)]
    fn new(
        contract_address: &str,
        chain_id: u32,
        risk_score: u8,
        honeypot: bool,
        buy_tax: u8,
        sell_tax: u8,
        liquidity_usd: u64,
        holder_count: u32,
    ) -> Self {
        let mut ca = [0u8; 32];
        let bytes = contract_address.as_bytes();
        let len = bytes.len().min(32);
        ca[..len].copy_from_slice(&bytes[..len]);

        Self {
            contract_address: ca,
            chain_id,
            risk_score,
            honeypot_status: if honeypot { 1 } else { 0 },
            buy_tax,
            sell_tax,
            analysis_timestamp_ns: now_ns(),
            liquidity_usd,
            holder_count,
            _reserved: [0; 4],
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, TOKEN_ANALYSIS_SIZE) }
    }
}

/// Simulasi REVM - Dummy workload untuk merepresentasikan beban analisis
fn simulate_revm_analysis(token_index: u32) -> TokenAnalysis {
    // Simulasi beban CPU (dummy computation)
    // Dalam produksi, ini adalah REVM simulation yang berat
    let mut hash: u64 = token_index as u64;
    for _ in 0..1000 {
        hash = hash.wrapping_mul(6364136223846793005).wrapping_add(1);
    }

    // Generate fake contract address
    let ca = format!("0x{:016x}{:016x}", hash, hash.rotate_left(32));

    // Randomize risk metrics based on hash
    let risk_score = ((hash % 100) as u8).min(99);
    let is_honeypot = (hash % 10) < 2; // 20% honeypot rate
    let buy_tax = ((hash >> 8) % 30) as u8;
    let sell_tax = ((hash >> 16) % 50) as u8;
    let liquidity = (hash % 1_000_000) * 100; // $0 - $1M
    let holders = ((hash >> 24) % 10000) as u32;

    TokenAnalysis::new(
        &ca,
        8453, // Base chain
        risk_score,
        is_honeypot,
        buy_tax,
        sell_tax,
        liquidity,
        holders,
    )
}

/// Get current timestamp in nanoseconds
#[inline(always)]
fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Battle Test Configuration
struct BattleConfig {
    host: String,
    tokens: u32,
    rate: u32, // tokens per second
    verbose: bool,
}

impl Default for BattleConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1:9999".to_string(),
            tokens: 1000,
            rate: 100,
            verbose: false,
        }
    }
}

/// Latency Statistics
struct LatencyStats {
    samples: Vec<u64>,
    min_ns: u64,
    max_ns: u64,
    total_ns: u64,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(10000),
            min_ns: u64::MAX,
            max_ns: 0,
            total_ns: 0,
        }
    }

    fn record(&mut self, latency_ns: u64) {
        self.samples.push(latency_ns);
        self.min_ns = self.min_ns.min(latency_ns);
        self.max_ns = self.max_ns.max(latency_ns);
        self.total_ns += latency_ns;
    }

    fn percentile(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 * p / 100.0) as usize).min(sorted.len() - 1);
        sorted[idx]
    }

    fn print_report(&self) {
        if self.samples.is_empty() {
            println!("  No samples collected");
            return;
        }

        let avg_ns = self.total_ns / self.samples.len() as u64;
        let p50 = self.percentile(50.0);
        let p95 = self.percentile(95.0);
        let p99 = self.percentile(99.0);
        let p999 = self.percentile(99.9);

        println!("  Samples:    {}", self.samples.len());
        println!(
            "  Min:        {:.2} μs ({} ns)",
            self.min_ns as f64 / 1000.0,
            self.min_ns
        );
        println!(
            "  Max:        {:.2} μs ({} ns)",
            self.max_ns as f64 / 1000.0,
            self.max_ns
        );
        println!(
            "  Avg:        {:.2} μs ({} ns)",
            avg_ns as f64 / 1000.0,
            avg_ns
        );
        println!("  P50:        {:.2} μs", p50 as f64 / 1000.0);
        println!("  P95:        {:.2} μs", p95 as f64 / 1000.0);
        println!("  P99:        {:.2} μs", p99 as f64 / 1000.0);
        println!("  P99.9:      {:.2} μs", p999 as f64 / 1000.0);

        // Check if we meet the 50μs target
        if p99 < 50_000 {
            println!("\n  ✅ P99 < 50μs - BATTLE READY!");
        } else {
            println!("\n  ⚠️  P99 >= 50μs - Needs optimization");
        }
    }
}

/// Run the battle test
fn run_battle_test(config: &BattleConfig) -> std::io::Result<()> {
    println!("⚔️  HERMES BATTLE TEST - Live Combat Simulation");
    println!("================================================\n");

    println!("Configuration:");
    println!("  Server:     {}", config.host);
    println!("  Tokens:     {}", config.tokens);
    println!("  Rate:       {} tokens/sec", config.rate);
    println!();

    // Connect to Hermes
    println!("🔌 Connecting to Hermes server...");
    let mut stream = TcpStream::connect(&config.host)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    println!("   Connected!\n");

    // Pre-allocate encoder
    let mut encoder = Encoder::new(1024 * 1024);
    let mut stats = LatencyStats::new();

    // Calculate interval between tokens
    let interval_ns = 1_000_000_000u64 / config.rate as u64;

    println!(
        "🚀 Starting injection ({} tokens at {} tokens/sec)...\n",
        config.tokens, config.rate
    );

    let test_start = Instant::now();
    let mut next_send = Instant::now();
    let mut sent_count = 0u32;
    let mut honeypot_count = 0u32;

    for i in 0..config.tokens {
        // Wait until next send time (rate limiting)
        let now = Instant::now();
        if now < next_send {
            thread::sleep(next_send - now);
        }
        next_send = Instant::now() + Duration::from_nanos(interval_ns);

        // Simulate REVM analysis
        let analysis = simulate_revm_analysis(i);

        // Record send timestamp
        let send_timestamp_ns = now_ns();

        // Encode and send
        encoder.reset();
        let payload = analysis.as_bytes();

        if let Some(encoded) = encoder.encode(MessageType::Publish, i as u64, payload) {
            stream.write_all(encoded)?;
            sent_count += 1;

            if analysis.honeypot_status == 1 {
                honeypot_count += 1;
            }

            // Record encoding + send latency
            let send_latency = now_ns() - send_timestamp_ns;
            stats.record(send_latency);

            if config.verbose && i % 100 == 0 {
                println!(
                    "  [{}] CA: {}... Risk: {} Honeypot: {} Latency: {:.2}μs",
                    i,
                    String::from_utf8_lossy(&analysis.contract_address[..16]),
                    analysis.risk_score,
                    analysis.honeypot_status == 1,
                    send_latency as f64 / 1000.0
                );
            }
        }

        // Progress indicator
        if (i + 1) % 100 == 0 {
            print!(
                "\r  Progress: {}/{} ({:.1}%)",
                i + 1,
                config.tokens,
                (i + 1) as f64 / config.tokens as f64 * 100.0
            );
            std::io::stdout().flush().ok();
        }
    }

    let test_duration = test_start.elapsed();
    println!("\n");

    // Print results
    println!("📊 BATTLE TEST RESULTS");
    println!("======================\n");

    println!("Injection Summary:");
    println!("  Total tokens:    {}", config.tokens);
    println!("  Sent:            {}", sent_count);
    println!(
        "  Honeypots:       {} ({:.1}%)",
        honeypot_count,
        honeypot_count as f64 / sent_count as f64 * 100.0
    );
    println!("  Duration:        {:.2}s", test_duration.as_secs_f64());
    println!(
        "  Actual rate:     {:.1} tokens/sec\n",
        sent_count as f64 / test_duration.as_secs_f64()
    );

    println!("Send Latency (Rust -> Hermes):");
    stats.print_report();

    println!("\n💡 Tips:");
    println!("   - Run Python client simultaneously to measure E2E latency");
    println!("   - Use 'taskset -c 0,1 cargo run' on Linux for CPU pinning");
    println!("   - Monitor packet drops with 'netstat -s | grep -i drop'");

    Ok(())
}

/// Parse command line arguments
fn parse_args() -> BattleConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = BattleConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--tokens" | "-t" => {
                if i + 1 < args.len() {
                    config.tokens = args[i + 1].parse().unwrap_or(1000);
                    i += 1;
                }
            }
            "--rate" | "-r" => {
                if i + 1 < args.len() {
                    config.rate = args[i + 1].parse().unwrap_or(100);
                    i += 1;
                }
            }
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    config.host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--verbose" | "-v" => {
                config.verbose = true;
            }
            "--help" => {
                println!("Hermes Battle Test - Live Combat Simulation\n");
                println!("Usage: battle_test [OPTIONS]\n");
                println!("Options:");
                println!("  -t, --tokens <N>   Number of tokens to simulate (default: 1000)");
                println!("  -r, --rate <N>     Tokens per second (default: 100)");
                println!("  -h, --host <ADDR>  Hermes server address (default: 127.0.0.1:9999)");
                println!("  -v, --verbose      Show detailed output");
                println!("      --help         Show this help message");
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

    if let Err(e) = run_battle_test(&config) {
        eprintln!("❌ Battle test failed: {}", e);
        eprintln!("\n💡 Make sure Hermes server is running:");
        eprintln!("   cargo run --release --bin hermes_server");
        std::process::exit(1);
    }
}
