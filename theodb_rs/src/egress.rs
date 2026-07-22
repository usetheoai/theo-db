//! M134 (#117) — the **pure** egress address policy: which addresses a database host must never be steered at,
//! and which host the HTTP client will actually contact.
//!
//! Deliberately std-only and free of pgrx: the PG-facing guard (resolution, GUC lookup, `ereport`) lives in
//! `http::guard_egress`. Two reasons, both load-bearing:
//!
//! 1. **SRP** — the *decisions* live here (is this address internal? which host will actually be dialed?); the
//!    *effects* live in `http::guard_egress` (resolve, read the GUC, `ereport`). `endpoint_host` is admittedly
//!    transport knowledge rather than address policy — it mirrors minreq's parser — but it belongs on this side
//!    of the line because being pure is what makes it provable, and F1 proved that is where it must be provable.
//! 2. **It is the half that must be provable.** A missing range here silently PERMITS traffic, and most ranges
//!    (NAT64, 6to4, multicast) cannot be expressed in a URL the HTTP client will even parse — so the in-PG SQL
//!    matrix cannot reach them. Keeping this file dependency-free means it compiles and runs standalone
//!    (`rustc --test src/egress.rs`), which is how the range table is actually verified on a box where
//!    `cargo pgrx test` does not link.

/// M134 (#117) — is this address one a database host must never be steered at?
///
/// Blind SSRF: a role holding EXECUTE on an LLM-touching function could set the endpoint GUC and make the DB host
/// probe cloud metadata (`169.254.169.254`), loopback services, or internal RFC1918 hosts — observable through
/// timing and breaker state even when the reply is discarded. Pure function: every range is unit-testable with no
/// network.
pub(crate) fn is_blocked_addr(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()            // 127.0.0.0/8
                || v4.is_private()      // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local()   // 169.254.0.0/16 — cloud metadata
                || v4.is_multicast()    // 224/4
                || v4.is_broadcast()
                || v4.is_unspecified()  // 0.0.0.0
                || o[0] == 0            // 0.0.0.0/8
                // M134 review (F2): ranges a cloud/k8s deployment actually routes internally.
                || (o[0] == 100 && (64..128).contains(&o[1]))  // 100.64/10 carrier-grade NAT (k8s, ISPs)
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19)) // 198.18/15 benchmarking
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0/24 IETF protocol assignments
                || o[0] >= 240 // 240/4 reserved
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            v6.is_loopback()            // ::1
                || v6.is_unspecified()
                || v6.is_multicast()    // ff00::/8
                || (s[0] & 0xfe00) == 0xfc00  // fc00::/7 unique-local
                || (s[0] & 0xffc0) == 0xfe80  // fe80::/10 link-local
                || (s[0] & 0xffc0) == 0xfec0  // fec0::/10 site-local (deprecated, still routed internally)
                // M134 review (F2): v6 forms that EMBED a v4 address. On an IPv6-only cloud host with NAT64,
                // `64:ff9b::a9fe:a9fe` reaches 169.254.169.254 — judging it as "some public v6" would clear the
                // metadata service through the front door.
                || s[0] == 0x2002             // 2002::/16 6to4 (embeds v4 in s[1..3])
                || (s[0] == 0x2001 && s[1] == 0) // 2001:0::/32 Teredo
                || embedded_v4(v6).is_some_and(|m| is_blocked_addr(IpAddr::V4(m)))
        }
    }
}

/// The IPv4 address embedded in a v6 form, when there is one: v4-mapped (`::ffff:a.b.c.d`), v4-compatible
/// (`::a.b.c.d`), NAT64 (`64:ff9b::a.b.c.d`, RFC 6052) and 6to4 (`2002:a.b.c.d::`). Each must be judged by its
/// IPv4 meaning — that is the address the packet ends up at.
fn embedded_v4(v6: std::net::Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    let s = v6.segments();
    if let Some(m) = v6.to_ipv4_mapped() {
        return Some(m);
    }
    if s[0] == 0x64 && s[1] == 0xff9b {
        return Some(std::net::Ipv4Addr::from(((s[6] as u32) << 16) | s[7] as u32));
    }
    if s[0] == 0x2002 {
        return Some(std::net::Ipv4Addr::from(((s[1] as u32) << 16) | s[2] as u32));
    }
    // v4-compatible ::a.b.c.d (deprecated) — everything zero except the last two segments.
    if s[..6].iter().all(|&x| x == 0) && (s[6] != 0 || s[7] != 0) {
        return Some(std::net::Ipv4Addr::from(((s[6] as u32) << 16) | s[7] as u32));
    }
    None
}

/// Extract the host `minreq` will actually connect to — by MIRRORING its parser, not by parsing the URL
/// "correctly".
///
/// M134 review (BLOCKER F1): the two must agree or the guard is decorative. `minreq` 2.14 does NOT implement
/// userinfo (`HttpUrl::parse`, `http_url.rs:70-88`): it takes every char up to the first `:` / `/` / `?` as the
/// host, and when the port fails to parse it silently falls back to 80/443 (`http_url.rs:159-165`). So
/// `http://169.254.169.254:x@api.openai.com/v1` connects to **169.254.169.254:80** while an RFC-correct parser
/// reads the host as `api.openai.com`. A guard using the RFC reading would clear the metadata service.
///
/// Therefore: same char scan as minreq, then fail closed on any authority shape we cannot model (`@`, brackets,
/// non-graphic bytes). Rejecting brackets loses nothing — minreq cannot reach a bracketed IPv6 literal either
/// (it parses `[::1]:8080` to the host `"["`), so refusing is the honest semantics, not a regression.
pub(crate) fn endpoint_host(endpoint: &str) -> Option<String> {
    let rest = endpoint.strip_prefix("http://").or_else(|| endpoint.strip_prefix("https://"))?;
    let host = &rest[..rest.find([':', '/', '?']).unwrap_or(rest.len())];
    if host.is_empty()
        || host.bytes().any(|b| b == b'@' || b == b'[' || b == b']' || !b.is_ascii_graphic())
    {
        return None;
    }
    Some(host.to_string())
}

/// Parse the operator allowlist GUC (comma-separated hosts). Pure so it is testable without a GUC.
pub(crate) fn parse_allowlist(raw: &str) -> Vec<String> {
    raw.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The classifier is the load-bearing half: if a range is missing here, the guard silently permits it. Pure, so
    // every range is asserted without a socket.
    #[test]
    fn test_m134_classifier_blocks_all_private_ranges() {
        for s in [
            "127.0.0.1",              // loopback
            "10.0.0.1",               // RFC1918 /8
            "172.16.0.1",             // RFC1918 /12
            "192.168.0.1",            // RFC1918 /16
            "169.254.169.254",        // link-local — the cloud metadata service, the whole point
            "0.0.0.0",                // unspecified (aliases loopback on some stacks)
            "::1",                    // v6 loopback
            "fd00::1",                // v6 unique-local
            "fe80::1",                // v6 link-local
            "::ffff:169.254.169.254", // v4-mapped metadata — must not slip through as "a v6 address"
            // M134 review (F2) — v6 forms that EMBED a v4 address, plus internally-routed v4 ranges.
            "64:ff9b::a9fe:a9fe", // NAT64 (RFC 6052) → 169.254.169.254 on an IPv6-only cloud host
            "2002:a9fe:a9fe::",   // 6to4 → 169.254.169.254
            "2001:0:1:2::3",      // Teredo
            "::7f00:1",           // v4-compatible → 127.0.0.1
            "fec0::1",            // site-local
            "ff02::1",            // multicast
            "100.64.0.1",         // carrier-grade NAT (k8s / ISP internal)
            "224.0.0.1",          // multicast
            "198.18.0.1",         // benchmarking
            "240.0.0.1",          // reserved
        ] {
            let ip: std::net::IpAddr = s.parse().unwrap();
            assert!(is_blocked_addr(ip), "{s} must be blocked");
        }
        for s in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: std::net::IpAddr = s.parse().unwrap();
            assert!(!is_blocked_addr(ip), "{s} is public and must be allowed");
        }
    }

    // Host extraction must agree with `minreq`, NOT with the RFC. M134 review found a BLOCKER here: the first
    // version of this test asserted `http://user:pass@10.0.0.1:8080/x` → `10.0.0.1` (the RFC reading), while
    // minreq connects to `user`. The guard was checking a host the client never contacts — a full bypass, blessed
    // by a green test. These cases now encode minreq's actual parser.
    #[test]
    fn test_m134_endpoint_host_mirrors_the_http_client() {
        for (url, want) in [
            ("http://169.254.169.254/latest/meta-data/", Some("169.254.169.254")),
            ("https://api.openai.com/v1/embeddings", Some("api.openai.com")),
            ("http://127.0.0.1:1/dead", Some("127.0.0.1")),
            ("http://host?a=b", Some("host")),
            // `:` wins before `@`, so minreq's host here is literally `user` — an unresolvable string, which
            // fails closed at resolution. The RFC reading (`10.0.0.1`) is the one that would be WRONG.
            ("http://user:pass@10.0.0.1:8080/x", Some("user")),
            // No `:` before the `@`, so the whole thing lands in the host — a shape we refuse outright.
            ("http://user@10.0.0.1/x", None),
            ("http://[::1]:8080/v1", None), // minreq parses the host as "[" — unreachable either way
            ("file:///etc/passwd", None),
            ("http://", None),
            ("http://host\r\nX: y/v1", None),
        ] {
            assert_eq!(endpoint_host(url).as_deref(), want, "host of {url}");
        }
    }

    // The F1 exploit itself, as a regression test. `http://169.254.169.254:x@api.openai.com/v1` makes minreq
    // connect to 169.254.169.254:80 (unparseable port → silent fallback to 80) while an RFC-correct parser sees
    // the PUBLIC host and would wave it through. The guard must read the host minreq reads — the internal one —
    // and then block it.
    #[test]
    fn test_m134_userinfo_confusion_cannot_smuggle_an_internal_host() {
        for (url, smuggled) in [
            ("http://169.254.169.254:80@8.8.8.8/latest/meta-data/", "169.254.169.254"),
            ("http://169.254.169.254:x@api.openai.com/v1/chat/completions", "169.254.169.254"),
            ("http://10.0.0.5:1@1.1.1.1/x", "10.0.0.5"),
        ] {
            let host = endpoint_host(url).expect("minreq parses a host here, so we must too");
            assert_eq!(
                host, smuggled,
                "we must read the host minreq dials, not the RFC authority: {url}"
            );
            let ip: std::net::IpAddr = host.parse().expect("these payloads dial a literal");
            assert!(is_blocked_addr(ip), "{url} must be refused");
        }
    }

    // T2.3's second half, which the first cut asserted only in prose: the allowlist must be a SCOPED permit, not
    // a global off-switch. A sibling address in the same range as an allowlisted host stays blocked.
    #[test]
    fn test_m134_allowlist_is_scoped_not_a_global_off_switch() {
        let allow = parse_allowlist("127.0.0.1");
        assert!(allow.contains(&"127.0.0.1".to_string()), "the named host is permitted");
        assert!(
            !allow.contains(&"127.0.0.2".to_string()),
            "a sibling in the same /8 is NOT permitted"
        );
        // and the sibling is still classified internal, so the guard refuses it
        assert!(is_blocked_addr("127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn test_m134_allowlist_parsing_is_whitespace_and_case_insensitive() {
        assert_eq!(
            parse_allowlist(" 127.0.0.1 , Other.Example "),
            vec!["127.0.0.1", "other.example"]
        );
        assert!(parse_allowlist("").is_empty());
        assert!(parse_allowlist(" , , ").is_empty());
    }
}
