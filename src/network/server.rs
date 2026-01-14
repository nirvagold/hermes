//! Hermes Server dengan event-driven I/O
//!
//! Menggunakan mio untuk non-blocking I/O multiplexing.
//!
//! Note: This module provides the mio-based server implementation.
//! For production use, see src/bin/hermes_server.rs which uses
//! a simpler synchronous approach for better Windows compatibility.

#![allow(dead_code)] // Server module is for future async implementation

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

use mio::net::TcpListener as MioTcpListener;
use mio::{Events, Interest, Poll, Token};

use super::Connection;
use crate::core::RingBuffer;
use crate::protocol::{Decoder, Encoder, MessageType, HEADER_SIZE};

const SERVER_TOKEN: Token = Token(0);
const MAX_CONNECTIONS: usize = 1024;
const EVENTS_CAPACITY: usize = 1024;

/// Hermes Server
///
/// Event-driven server dengan:
/// - Non-blocking I/O (epoll/kqueue/IOCP)
/// - Pre-allocated connection slots
/// - Integrated ring buffer untuk message queue
pub struct Server {
    poll: Poll,
    listener: MioTcpListener,
    connections: HashMap<Token, Connection>,
    next_token: usize,
    // Message queue - shared ring buffer
    message_queue: RingBuffer<QueuedMessage, 65536>,
    // Pre-allocated encoder untuk responses
    encoder: Encoder,
}

/// Message dalam queue
#[derive(Clone, Copy)]
struct QueuedMessage {
    source_token: usize,
    sequence: u64,
    payload_offset: usize,
    payload_len: usize,
}

impl Server {
    /// Membuat server baru
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let poll = Poll::new()?;

        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let mut listener = MioTcpListener::from_std(listener);

        poll.registry()
            .register(&mut listener, SERVER_TOKEN, Interest::READABLE)?;

        Ok(Self {
            poll,
            listener,
            connections: HashMap::with_capacity(MAX_CONNECTIONS),
            next_token: 1,
            message_queue: RingBuffer::new(),
            encoder: Encoder::new(1024 * 1024), // 1MB encoder buffer
        })
    }

    /// Run server event loop
    pub fn run(&mut self) -> io::Result<()> {
        let mut events = Events::with_capacity(EVENTS_CAPACITY);

        println!(
            "Hermes server listening on {:?}",
            self.listener.local_addr()?
        );

        loop {
            // Poll dengan timeout 1ms untuk responsiveness
            self.poll
                .poll(&mut events, Some(Duration::from_millis(1)))?;

            for event in events.iter() {
                match event.token() {
                    SERVER_TOKEN => self.accept_connections()?,
                    token => {
                        if event.is_readable() {
                            self.handle_read(token)?;
                        }
                        if event.is_writable() {
                            self.handle_write(token)?;
                        }
                    }
                }
            }

            // Process message queue
            self.process_queue()?;
        }
    }

    /// Accept new connections
    fn accept_connections(&mut self) -> io::Result<()> {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    if self.connections.len() >= MAX_CONNECTIONS {
                        eprintln!("Max connections reached, rejecting {}", addr);
                        continue;
                    }

                    let token = Token(self.next_token);
                    self.next_token += 1;

                    // Convert mio TcpStream to std TcpStream
                    #[cfg(windows)]
                    let std_stream = {
                        use std::os::windows::io::{AsRawSocket, FromRawSocket};
                        unsafe { std::net::TcpStream::from_raw_socket(stream.as_raw_socket()) }
                    };

                    #[cfg(unix)]
                    let std_stream = {
                        use std::os::unix::io::{AsRawFd, FromRawFd};
                        unsafe { std::net::TcpStream::from_raw_fd(stream.as_raw_fd()) }
                    };

                    let conn = Connection::new(std_stream)?;

                    // Register untuk read events
                    let mut mio_stream = mio::net::TcpStream::from_std(conn.stream().try_clone()?);
                    self.poll
                        .registry()
                        .register(&mut mio_stream, token, Interest::READABLE)?;

                    self.connections.insert(token, conn);
                    println!("New connection from {} (token: {:?})", addr, token);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Handle readable event
    fn handle_read(&mut self, token: Token) -> io::Result<()> {
        let conn = match self.connections.get_mut(&token) {
            Some(c) => c,
            None => return Ok(()),
        };

        // Fill read buffer
        match conn.fill_read_buffer() {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
                self.connections.remove(&token);
                println!("Connection {:?} closed", token);
                return Ok(());
            }
            Err(e) => return Err(e),
        }

        // Copy readable data untuk decode (menghindari borrow conflict)
        let readable_data = conn.readable().to_vec();
        let mut decoder = Decoder::new(&readable_data);
        let mut consumed = 0;
        let mut responses: Vec<(u64, Vec<u8>)> = Vec::new();

        while let Some((header, payload)) = decoder.next() {
            consumed += HEADER_SIZE + payload.len();

            match MessageType::from_u8(header.msg_type) {
                Some(MessageType::Publish) => {
                    // Queue message untuk broadcast
                    let msg = QueuedMessage {
                        source_token: token.0,
                        sequence: header.sequence,
                        payload_offset: 0,
                        payload_len: payload.len(),
                    };
                    let _ = self.message_queue.push(msg);
                }
                Some(MessageType::Heartbeat) => {
                    // Queue heartbeat response
                    responses.push((header.sequence, Vec::new()));
                }
                _ => {}
            }
        }

        // Get connection again untuk write responses
        if let Some(conn) = self.connections.get_mut(&token) {
            conn.consume(consumed);

            for (seq, _) in responses {
                self.encoder.reset();
                if let Some(response) = self.encoder.encode(MessageType::Ack, seq, &[]) {
                    let _ = conn.queue_write(response);
                }
            }
        }

        Ok(())
    }

    /// Handle writable event
    fn handle_write(&mut self, token: Token) -> io::Result<()> {
        if let Some(conn) = self.connections.get_mut(&token) {
            conn.flush_write_buffer()?;
        }
        Ok(())
    }

    /// Process message queue
    fn process_queue(&mut self) -> io::Result<()> {
        while let Some(_msg) = self.message_queue.pop() {
            // In real implementation: broadcast to subscribers
            // For PoC, just drain the queue
        }
        Ok(())
    }
}
