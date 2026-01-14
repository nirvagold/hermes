//! Hermes Server Binary - FIXED VERSION
//!
//! Ultra Low-Latency Message Broker Server dengan:
//! - Proper Pub/Sub broadcast logic
//! - TCP_NODELAY enabled
//! - Minimal sleep untuk low latency
//!
//! Usage:
//!   cargo run --release --bin hermes_server [OPTIONS]

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hermes::core::MmapStorage;
use hermes::protocol::{Decoder, MessageType, HEADER_SIZE};

/// Server configuration
struct ServerConfig {
    bind_addr: String,
    storage_path: String,
    storage_size_mb: usize,
    verbose: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:9999".to_string(),
            storage_path: "hermes_data.dat".to_string(),
            storage_size_mb: 64,
            verbose: false,
        }
    }
}

/// Server statistics
struct ServerStats {
    messages_received: AtomicU64,
    messages_broadcast: AtomicU64,
    messages_dropped: AtomicU64,
    bytes_received: AtomicU64,
    bytes_sent: AtomicU64,
    connections_total: AtomicU64,
    connections_active: AtomicU64,
    broadcast_errors: AtomicU64,
}

impl ServerStats {
    fn new() -> Self {
        Self {
            messages_received: AtomicU64::new(0),
            messages_broadcast: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            connections_total: AtomicU64::new(0),
            connections_active: AtomicU64::new(0),
            broadcast_errors: AtomicU64::new(0),
        }
    }

    fn print_stats(&self, uptime: Duration) {
        let msgs_in = self.messages_received.load(Ordering::Relaxed);
        let msgs_out = self.messages_broadcast.load(Ordering::Relaxed);
        let dropped = self.messages_dropped.load(Ordering::Relaxed);
        let bytes_in = self.bytes_received.load(Ordering::Relaxed);
        let bytes_out = self.bytes_sent.load(Ordering::Relaxed);
        let conns = self.connections_active.load(Ordering::Relaxed);
        let errors = self.broadcast_errors.load(Ordering::Relaxed);

        let rate_in = msgs_in as f64 / uptime.as_secs_f64();
        let rate_out = msgs_out as f64 / uptime.as_secs_f64();

        println!("\n📊 Server Stats (uptime: {:.1}s)", uptime.as_secs_f64());
        println!("   Messages IN:   {} ({:.1}/sec)", msgs_in, rate_in);
        println!("   Messages OUT:  {} ({:.1}/sec)", msgs_out, rate_out);
        println!("   Dropped:       {} ⚠️", dropped);
        println!("   Bytes in:      {} KB", bytes_in / 1024);
        println!("   Bytes out:     {} KB", bytes_out / 1024);
        println!("   Connections:   {}", conns);
        if errors > 0 {
            println!("   Send errors:   {} ⚠️", errors);
        }
    }
}

/// Client connection with role
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClientRole {
    Unknown,
    Publisher,  // Sends data (Rust Injector)
    Subscriber, // Receives data (Python Monitor)
}

/// Client connection handler
struct ClientHandler {
    stream: TcpStream,
    addr: SocketAddr,
    role: ClientRole,
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
    read_pos: usize,
    messages_sent: u64,
    messages_received: u64,
}

impl ClientHandler {
    fn new(stream: TcpStream, addr: SocketAddr) -> io::Result<Self> {
        // CRITICAL: TCP_NODELAY untuk low latency
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;

        // Set socket buffer sizes untuk throughput
        // Ignore errors - not all platforms support this
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = stream.as_raw_fd();
            unsafe {
                let optval: libc::c_int = 256 * 1024; // 256KB
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_SNDBUF,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_RCVBUF,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }

        Ok(Self {
            stream,
            addr,
            role: ClientRole::Unknown,
            read_buffer: vec![0u8; 128 * 1024], // 128KB read buffer
            write_buffer: Vec::with_capacity(128 * 1024),
            read_pos: 0,
            messages_sent: 0,
            messages_received: 0,
        })
    }

    /// Try to read data from socket (non-blocking)
    fn try_read(&mut self) -> io::Result<usize> {
        if self.read_pos >= self.read_buffer.len() {
            // Buffer full - should not happen with proper consumption
            return Ok(0);
        }

        match self.stream.read(&mut self.read_buffer[self.read_pos..]) {
            Ok(0) => Ok(0), // Connection closed
            Ok(n) => {
                self.read_pos += n;
                Ok(n)
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// Process received messages, returns list of messages to broadcast
    fn process_messages(
        &mut self,
        storage: &mut MmapStorage,
        stats: &ServerStats,
    ) -> Vec<(usize, Vec<u8>)> {
        let mut broadcasts = Vec::new();

        if self.read_pos < HEADER_SIZE {
            return broadcasts;
        }

        let mut decoder = Decoder::new(&self.read_buffer[..self.read_pos]);
        let mut consumed = 0;

        while let Some((header, payload)) = decoder.next() {
            let msg_size = HEADER_SIZE + payload.len();

            // Extract full message BEFORE updating consumed
            let msg_start = consumed;
            let msg_end = consumed + msg_size;
            let full_msg = self.read_buffer[msg_start..msg_end].to_vec();

            consumed = msg_end;

            stats.messages_received.fetch_add(1, Ordering::Relaxed);
            stats
                .bytes_received
                .fetch_add(msg_size as u64, Ordering::Relaxed);
            self.messages_received += 1;

            match MessageType::from_u8(header.msg_type) {
                Some(MessageType::Publish) => {
                    // This client is a Publisher
                    if self.role == ClientRole::Unknown {
                        self.role = ClientRole::Publisher;
                    }

                    // Store to mmap for persistence
                    storage.write(&full_msg);

                    // Queue for broadcast (include message size for stats)
                    broadcasts.push((msg_size, full_msg));
                }
                Some(MessageType::Subscribe) => {
                    // This client wants to receive messages
                    self.role = ClientRole::Subscriber;
                }
                Some(MessageType::Heartbeat) => {
                    // Just acknowledge - client is alive
                }
                _ => {}
            }
        }

        // Shift remaining data to front of buffer
        if consumed > 0 {
            if consumed < self.read_pos {
                self.read_buffer.copy_within(consumed..self.read_pos, 0);
                self.read_pos -= consumed;
            } else {
                self.read_pos = 0;
            }
        }

        broadcasts
    }

    /// Send data to client (with buffering for WouldBlock)
    fn send(&mut self, data: &[u8]) -> io::Result<bool> {
        // First try to flush any pending data
        self.flush_pending()?;

        // If we still have pending data, buffer this too
        if !self.write_buffer.is_empty() {
            if self.write_buffer.len() + data.len() > 1024 * 1024 {
                // Buffer too large - drop message
                return Ok(false);
            }
            self.write_buffer.extend_from_slice(data);
            return Ok(true);
        }

        // Try direct send
        match self.stream.write_all(data) {
            Ok(_) => {
                self.messages_sent += 1;
                Ok(true)
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Buffer for later
                self.write_buffer.extend_from_slice(data);
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }

    /// Flush pending write buffer
    fn flush_pending(&mut self) -> io::Result<()> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }

        match self.stream.write(&self.write_buffer) {
            Ok(n) => {
                if n > 0 {
                    self.write_buffer.drain(..n);
                }
                Ok(())
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Check if connection is still alive
    #[allow(dead_code)]
    fn is_alive(&self) -> bool {
        let mut peek_buf = [0u8; 1];
        match self.stream.peek(&mut peek_buf) {
            Ok(0) => false, // EOF
            Ok(_) => true,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => true,
            Err(_) => false,
        }
    }
}

/// Main server loop
fn run_server(config: ServerConfig) -> io::Result<()> {
    println!("🚀 HERMES SERVER v2 - Fixed Broadcast");
    println!("=====================================\n");

    // Initialize storage
    let storage_size = config.storage_size_mb * 1024 * 1024;
    let mut storage = MmapStorage::open(&config.storage_path, storage_size)?;
    println!(
        "💾 Storage: {} ({} MB)",
        config.storage_path, config.storage_size_mb
    );

    // Bind listener with reuse
    let listener = TcpListener::bind(&config.bind_addr)?;
    listener.set_nonblocking(true)?;
    println!("🔌 Listening on {}", config.bind_addr);
    println!("⚡ TCP_NODELAY: ENABLED");
    println!("\n📡 Waiting for connections...\n");

    let stats = ServerStats::new();
    let start_time = Instant::now();
    let mut last_stats_print = Instant::now();

    let mut clients: HashMap<usize, ClientHandler> = HashMap::new();
    let mut next_client_id = 0usize;

    // Track which clients should receive broadcasts
    let mut subscriber_ids: Vec<usize> = Vec::new();

    loop {
        let _loop_start = Instant::now();

        // === PHASE 1: Accept new connections ===
        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    match ClientHandler::new(stream, addr) {
                        Ok(handler) => {
                            let id = next_client_id;
                            next_client_id += 1;

                            println!("✅ [{}] Connected: {} (TCP_NODELAY=true)", id, addr);
                            clients.insert(id, handler);

                            // New clients are potential subscribers
                            subscriber_ids.push(id);

                            stats.connections_total.fetch_add(1, Ordering::Relaxed);
                            stats.connections_active.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            eprintln!("⚠️ Failed to setup client: {}", e);
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }

        // === PHASE 2: Read from all clients ===
        let mut all_broadcasts: Vec<(usize, usize, Vec<u8>)> = Vec::new(); // (sender_id, msg_size, data)
        let mut disconnected: Vec<usize> = Vec::new();

        for (&id, client) in clients.iter_mut() {
            // Try to read
            match client.try_read() {
                Ok(0) => {
                    // No data read - this is normal for non-blocking sockets
                    // Only mark as disconnected if we get explicit EOF
                }
                Ok(n) => {
                    if config.verbose {
                        println!("   [{}] Read {} bytes", id, n);
                    }

                    // Process messages
                    let msgs = client.process_messages(&mut storage, &stats);
                    for (msg_size, msg_data) in msgs {
                        all_broadcasts.push((id, msg_size, msg_data));
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Normal for non-blocking - no data available
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                    println!("   [{}] Connection reset", id);
                    disconnected.push(id);
                }
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionAborted => {
                    println!("   [{}] Connection aborted", id);
                    disconnected.push(id);
                }
                Err(e) => {
                    // Only disconnect on real errors, not WouldBlock
                    if e.kind() != io::ErrorKind::WouldBlock {
                        eprintln!("⚠️ [{}] Read error: {} (kind: {:?})", id, e, e.kind());
                        disconnected.push(id);
                    }
                }
            }
        }

        // === PHASE 3: Broadcast to ALL OTHER clients ===
        for (_sender_id, _msg_size, msg_data) in &all_broadcasts {
            for (&client_id, client) in clients.iter_mut() {
                // Skip sender - don't echo back
                if client_id == *_sender_id {
                    continue;
                }

                // Send to this client
                match client.send(msg_data) {
                    Ok(true) => {
                        stats.messages_broadcast.fetch_add(1, Ordering::Relaxed);
                        stats
                            .bytes_sent
                            .fetch_add(msg_data.len() as u64, Ordering::Relaxed);
                    }
                    Ok(false) => {
                        stats.messages_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        stats.broadcast_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // === PHASE 4: Flush pending writes ===
        for client in clients.values_mut() {
            client.flush_pending().ok();
        }

        // === PHASE 5: Remove disconnected clients ===
        for id in disconnected {
            if let Some(client) = clients.remove(&id) {
                println!(
                    "❌ [{}] Disconnected: {} (sent: {}, recv: {})",
                    id, client.addr, client.messages_sent, client.messages_received
                );
                stats.connections_active.fetch_sub(1, Ordering::Relaxed);
                subscriber_ids.retain(|&x| x != id);
            }
        }

        // === PHASE 6: Print stats periodically ===
        if last_stats_print.elapsed() > Duration::from_secs(5) {
            stats.print_stats(start_time.elapsed());
            last_stats_print = Instant::now();
        }

        // === Adaptive sleep for CPU efficiency ===
        // ULTRA LOW LATENCY MODE: No sleep when active
        // Only yield briefly when completely idle
        if all_broadcasts.is_empty() && clients.is_empty() {
            // No clients, no work - sleep to save CPU
            std::thread::sleep(Duration::from_micros(100));
        } else if all_broadcasts.is_empty() {
            // Clients connected but no messages - minimal yield
            std::thread::yield_now();
        }
        // When processing messages: NO SLEEP - busy poll for minimum latency
    }
}

fn parse_args() -> ServerConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = ServerConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" | "-b" => {
                if i + 1 < args.len() {
                    config.bind_addr = args[i + 1].clone();
                    i += 1;
                }
            }
            "--storage" | "-s" => {
                if i + 1 < args.len() {
                    config.storage_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--size" => {
                if i + 1 < args.len() {
                    config.storage_size_mb = args[i + 1].parse().unwrap_or(64);
                    i += 1;
                }
            }
            "--verbose" | "-v" => {
                config.verbose = true;
            }
            "--help" | "-h" => {
                println!("Hermes Server v2 - Ultra Low-Latency Message Broker\n");
                println!("Usage: hermes_server [OPTIONS]\n");
                println!("Options:");
                println!("  -b, --bind <ADDR>     Bind address (default: 0.0.0.0:9999)");
                println!("  -s, --storage <PATH>  Storage file path (default: hermes_data.dat)");
                println!("      --size <MB>       Storage size in MB (default: 64)");
                println!("  -v, --verbose         Verbose output");
                println!("  -h, --help            Show this help");
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

    if let Err(e) = run_server(config) {
        eprintln!("❌ Server error: {}", e);
        std::process::exit(1);
    }
}
