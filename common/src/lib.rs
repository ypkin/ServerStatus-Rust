pub mod server_status {
    tonic::include_proto!("server_status");
}

/// Probe-target normalization, shared by server and client.
///
/// ICMP probes ping a host, not a socket: any port (and any v6 bracket
/// pair) in a stored probe address is noise and must be stripped before
/// the target is used or displayed. This keeps old saved values such as
/// `cuccv4.passall.us:8443` working without re-entry after the probe
/// mechanism switched from TCP-connect to ICMP echo.
///
/// Rules (applied in order):
/// - surrounding whitespace is trimmed;
/// - a leading `http://` / `https://` scheme is dropped;
/// - anything after the first `/`, `?` or `#` (path/query/fragment) is dropped;
/// - `[addr]` or `[addr]:port` yields `addr`;
/// - a single `:` followed by an empty or all-digit port yields the host
///   part (`example.com:8443` -> `example.com`);
/// - anything else is returned unchanged: bare IPs, bare hostnames, and
///   bare IPv6 literals (two or more colons are ambiguous with a port,
///   so they are never split).
pub mod probe {
    /// Strip a probe address down to the bare host ICMP pings.
    pub fn normalize_probe_target(raw: &str) -> String {
        let mut s = raw.trim();
        if let Some(rest) = s.strip_prefix("http://").or_else(|| s.strip_prefix("https://")) {
            s = rest;
        }
        if let Some(idx) = s.find(|c| c == '/' || c == '?' || c == '#') {
            s = &s[..idx];
        }
        let s = s.trim();
        if s.is_empty() {
            return String::new();
        }
        if let Some(rest) = s.strip_prefix('[') {
            if let Some(end) = rest.find(']') {
                return rest[..end].to_string();
            }
            return s.to_string();
        }
        if s.matches(':').count() == 1 {
            if let Some((host, port)) = s.split_once(':') {
                if port.bytes().all(|b| b.is_ascii_digit()) {
                    return host.to_string();
                }
            }
        }
        s.to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::normalize_probe_target;

        #[test]
        fn strips_ports_and_brackets() {
            assert_eq!(normalize_probe_target("cuccv4.passall.us:8443"), "cuccv4.passall.us");
            assert_eq!(normalize_probe_target("1.2.3.4:80"), "1.2.3.4");
            assert_eq!(normalize_probe_target("[2400::1]:80"), "2400::1");
            assert_eq!(normalize_probe_target("[2400::1]"), "2400::1");
        }

        #[test]
        fn leaves_bare_values_unchanged() {
            assert_eq!(normalize_probe_target("1.2.3.4"), "1.2.3.4");
            assert_eq!(normalize_probe_target("cuccv4.passall.us"), "cuccv4.passall.us");
            assert_eq!(normalize_probe_target(""), "");
            assert_eq!(normalize_probe_target("   "), "");
            // Bare IPv6 (two or more colons) is never split: a trailing
            // group is indistinguishable from a port.
            assert_eq!(normalize_probe_target("2400::1"), "2400::1");
            assert_eq!(normalize_probe_target("2001:db8::1:80"), "2001:db8::1:80");
        }

        #[test]
        fn trims_scheme_and_path_noise() {
            assert_eq!(normalize_probe_target("  http://example.com:80/x "), "example.com");
            assert_eq!(normalize_probe_target("https://example.com"), "example.com");
            assert_eq!(normalize_probe_target("example.com:8443?x=1"), "example.com");
        }
    }
}

pub mod utils {
    const SYMBOLS_LEN: usize = 6;
    const SYMBOLS: [&str; SYMBOLS_LEN] = ["B", "K", "M", "G", "T", "P"];
    const fn init_units() -> [[u64; SYMBOLS_LEN]; 2] {
        let mut arr = [[0_u64; SYMBOLS_LEN]; 2];
        let mut idx = 0;
        while idx < SYMBOLS_LEN {
            arr[0][idx] = u64::pow(2, (idx * 10) as u32);
            arr[1][idx] = u64::pow(10, (idx * 3) as u32);
            idx += 1;
        }
        arr
    }

    const UNITS: [[u64; SYMBOLS_LEN]; 2] = init_units();
    #[must_use] 
    pub fn bytes2human(value: u64, precision: usize, si: bool) -> String {
        let t: usize = usize::from(si);
        for idx in (0..SYMBOLS_LEN).rev() {
            let s = SYMBOLS[idx];
            if value >= UNITS[t][idx] {
                return format!("{:.*}{}", precision, value as f64 / UNITS[t][idx] as f64, s);
            }
        }
        format!("{:.*}B", precision, value as f64)
    }
}

#[allow(unused)]
#[cfg(test)]
mod tests {
    use crate::utils::bytes2human;

    #[test]
    fn test() {
        dbg!(bytes2human(536870912000, 2, false));
        dbg!(bytes2human(2048, 0, false));
        dbg!(bytes2human(0, 2, false));
        dbg!(bytes2human(1023, 0, false));
        dbg!(bytes2human(1024, 0, false));
        dbg!(bytes2human(u64::pow(1024, 2), 0, false));
        dbg!(bytes2human(u64::pow(1024, 3), 0, false));

        dbg!(bytes2human(100, 0, true));
        dbg!(bytes2human(1000, 0, true));
        dbg!(bytes2human(u64::pow(1000, 2), 0, true));
        dbg!(bytes2human(u64::pow(1000, 3), 0, true));
    }
}
