//! Synchronous ICMP echo probing for the three-carrier probes.
//!
//! The probe threads in `status.rs` are plain std threads (1 s sample
//! period, 1 s timeout), so this module is deliberately sync: one
//! blocking socket per probe cell, driven by `set_read_timeout`.
//! No external `ping` binary is spawned and no command output is parsed.
//!
//! Socket strategy, in order:
//! 1. `SOCK_DGRAM` with `IPPROTO_ICMP`/`IPPROTO_ICMPV6` (the kernel "ping"
//!    socket: no privilege needed where `ping_group_range` allows it,
//!    and the kernel handles checksums and the IP header). The socket
//!    is bound to its identifier before connect: an unbound ping socket
//!    lets the kernel pick (and, on some kernels such as
//!    5.15.0-oracle, rewrite) the identifier, so the reply ident no
//!    longer matches the sent ident and every sample times out with no
//!    error. A bound ping socket keeps the ident it was bound to.
//! 2. `SOCK_RAW` with the same protocol (needs `CAP_NET_RAW`, which the
//!    client has: every fleet service file runs it as root). Raw
//!    sockets are left unbound: the kernel does not rewrite the ident
//!    on raw sockets.
//! If both fail, the caller logs once per target and counts the sample
//! as lost. There is intentionally NO fallback to TCP connect: a
//! port-dependent result is exactly the misleading data this module
//! replaced.

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io;
use std::mem::MaybeUninit;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

/// How the socket was obtained, for the one-time startup log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketMethod {
    /// Kernel ping socket (`SOCK_DGRAM`).
    Dgram,
    /// Raw socket (`SOCK_RAW`, root only).
    Raw,
}

/// Failure of a single echo attempt. Anything that is not a matched echo
/// reply within the timeout counts as a lost sample upstream.
#[derive(Debug)]
pub enum PingError {
    Timeout,
    Io(io::Error),
}

impl From<io::Error> for PingError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock {
            PingError::Timeout
        } else {
            PingError::Io(e)
        }
    }
}

fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for c in &mut chunks {
        sum += u16::from_be_bytes([c[0], c[1]]) as u32;
    }
    if let Some(&last) = chunks.remainder().first() {
        sum += (last as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn checksum_v6(src: IpAddr, dst: IpAddr, msg: &[u8]) -> u16 {
    let mut pseudo = Vec::with_capacity(40 + msg.len());
    match (src, dst) {
        (IpAddr::V6(s), IpAddr::V6(d)) => {
            pseudo.extend_from_slice(&s.octets());
            pseudo.extend_from_slice(&d.octets());
        }
        _ => return 0,
    }
    pseudo.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    pseudo.extend_from_slice(&[0, 0, 0, 58]); // next header: ICMPv6
    pseudo.extend_from_slice(msg);
    internet_checksum(&pseudo)
}

/// One probe target's ICMP socket. Created lazily per target so a target
/// change (server push) rebuilds it; kept across samples otherwise.
pub struct IcmpPinger {
    socket: Socket,
    target: IpAddr,
    is_v6: bool,
    /// True when recv() returns the ICMP message without an IP header
    /// (DGRAM sockets, and RAW ICMPv6 sockets).
    no_ip_header: bool,
    ident: u16,
    pub method: SocketMethod,
}

/// Process-wide ident dispenser. Every live pinger needs a distinct
/// identifier: two DGRAM ping sockets bound to the same ident conflict
/// (the second bind fails with AddrInUse), and the kernel routes
/// replies by ident, so a shared ident would also mix cells' replies.
/// Seeded from the pid (the old behavior for the first pinger) and
/// incremented per pinger; 0 is skipped because bind port 0 means
/// "kernel assigns", which is exactly the failure this fixes. One
/// live pinger per probe cell, so per-pinger uniqueness implies
/// per-cell uniqueness.
static IDENT_SEQ: AtomicU16 = AtomicU16::new(0);

fn next_ident() -> u16 {
    let base = std::process::id() as u16;
    loop {
        let ident = base.wrapping_add(IDENT_SEQ.fetch_add(1, Ordering::Relaxed));
        if ident != 0 {
            return ident;
        }
    }
}

impl IcmpPinger {
    /// Open a socket for `target`, preferring DGRAM over RAW.
    ///
    /// DGRAM sockets are bound to their identifier before connect (see
    /// the module docs for why: unbound ping sockets get their ident
    /// rewritten on some kernels). Each attempt uses a fresh ident;
    /// an AddrInUse bind (another process holding that ident) retries
    /// with the next one. RAW sockets are left unbound, as before.
    pub fn new(target: IpAddr) -> io::Result<Self> {
        let is_v6 = target.is_ipv6();
        let (domain, proto) = if is_v6 {
            (Domain::IPV6, Protocol::ICMPV6)
        } else {
            (Domain::IPV4, Protocol::ICMPV4)
        };
        let mut dgram_err: Option<io::Error> = None;
        for _ in 0..8 {
            let ident = next_ident();
            match Socket::new(domain, Type::DGRAM, Some(proto)) {
                Ok(s) => {
                    let bind_ip = if is_v6 {
                        IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)
                    } else {
                        IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
                    };
                    match s.bind(&SockAddr::from(std::net::SocketAddr::new(bind_ip, ident))) {
                        Ok(()) => {
                            // Connect so send/recv need no address each round and incoming
                            // packets are filtered to this peer; it also pins the source
                            // address used for the v6 pseudo-header checksum.
                            s.connect(&SockAddr::from(std::net::SocketAddr::new(target, 0)))?;
                            return Ok(Self {
                                socket: s,
                                target,
                                is_v6,
                                no_ip_header: true,
                                ident,
                                method: SocketMethod::Dgram,
                            });
                        }
                        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                            dgram_err = Some(e);
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => {
                    dgram_err = Some(e);
                    break;
                }
            }
        }
        match Socket::new(domain, Type::RAW, Some(proto)) {
            Ok(s) => {
                s.connect(&SockAddr::from(std::net::SocketAddr::new(target, 0)))?;
                Ok(Self {
                    socket: s,
                    target,
                    is_v6,
                    no_ip_header: is_v6,
                    ident: next_ident(),
                    method: SocketMethod::Raw,
                })
            }
            Err(_) => Err(dgram_err.unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "no ICMP socket available")
            })),
        }
    }

    fn build_request(&self, seq: u16) -> Vec<u8> {
        // Echo request: type, code, checksum, ident, seq, 32-byte payload.
        let mut msg = vec![0u8; 8 + 32];
        msg[0] = if self.is_v6 { 128 } else { 8 };
        msg[1] = 0;
        msg[4..6].copy_from_slice(&self.ident.to_be_bytes());
        msg[6..8].copy_from_slice(&seq.to_be_bytes());
        for (i, b) in msg[8..].iter_mut().enumerate() {
            *b = (i & 0xff) as u8;
        }
        let sum = if self.is_v6 {
            let src = self
                .socket
                .local_addr()
                .ok()
                .and_then(|a| a.as_socket().map(|s| s.ip()))
                .unwrap_or(IpAddr::from([0u8; 16]));
            checksum_v6(src, self.target, &msg)
        } else {
            internet_checksum(&msg)
        };
        msg[2..4].copy_from_slice(&sum.to_be_bytes());
        msg
    }

    /// Send one echo request and wait for the matching reply.
    /// Returns the round-trip time, or `PingError::Timeout` (plus other
    /// I/O errors, which the caller also treats as a lost sample).
    pub fn ping(&self, seq: u16, timeout: Duration) -> Result<Duration, PingError> {
        let req = self.build_request(seq);
        let start = Instant::now();
        self.socket.send(&req)?;
        // socket2 0.6 recv() takes MaybeUninit storage.
        let mut raw = [MaybeUninit::<u8>::uninit(); 512];
        loop {
            let left = timeout.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Err(PingError::Timeout);
            }
            self.socket.set_read_timeout(Some(left))?;
            let n = match self.socket.recv(&mut raw) {
                Ok(n) => n,
                Err(e) => return Err(PingError::from(e)),
            };
            // SAFETY: recv() initializes exactly the first n bytes.
            let buf = unsafe { std::slice::from_raw_parts(raw.as_ptr() as *const u8, n) };
            let icmp = if self.no_ip_header {
                buf
            } else {
                // RAW IPv4 recv() includes the IP header; skip it.
                if buf.is_empty() {
                    continue;
                }
                let ihl = ((buf[0] & 0x0f) as usize) * 4;
                if buf.len() < ihl + 8 {
                    continue;
                }
                &buf[ihl..]
            };
            if icmp.len() < 8 {
                continue;
            }
            let reply_type = if self.is_v6 { 129 } else { 0 };
            if icmp[0] == reply_type
                && icmp[1] == 0
                && icmp[4..6] == self.ident.to_be_bytes()
                && icmp[6..8] == seq.to_be_bytes()
            {
                return Ok(start.elapsed());
            }
            // Any other ICMP message (e.g. destination unreachable) is
            // not a reply: keep waiting until the timeout expires.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn checksum_is_self_consistent() {
        let mut msg = vec![8u8, 0, 0, 0, 0x12, 0x34, 0, 1];
        msg.extend_from_slice(b"payload");
        let sum = internet_checksum(&msg);
        msg[2..4].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(internet_checksum(&msg), 0);
    }

    #[test]
    fn v6_checksum_is_self_consistent() {
        let src: IpAddr = Ipv6Addr::new(0x2400, 0, 0, 0, 0, 0, 0, 1).into();
        let dst: IpAddr = Ipv6Addr::new(0x2400, 0, 0, 0, 0, 0, 0, 2).into();
        let mut msg = vec![128u8, 0, 0, 0, 0x12, 0x34, 0, 1];
        msg.extend_from_slice(b"payload");
        let sum = checksum_v6(src, dst, &msg);
        msg[2..4].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(checksum_v6(src, dst, &msg), 0);
    }

    /// Live loopback test: needs permission to open an ICMP socket
    /// (root, or ping_group_range). When the sandbox cannot, the test
    /// skips instead of failing on environment, not on code.
    #[test]
    fn loopback_echo_round_trip() {
        let target = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let pinger = match IcmpPinger::new(target) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping live ICMP test: cannot open socket: {e}");
                return;
            }
        };
        let rtt = pinger
            .ping(1, Duration::from_millis(1000))
            .expect("loopback ICMP echo must reply");
        assert!(rtt < Duration::from_millis(1000));
    }

    /// Live timeout test: TEST-NET-1 never answers, so a short timeout
    /// must surface as `Timeout` (the sample the caller maps to a lost
    /// sample with TIMEOUT_MS). Same skip rule as above.
    #[test]
    fn unroutable_target_times_out() {
        let target = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let pinger = match IcmpPinger::new(target) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping live ICMP test: cannot open socket: {e}");
                return;
            }
        };
        let err = pinger
            .ping(1, Duration::from_millis(200))
            .expect_err("TEST-NET-1 must not answer");
        assert!(matches!(err, PingError::Timeout));
    }

    /// Bind check: a DGRAM pinger's socket must report the ident it
    /// was opened with as its local port. Same skip rule as the live
    /// tests above; RAW pingers skip (raw sockets are not bound).
    #[test]
    fn dgram_bind_sets_local_ident() {
        let target = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let pinger = match IcmpPinger::new(target) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping live ICMP test: cannot open socket: {e}");
                return;
            }
        };
        if pinger.method != SocketMethod::Dgram {
            eprintln!("skipping bind check: fell back to RAW socket");
            return;
        }
        let local = pinger
            .socket
            .local_addr()
            .expect("bound DGRAM socket must have a local address");
        let port = local
            .as_socket()
            .expect("local address must be an IP socket address")
            .port();
        assert_eq!(port, pinger.ident);
        assert_ne!(pinger.ident, 0);
    }

    /// Cell uniqueness: six pingers held alive at once (the fleet's
    /// six probe cells) must all get distinct, nonzero idents, or
    /// their binds would conflict. Same skip rule as above.
    #[test]
    fn six_pingers_get_unique_idents() {
        let target = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut pingers = Vec::new();
        for _ in 0..6 {
            match IcmpPinger::new(target) {
                Ok(p) => {
                    if p.method != SocketMethod::Dgram {
                        eprintln!("skipping uniqueness check: fell back to RAW socket");
                        return;
                    }
                    pingers.push(p);
                }
                Err(e) => {
                    eprintln!("skipping live ICMP test: cannot open socket: {e}");
                    return;
                }
            }
        }
        let mut idents: Vec<u16> = pingers.iter().map(|p| p.ident).collect();
        assert_eq!(idents.len(), 6);
        assert!(idents.iter().all(|i| *i != 0));
        idents.sort_unstable();
        idents.dedup();
        assert_eq!(idents.len(), 6, "idents must be unique");
    }
}
