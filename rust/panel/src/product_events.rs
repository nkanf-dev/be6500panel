//! Bounded SSE sockets on the existing single listener lane. No client threads.
use crate::{auth::Auth, product_io::timestamp};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{self, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};
const MAX_STREAMS: usize = 8;
const MAX_EVENT_BYTES: usize = 64 << 10;
const WRITE_DEADLINE: Duration = Duration::from_secs(5);
struct Client {
    stream: TcpStream,
    cookie: Option<String>,
    pending: Vec<u8>,
    offset: usize,
    queued: Instant,
}
pub struct Streams {
    clients: VecDeque<Client>,
    sequence: u64,
    next_snapshot: Instant,
    next_heartbeat: Instant,
}
impl Default for Streams {
    fn default() -> Self {
        Self::new()
    }
}
impl Streams {
    pub fn new() -> Self {
        Self {
            clients: VecDeque::with_capacity(MAX_STREAMS),
            sequence: 0,
            next_snapshot: Instant::now(),
            next_heartbeat: Instant::now() + Duration::from_secs(15),
        }
    }
    pub fn len(&self) -> usize {
        self.clients.len()
    }
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
    pub fn due(&self) -> bool {
        !self.clients.is_empty() && Instant::now() >= self.next_snapshot
    }
    pub fn add(&mut self, mut stream: TcpStream, cookie: Option<&str>) -> io::Result<()> {
        if self.clients.len() == MAX_STREAMS {
            return Err(io::Error::other("stream limit"));
        }
        stream.set_write_timeout(Some(WRITE_DEADLINE))?;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\nX-Content-Type-Options: nosniff\r\n\r\n")?;
        stream.set_nonblocking(true)?;
        self.clients.push_back(Client {
            stream,
            cookie: cookie.map(str::to_owned),
            pending: Vec::new(),
            offset: 0,
            queued: Instant::now(),
        });
        self.next_snapshot = Instant::now();
        Ok(())
    }
    pub fn publish(
        &mut self,
        system: Result<Value, (&'static str, &'static str)>,
        now_unix: u64,
    ) -> io::Result<()> {
        self.sequence = self.sequence.saturating_add(1);
        let (kind, doc) = match system {
            Ok(system) => (
                "snapshot",
                json!({"system":system,"sampledAt":timestamp(now_unix)}),
            ),
            Err((code, message)) => (
                "observation_error",
                json!({"error":{"code":code,"message":message}}),
            ),
        };
        let raw = serde_json::to_vec(&doc).map_err(io::Error::other)?;
        if raw.len() > MAX_EVENT_BYTES {
            return Err(io::Error::other("event bound"));
        }
        let prefix = format!("id: {}\nevent: {kind}\ndata: ", self.sequence);
        let mut event = Vec::with_capacity(prefix.len() + raw.len() + 2);
        event.extend_from_slice(prefix.as_bytes());
        event.extend_from_slice(&raw);
        event.extend_from_slice(b"\n\n");
        let now = Instant::now();
        self.next_snapshot = now + Duration::from_secs(2);
        self.clients.retain_mut(|client| {
            if client.offset < client.pending.len() {
                if now.duration_since(client.queued) > WRITE_DEADLINE {
                    let _ = client.stream.shutdown(Shutdown::Both);
                    return false;
                }
                return true;
            }
            client.pending.clear();
            client.pending.extend_from_slice(&event);
            client.offset = 0;
            client.queued = now;
            true
        });
        Ok(())
    }
    pub fn flush(&mut self, auth: &mut Auth) {
        let now = Instant::now();
        let heartbeat = now >= self.next_heartbeat;
        if heartbeat {
            self.next_heartbeat = now + Duration::from_secs(15);
        }
        self.clients.retain_mut(|client| {
            if !auth.authenticated(client.cookie.as_deref()) {
                let _ = client.stream.shutdown(Shutdown::Both);
                return false;
            }
            if client.offset == client.pending.len() && heartbeat {
                client.pending.clear();
                client.pending.extend_from_slice(b": heartbeat\n\n");
                client.offset = 0;
                client.queued = now;
            }
            if client.offset == client.pending.len() {
                return true;
            }
            if now.duration_since(client.queued) > WRITE_DEADLINE {
                let _ = client.stream.shutdown(Shutdown::Both);
                return false;
            }
            loop {
                match client.stream.write(&client.pending[client.offset..]) {
                    Ok(0) => return false,
                    Ok(n) => {
                        client.offset += n;
                        if client.offset == client.pending.len() {
                            return true;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return true,
                    Err(_) => return false,
                }
            }
        });
    }
    pub fn close(&mut self) {
        for client in self.clients.drain(..) {
            let _ = client.stream.shutdown(Shutdown::Both);
        }
    }
}
impl Drop for Streams {
    fn drop(&mut self) {
        self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_auth_stream_emits_existing_snapshot_wire_without_threads() {
        use std::io::Read;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let server = listener.accept().unwrap().0;
        let mut streams = Streams::new();
        streams.add(server, None).unwrap();
        streams
            .publish(Ok(json!({"mode":"host","sampledAt":timestamp(0)})), 0)
            .unwrap();
        streams.flush(&mut Auth::new(""));
        let mut bytes = [0; 4096];
        let n = peer.read(&mut bytes).unwrap();
        let text = std::str::from_utf8(&bytes[..n]).unwrap();
        assert!(
            text.contains("text/event-stream")
                && text.contains("event: snapshot")
                && text.contains("\"system\"")
        );
        streams.close();
        assert!(streams.is_empty());
    }
}
