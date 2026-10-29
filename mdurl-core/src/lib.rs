//! Parse/format kernel ported from `executablebooks/mdurl` (`_parse.py`, `_format.py`).
//!
//! Faithful transcription of the Python control flow, operating on one URL per
//! call (whole-unit crossing, P1). All delimiter scans are byte-level over
//! ASCII delimiters, which are always UTF-8 boundaries; length checks that
//! Python does in code points use `.chars().count()`.

/// Python `str` whitespace for `str.strip()` / `re \s` on `str` patterns.
///
/// CPython treats U+001C..=U+001F as whitespace; Rust `char::is_whitespace`
/// (White_Space property) does not. Everything else in the two sets agrees.
#[inline]
fn py_is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn py_strip(s: &str) -> &str {
    let start = s.find(|c: char| !py_is_space(c)).unwrap_or(s.len());
    let end = s.rfind(|c: char| !py_is_space(c)).map(|i| {
        let c = s[i..].chars().next().unwrap();
        i + c.len_utf8()
    });
    match end {
        Some(e) if e > start => &s[start..e],
        _ => "",
    }
}

#[inline]
fn is_proto_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'.' || b == b'+' || b == b'-'
}

/// `PROTOCOL_PATTERN = ^([a-z0-9.+-]+:)` + IGNORECASE. Returns byte length of
/// the protocol prefix (including the colon), or `None`.
fn match_protocol(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && is_proto_char(bytes[i]) {
        i += 1;
    }
    if i > 0 && i < bytes.len() && bytes[i] == b':' {
        Some(i + 1)
    } else {
        None
    }
}

/// `AUTH_HOST_PATTERN = ^//[^@/]+@[^@/]+` (inline `re.search` in `parse`).
fn match_auth_host(rest: &str) -> bool {
    let b = rest.as_bytes();
    if b.len() < 4 || b[0] != b'/' || b[1] != b'/' {
        return false;
    }
    let mut i = 2;
    let mut seen_user = false;
    let mut seen_host = false;
    while i < b.len() {
        let c = b[i];
        if c == b'@' || c == b'/' {
            break;
        }
        seen_user = true;
        i += 1;
    }
    if i < b.len() && b[i] == b'@' {
        i += 1;
        while i < b.len() {
            let c = b[i];
            if c == b'@' || c == b'/' {
                break;
            }
            seen_host = true;
            i += 1;
        }
    }
    seen_user && seen_host
}

/// `SIMPLE_PATH_PATTERN = ^(//?(?!/)[^?\s]*)(\?[^\s]*)?$`.
/// NOTE: `//?` is a literal `/` plus an optional second `/` — a leading
/// slash is mandatory. Returns `(pathname, search)` (`search` includes the
/// leading `?`, or is empty).
fn match_simple_path(rest: &str) -> Option<(&str, &str)> {
    let b = rest.as_bytes();
    let i = if b.len() >= 2 && b[0] == b'/' && b[1] == b'/' {
        if b.len() >= 3 && b[2] == b'/' {
            return None;
        }
        2
    } else if !b.is_empty() && b[0] == b'/' {
        1
    } else {
        return None;
    };
    // `[^?\s]*` over chars from byte offset i
    let mut path_end = i;
    let mut saw_q = false;
    for (off, c) in rest[i..].char_indices() {
        if c == '?' {
            saw_q = true;
            path_end = i + off;
            break;
        }
        if py_is_space(c) {
            return None;
        }
        path_end = i + off + c.len_utf8();
    }
    if saw_q {
        // `(\?[^\s]*)?$`: search runs to end with no whitespace
        let mut search_end = path_end + 1;
        for (off, c) in rest[path_end + 1..].char_indices() {
            if py_is_space(c) {
                return None;
            }
            search_end = path_end + 1 + off + c.len_utf8();
        }
        Some((&rest[..path_end], &rest[path_end..search_end]))
    } else {
        Some((&rest[..path_end], ""))
    }
}

fn hostless_protocol(proto: &str) -> bool {
    matches!(proto, "javascript" | "javascript:")
}

fn slashed_protocol(proto: &str) -> bool {
    matches!(
        proto,
        "http" | "https" | "ftp" | "gopher" | "file" | "http:" | "https:" | "ftp:"
            | "gopher:" | "file:"
    )
}

#[inline]
fn is_hostname_class(c: char) -> bool {
    matches!(c, '+' | 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-')
}

const HOST_ENDING_CHARS: &[u8] = b"/?#";
const NON_HOST_CHARS: &[u8] = b"%/?;#'{}|\\^`<>\" \r\n\t";

/// First `PORT_PATTERN = :[0-9]*$` match scanning left to right, like
/// `re.search`. Returns byte offset of the colon.
fn match_port(host: &str) -> Option<usize> {
    let b = host.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b':' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == b.len() {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub protocol: Option<String>,
    pub slashes: bool,
    pub auth: Option<String>,
    pub port: Option<String>,
    pub hostname: Option<String>,
    pub hash: Option<String>,
    pub search: Option<String>,
    pub pathname: Option<String>,
}
impl ParsedUrl {
    fn empty() -> Self {
        ParsedUrl {
            protocol: None,
            slashes: false,
            auth: None,
            port: None,
            hostname: None,
            hash: None,
            search: None,
            pathname: None,
        }
    }
}

/// Transcription of `MutableURL.parse`.
pub fn parse(url: &str, slashes_denote_host: bool) -> ParsedUrl {
    let mut out = ParsedUrl::empty();
    let rest0 = py_strip(url);

    // Fast path for simple path URLs. NOTE: the `#` guard reads the
    // *unstripped* input, exactly like `len(url.split("#")) == 1`.
    if !slashes_denote_host && !url.contains('#') {
        if let Some((pathname, search)) = match_simple_path(rest0) {
            out.pathname = Some(pathname.to_string());
            if !search.is_empty() {
                out.search = Some(search.to_string());
            }
            return out;
        }
    }

    let mut proto = String::new();
    let mut lower_proto = String::new();
    let mut rest = rest0.to_string();
    if let Some(proto_len) = match_protocol(&rest) {
        proto = rest[..proto_len].to_string();
        lower_proto = proto.to_lowercase();
        out.protocol = Some(proto.clone());
        rest = rest[proto_len..].to_string();
    }

    let mut slashes = false;
    if slashes_denote_host || !proto.is_empty() || match_auth_host(&rest) {
        slashes = rest.starts_with("//");
        // `if slashes and not (proto and HOSTLESS_PROTOCOL[proto])`:
        // empty proto is falsy, so the strip happens unless a hostless
        // protocol prefix is present.
        if slashes && (proto.is_empty() || !hostless_protocol(&proto)) {
            rest = rest[2..].to_string();
            out.slashes = true;
        }
    }

    if !hostless_protocol(&proto)
        && (slashes || (!proto.is_empty() && !slashed_protocol(&proto)))
    {
        // First instance of any host-ending char (byte scan; ASCII delimiters).
        let host_end: Option<usize> = HOST_ENDING_CHARS
            .iter()
            .filter_map(|&h| rest.as_bytes().iter().position(|&b| b == h))
            .min();

        // `rest.rfind("@", 0, host_end + 1)`: search rest[..host_end+1].
        // host_end is a byte index of an ASCII byte, so host_end+1 is a
        // char boundary.
        let at_sign = match host_end {
            None => rest.rfind('@'),
            Some(e) => rest[..(e + 1).min(rest.len())].rfind('@'),
        };

        if let Some(at) = at_sign {
            out.auth = Some(rest[..at].to_string());
            rest = rest[at + 1..].to_string();
        }

        let host_end: Option<usize> = NON_HOST_CHARS
            .iter()
            .filter_map(|&h| rest.as_bytes().iter().position(|&b| b == h))
            .min();
        let mut host_end = host_end.unwrap_or(rest.len());
        if host_end > 0 && rest.as_bytes()[host_end - 1] == b':' {
            host_end -= 1;
        }
        let host = rest[..host_end].to_string();
        rest = rest[host_end..].to_string();

        // pull out port
        if let Some(colon) = match_port(&host) {
            let port = &host[colon..];
            if port != ":" {
                out.port = Some(port[1..].to_string());
            }
            if !host[..colon].is_empty() {
                out.hostname = Some(host[..colon].to_string());
            }
        } else if !host.is_empty() {
            out.hostname = Some(host.clone());
        }

        if out.hostname.is_none() {
            out.hostname = Some(String::new());
        }

        let ipv6 = out
            .hostname
            .as_deref()
            .map(|h| h.starts_with('[') && h.ends_with(']'))
            .unwrap_or(false);
        if !ipv6 {
            let hostname = out.hostname.clone().unwrap_or_default();
            let parts: Vec<&str> = hostname.split('.').collect();
            let l = parts.len();
            let mut i = 0;
            let mut broken: Option<(Vec<String>, Vec<String>)> = None;
            while i < l {
                let part = parts[i];
                if part.is_empty() {
                    i += 1;
                    continue;
                }
                if !(part.chars().count() <= 63 && part.chars().all(is_hostname_class)) {
                    let newpart: String = part
                        .chars()
                        .map(|c| if (c as u32) > 127 { 'x' } else { c })
                        .collect();
                    if !(newpart.chars().count() <= 63
                        && newpart.chars().all(is_hostname_class))
                    {
                        let mut valid_parts: Vec<String> =
                            parts[..i].iter().map(|s| s.to_string()).collect();
                        let mut not_host: Vec<String> =
                            parts[i + 1..].iter().map(|s| s.to_string()).collect();
                        // HOSTNAME_PART_START = ^([class]{0,63})(.*)$:
                        // g1 = class prefix (<=63 chars), g2 = remainder.
                        let mut g1_end = 0usize;
                        let mut n = 0usize;
                        for c in part.chars() {
                            if n < 63 && is_hostname_class(c) {
                                g1_end += c.len_utf8();
                                n += 1;
                            } else {
                                break;
                            }
                        }
                        valid_parts.push(part[..g1_end].to_string());
                        not_host.insert(0, part[g1_end..].to_string());
                        broken = Some((valid_parts, not_host));
                        break;
                    }
                }
                i += 1;
            }
            if let Some((valid_parts, not_host)) = broken {
                if !not_host.is_empty() {
                    rest = format!("{}{}", not_host.join("."), rest);
                }
                out.hostname = Some(valid_parts.join("."));
            }
        }

        if out.hostname.as_deref().map(|h| h.chars().count()).unwrap_or(0) > 255 {
            out.hostname = Some(String::new());
        }

        if ipv6 {
            let h = out.hostname.clone().unwrap_or_default();
            out.hostname = Some(h[1..h.len() - 1].to_string());
        }
    }

    if let Some(hash) = rest.find('#') {
        out.hash = Some(rest[hash..].to_string());
        rest = rest[..hash].to_string();
    }
    if let Some(qm) = rest.find('?') {
        out.search = Some(rest[qm..].to_string());
        rest = rest[..qm].to_string();
    }
    if !rest.is_empty() {
        out.pathname = Some(rest);
    }
    if slashed_protocol(&lower_proto)
        && out.hostname.as_deref().map(|h| !h.is_empty()).unwrap_or(false)
        && out.pathname.is_none()
    {
        out.pathname = Some(String::new());
    }

    out
}

/// Transcription of `_format.format`.
pub fn format_url(p: &ParsedUrl) -> String {
    let mut result = String::new();
    result.push_str(p.protocol.as_deref().unwrap_or(""));
    if p.slashes {
        result.push_str("//");
    }
    // `result += url.auth + "@" if url.auth else ""`: empty auth is falsy.
    if let Some(auth) = &p.auth {
        if !auth.is_empty() {
            result.push_str(auth);
            result.push('@');
        }
    }
    match &p.hostname {
        Some(h) if h.contains(':') => {
            result.push('[');
            result.push_str(h);
            result.push(']');
        }
        Some(h) => result.push_str(h),
        None => {}
    }
    if let Some(port) = &p.port {
        result.push(':');
        result.push_str(port);
    }
    result.push_str(p.pathname.as_deref().unwrap_or(""));
    result.push_str(p.search.as_deref().unwrap_or(""));
    result.push_str(p.hash.as_deref().unwrap_or(""));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ascii() {
        assert_eq!(py_strip("  http://foo.com  \n"), "http://foo.com");
    }

    #[test]
    fn simple_paths() {
        assert_eq!(
            match_simple_path("/a/b?x=1"),
            Some(("/a/b", "?x=1"))
        );
        assert_eq!(match_simple_path("//a/b"), Some(("//a/b", "")));
        assert!(match_simple_path("///a").is_none());
        assert!(match_simple_path("/a b").is_none());
    }

    #[test]
    fn protocols() {
        assert_eq!(match_protocol("https://x"), Some(6));
        assert_eq!(match_protocol("HTTP://x"), Some(5));
        assert_eq!(match_protocol("/rel/path"), None);
        assert!(match_auth_host("//u@h/"));
        assert!(!match_auth_host("/rel"));
    }
}
