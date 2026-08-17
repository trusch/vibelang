//! Networking extension for Rhai.
//!
//! Provides HTTP client functionality for VibeLang scripts.
//!
//! # Available Functions
//!
//! - `http_get(url)` - Perform HTTP GET request, return body as string
//! - `http_get_json(url)` - Perform HTTP GET and parse JSON response
//! - `http_post(url, body)` - Perform HTTP POST request
//! - `http_post_json(url, data)` - POST JSON data
//! - `url_encode(string)` - URL encode a string
//! - `url_decode(string)` - URL decode a string
//!
//! # Note
//!
//! This is a minimal HTTP client using blocking I/O.
//! For production use cases requiring advanced features (TLS, async, etc.),
//! consider using the full `ureq` or `reqwest` crates.
//!
//! # Security Warning
//!
//! This extension allows network access. Only enable it
//! in trusted environments.

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Register networking functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // HTTP methods
    engine.register_fn("http_get", http_get);
    engine.register_fn("http_get_lines", http_get_lines);
    engine.register_fn("http_get_json", http_get_json);
    engine.register_fn("http_post", http_post);
    engine.register_fn("http_post_json", http_post_json);

    // URL utilities
    engine.register_fn("url_encode", url_encode);
    engine.register_fn("url_decode", url_decode);
    engine.register_fn("parse_url", parse_url);
    engine.register_fn("build_query_string", build_query_string);

    // JSON utilities (basic)
    engine.register_fn("json_parse", json_parse);
    engine.register_fn("json_stringify", json_stringify);
}

fn boundary_error(
    code: &str,
    span: std::ops::Range<usize>,
    expected: &str,
    token: &str,
) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "{code} span={}..{} expected={expected} token={token:?}",
            span.start, span.end
        )
        .into(),
        rhai::Position::NONE,
    ))
}

/// Install the complete networking inventory with strict vibe-api 2 boundaries.
pub(crate) fn register_v2(engine: &mut Engine) {
    register(engine);
    engine
        .register_fn("http_get", http_get_strict)
        .register_fn("http_get_lines", http_get_lines_strict)
        .register_fn("http_post", http_post_strict)
        .register_fn("url_decode", url_decode_strict)
        .register_fn("parse_url", parse_url_strict)
        .register_fn("build_query_string", build_query_string_strict)
        .register_fn("json_parse", json_parse_strict)
        .register_fn("json_stringify", json_stringify_strict)
        .register_fn("http_get_json", |url: &str| {
            let body = http_get_strict(url)?;
            json_parse_strict(&body)
        })
        .register_fn("http_post_json", |url: &str, data: Map| {
            let json_body = json_stringify_strict(Dynamic::from(data))?;
            let response =
                http_request_strict("POST", url, Some((&json_body, "application/json")))?;
            json_parse_strict(&response)
        });
}

// ============================================================================
// HTTP Client
// ============================================================================

/// Parse a URL into components.
fn parse_url_components(url: &str) -> Result<(bool, String, u16, String), Box<EvalAltResult>> {
    let url = url.trim();

    // Check for HTTPS
    let (is_https, rest) = if let Some(stripped) = url.strip_prefix("https://") {
        (true, stripped)
    } else if let Some(stripped) = url.strip_prefix("http://") {
        (false, stripped)
    } else {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "URL must start with http:// or https://".to_string().into(),
            rhai::Position::NONE,
        )));
    };

    // Split host and path
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    // Split host and port
    let (host, port) = match host_port.find(':') {
        Some(i) => {
            let port_str = &host_port[i + 1..];
            let port = port_str.parse::<u16>().map_err(|_| {
                Box::new(EvalAltResult::ErrorRuntime(
                    format!("Invalid port: {}", port_str).into(),
                    rhai::Position::NONE,
                ))
            })?;
            (&host_port[..i], port)
        }
        None => (host_port, if is_https { 443 } else { 80 }),
    };

    Ok((is_https, host.to_string(), port, path.to_string()))
}

fn parse_url_components_strict(
    url: &str,
) -> Result<(bool, String, u16, String), Box<EvalAltResult>> {
    if url.trim() != url {
        return Err(boundary_error(
            "extension.net.url_whitespace",
            0..url.len(),
            "url_without_surrounding_whitespace",
            url,
        ));
    }
    if let Some((index, character)) = url.char_indices().find(|(_, character)| {
        !character.is_ascii() || character.is_whitespace() || character.is_control()
    }) {
        return Err(boundary_error(
            "extension.net.url_character",
            index..index + character.len_utf8(),
            "ascii_url_without_whitespace_or_control_characters",
            &url[index..index + character.len_utf8()],
        ));
    }
    if let Some(index) = url.find('#') {
        return Err(boundary_error(
            "extension.net.url_fragment",
            index..url.len(),
            "url_without_fragment",
            &url[index..],
        ));
    }
    let (is_https, rest, authority_start) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest, "https://".len())
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest, "http://".len())
    } else {
        let end = match url.find([':', '/', '?', '#']) {
            Some(end) => end,
            None => url.len(),
        };
        return Err(boundary_error(
            "extension.net.url_scheme",
            0..end,
            "http_or_https_scheme",
            &url[..end],
        ));
    };
    let authority_end = match rest.find(['/', '?', '#']) {
        Some(end) => end,
        None => rest.len(),
    };
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.chars().any(char::is_whitespace) {
        return Err(boundary_error(
            "extension.net.url_host",
            authority_start..authority_start + authority.len(),
            "non_empty_host_without_whitespace",
            authority,
        ));
    }
    if authority.contains('@') || authority.starts_with('[') {
        return Err(boundary_error(
            "extension.net.url_authority",
            authority_start..authority_start + authority.len(),
            "basic_host_with_optional_u16_port",
            authority,
        ));
    }
    let (host, port) = if let Some((host, port_text)) = authority.rsplit_once(':') {
        if host.is_empty() || port_text.is_empty() {
            return Err(boundary_error(
                "extension.net.url_port",
                authority_start + host.len()..authority_start + authority.len(),
                "u16_port",
                port_text,
            ));
        }
        let port_start = authority_start + host.len() + 1;
        let port = port_text.parse::<u16>().map_err(|_| {
            boundary_error(
                "extension.net.url_port",
                port_start..port_start + port_text.len(),
                "u16_port",
                port_text,
            )
        })?;
        if port == 0 {
            return Err(boundary_error(
                "extension.net.url_port",
                port_start..port_start + port_text.len(),
                "port_1..=65535",
                port_text,
            ));
        }
        (host, port)
    } else {
        (authority, if is_https { 443 } else { 80 })
    };
    if host.chars().any(|character| {
        !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '_')
    }) {
        return Err(boundary_error(
            "extension.net.url_host",
            authority_start..authority_start + host.len(),
            "ascii_host_name",
            host,
        ));
    }
    let suffix = &rest[authority_end..];
    let path = if suffix.is_empty() {
        "/".to_string()
    } else if suffix.starts_with('?') || suffix.starts_with('#') {
        format!("/{suffix}")
    } else {
        suffix.to_string()
    };
    Ok((is_https, host.to_string(), port, path))
}

fn http_get_strict(url: &str) -> Result<String, Box<EvalAltResult>> {
    http_request_strict("GET", url, None)
}

fn http_get_lines_strict(url: &str) -> Result<Array, Box<EvalAltResult>> {
    let body = http_get_strict(url)?;
    Ok(body
        .lines()
        .map(|line| Dynamic::from(line.to_string()))
        .collect())
}

fn http_post_strict(url: &str, body: &str) -> Result<String, Box<EvalAltResult>> {
    http_request_strict(
        "POST",
        url,
        Some((body, "application/x-www-form-urlencoded")),
    )
}

fn transport_error(
    code: &str,
    url: &str,
    expected: &str,
    error: impl std::fmt::Display,
) -> Box<EvalAltResult> {
    boundary_error(
        code,
        0..url.len(),
        &format!("{expected} error={error}"),
        url,
    )
}

fn http_request_strict(
    method: &str,
    url: &str,
    body: Option<(&str, &str)>,
) -> Result<String, Box<EvalAltResult>> {
    let (is_https, host, port, path) = parse_url_components_strict(url)?;
    if is_https {
        return Err(boundary_error(
            "extension.net.https_unsupported",
            0.."https".len(),
            "http_url_for_basic_transport",
            "https",
        ));
    }

    let address = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&address).map_err(|error| {
        transport_error(
            "extension.net.connect",
            url,
            "reachable_http_endpoint",
            error,
        )
    })?;
    let timeout = Some(Duration::from_secs(30));
    stream.set_read_timeout(timeout).map_err(|error| {
        transport_error(
            "extension.net.read_timeout",
            url,
            "configurable_read_timeout",
            error,
        )
    })?;
    stream.set_write_timeout(timeout).map_err(|error| {
        transport_error(
            "extension.net.write_timeout",
            url,
            "configurable_write_timeout",
            error,
        )
    })?;

    let default_port = 80;
    let host_header = if port == default_port {
        host.clone()
    } else {
        format!("{host}:{port}")
    };
    let request = if let Some((body, content_type)) = body {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nUser-Agent: vibelang/1.0\r\n\r\n{body}",
            body.len()
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nUser-Agent: vibelang/1.0\r\n\r\n"
        )
    };
    stream.write_all(request.as_bytes()).map_err(|error| {
        transport_error(
            "extension.net.write_request",
            url,
            "complete_http_request",
            error,
        )
    })?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    let status_bytes = reader.read_line(&mut status_line).map_err(|error| {
        transport_error(
            "extension.net.read_status",
            url,
            "complete_http_status_line",
            error,
        )
    })?;
    if status_bytes == 0 {
        return Err(boundary_error(
            "extension.net.response_eof",
            0..url.len(),
            "http_status_line",
            url,
        ));
    }
    let mut status_parts = status_line.split_whitespace();
    let version = status_parts.next().ok_or_else(|| {
        boundary_error(
            "extension.net.response_status",
            0..status_line.len(),
            "HTTP_version_and_three_digit_status",
            status_line.trim_end(),
        )
    })?;
    let status = status_parts.next().ok_or_else(|| {
        boundary_error(
            "extension.net.response_status",
            0..status_line.len(),
            "HTTP_version_and_three_digit_status",
            status_line.trim_end(),
        )
    })?;
    let status_code = status.parse::<u16>();
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || status.len() != 3
        || !matches!(status_code, Ok(100..=599))
    {
        return Err(boundary_error(
            "extension.net.response_status",
            0..status_line.len(),
            "HTTP_1.x_and_three_digit_status",
            status_line.trim_end(),
        ));
    }

    let mut content_length = None;
    let mut transfer_encoding = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).map_err(|error| {
            transport_error(
                "extension.net.read_header",
                url,
                "complete_http_headers",
                error,
            )
        })?;
        if bytes == 0 {
            return Err(boundary_error(
                "extension.net.response_eof",
                0..url.len(),
                "header_terminator",
                url,
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let (name, value) = line.split_once(':').ok_or_else(|| {
            boundary_error(
                "extension.net.response_header",
                0..line.len(),
                "header_name_colon_value",
                line,
            )
        })?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            let length = value.parse::<usize>().map_err(|_| {
                boundary_error(
                    "extension.net.content_length",
                    0..value.len(),
                    "usize_content_length",
                    value,
                )
            })?;
            if content_length.replace(length).is_some() {
                return Err(boundary_error(
                    "extension.net.content_length",
                    0..value.len(),
                    "single_content_length",
                    value,
                ));
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encoding = Some(value.to_owned());
        }
    }
    if let Some(encoding) = transfer_encoding {
        if !encoding.eq_ignore_ascii_case("identity") {
            return Err(boundary_error(
                "extension.net.transfer_encoding",
                0..encoding.len(),
                "identity_or_content_length_response",
                &encoding,
            ));
        }
    }

    let mut response = Vec::new();
    reader.read_to_end(&mut response).map_err(|error| {
        transport_error("extension.net.read_body", url, "complete_http_body", error)
    })?;
    if let Some(expected) = content_length {
        if response.len() != expected {
            return Err(boundary_error(
                "extension.net.body_length",
                0..url.len(),
                &format!("exact_content_length_{expected}"),
                url,
            ));
        }
    }
    String::from_utf8(response).map_err(|error| {
        transport_error("extension.net.body_encoding", url, "utf8_http_body", error)
    })
}

/// Perform an HTTP GET request.
///
/// Note: This is a simple implementation for HTTP only (no HTTPS).
/// For HTTPS support, enable the `ext-net-tls` feature.
pub fn http_get(url: &str) -> Result<String, Box<EvalAltResult>> {
    let (is_https, host, port, path) = parse_url_components(url)?;

    if is_https {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "HTTPS not supported in basic net extension. Use http:// or enable ext-net-tls"
                .to_string()
                .into(),
            rhai::Position::NONE,
        )));
    }

    // Connect
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to connect to {}: {}", addr, e).into(),
            rhai::Position::NONE,
        ))
    })?;

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    // Send request
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: vibelang/1.0\r\n\r\n",
        path, host
    );

    stream.write_all(request.as_bytes()).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to send request: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    // Read response
    let mut reader = BufReader::new(stream);

    // Read status line
    let mut status_line = String::new();
    reader.read_line(&mut status_line).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read response: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    // Read headers (skip them)
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to read headers: {}", e).into(),
                rhai::Position::NONE,
            ))
        })?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
    }

    // Read body
    let mut body = String::new();
    reader.read_to_string(&mut body).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read body: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    Ok(body)
}

/// Perform HTTP GET and return lines as array.
pub fn http_get_lines(url: &str) -> Result<Array, Box<EvalAltResult>> {
    let body = http_get(url)?;
    Ok(body
        .lines()
        .map(|line| Dynamic::from(line.to_string()))
        .collect())
}

/// Perform HTTP GET and parse JSON response.
pub fn http_get_json(url: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let body = http_get(url)?;
    json_parse(&body)
}

/// Perform an HTTP POST request.
pub fn http_post(url: &str, body: &str) -> Result<String, Box<EvalAltResult>> {
    http_post_with_content_type(url, body, "application/x-www-form-urlencoded")
}

/// Perform an HTTP POST request with JSON body.
pub fn http_post_json(url: &str, data: Map) -> Result<Dynamic, Box<EvalAltResult>> {
    let json_body = json_stringify_map(&data)?;
    let response = http_post_with_content_type(url, &json_body, "application/json")?;
    json_parse(&response)
}

fn http_post_with_content_type(
    url: &str,
    body: &str,
    content_type: &str,
) -> Result<String, Box<EvalAltResult>> {
    let (is_https, host, port, path) = parse_url_components(url)?;

    if is_https {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "HTTPS not supported in basic net extension"
                .to_string()
                .into(),
            rhai::Position::NONE,
        )));
    }

    // Connect
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to connect to {}: {}", addr, e).into(),
            rhai::Position::NONE,
        ))
    })?;

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    // Send request
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: {}\r\nContent-Length: {}\r\nUser-Agent: vibelang/1.0\r\n\r\n{}",
        path, host, content_type, body.len(), body
    );

    stream.write_all(request.as_bytes()).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to send request: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    // Read response
    let mut reader = BufReader::new(stream);

    // Read status line
    let mut status_line = String::new();
    reader.read_line(&mut status_line).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read response: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    // Read headers (skip them)
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to read headers: {}", e).into(),
                rhai::Position::NONE,
            ))
        })?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
    }

    // Read body
    let mut response_body = String::new();
    reader.read_to_string(&mut response_body).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to read body: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    Ok(response_body)
}

// ============================================================================
// URL Utilities
// ============================================================================

/// URL encode a string.
pub fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// URL decode a string.
pub fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();
    let mut recovered = false;

    while let Some(c) = chars.next() {
        match c {
            '+' => result.push(b' '),
            '%' => {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte);
                        continue;
                    }
                }
                recovered = true;
                result.push(b'%');
                result.extend(hex.as_bytes());
            }
            _ => result.extend(c.to_string().as_bytes()),
        }
    }

    let decoded = String::from_utf8_lossy(&result);
    if recovered || matches!(&decoded, std::borrow::Cow::Owned(_)) {
        log::warn!(
            "diagnostic.compat.parser_forgiving profile=compat.vibelang.v1 parser=url_decode input={s:?} recovery=legacy_escape_preservation effective_value={decoded:?} replacement=use_percent_followed_by_two_hex_digits"
        );
    }
    decoded.into_owned()
}

fn url_decode_strict(s: &str) -> Result<String, Box<EvalAltResult>> {
    fn hex_nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(boundary_error(
                        "extension.net.url_escape",
                        index..bytes.len(),
                        "percent_followed_by_two_hex_digits",
                        &s[index..],
                    ));
                }
                let high = hex_nibble(bytes[index + 1]);
                let low = hex_nibble(bytes[index + 2]);
                let byte = high
                    .zip(low)
                    .map(|(high, low)| high * 16 + low)
                    .ok_or_else(|| {
                        boundary_error(
                            "extension.net.url_escape",
                            index..index + 3,
                            "percent_followed_by_two_hex_digits",
                            &s[index..],
                        )
                    })?;
                decoded.push(byte);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|error| {
        boundary_error(
            "extension.net.url_utf8",
            error.utf8_error().valid_up_to()..s.len(),
            "valid_utf8_after_percent_decoding",
            s,
        )
    })
}

/// Parse a URL into its components.
pub fn parse_url(url: &str) -> Result<Map, Box<EvalAltResult>> {
    let (is_https, host, port, path) = parse_url_components(url)?;

    // Split path and query
    let (path_only, query) = match path.find('?') {
        Some(i) => (&path[..i], Some(&path[i + 1..])),
        None => (path.as_str(), None),
    };

    let mut result = Map::new();
    result.insert(
        "scheme".into(),
        Dynamic::from(if is_https { "https" } else { "http" }),
    );
    result.insert("host".into(), Dynamic::from(host));
    result.insert("port".into(), Dynamic::from(port as i64));
    result.insert("path".into(), Dynamic::from(path_only.to_string()));
    let query = match query {
        Some(query) => query,
        None => "",
    };
    result.insert("query".into(), Dynamic::from(query.to_string()));

    Ok(result)
}

fn parse_url_strict(url: &str) -> Result<Map, Box<EvalAltResult>> {
    let (is_https, host, port, path) = parse_url_components_strict(url)?;
    let (path_only, query) = match path.find('?') {
        Some(index) => (&path[..index], Some(&path[index + 1..])),
        None => (path.as_str(), None),
    };
    let mut result = Map::new();
    result.insert(
        "scheme".into(),
        Dynamic::from(if is_https { "https" } else { "http" }),
    );
    result.insert("host".into(), Dynamic::from(host));
    result.insert("port".into(), Dynamic::from(i64::from(port)));
    result.insert("path".into(), Dynamic::from(path_only.to_string()));
    let query = match query {
        Some(query) => query,
        None => "",
    };
    result.insert("query".into(), Dynamic::from(query.to_string()));
    Ok(result)
}

/// Build a query string from a map.
pub fn build_query_string(params: Map) -> String {
    params
        .into_iter()
        .map(|(k, v)| format!("{}={}", url_encode(k.as_ref()), url_encode(&v.to_string())))
        .collect::<Vec<_>>()
        .join("&")
}

fn build_query_string_strict(params: Map) -> Result<String, Box<EvalAltResult>> {
    params
        .into_iter()
        .map(|(key, value)| {
            let value = if value.is_string() {
                value.into_string().map_err(|value| {
                    boundary_error(
                        "extension.net.query_value",
                        0..key.len(),
                        "string_integer_finite_float_or_bool",
                        &value.to_string(),
                    )
                })?
            } else if value.is_int() {
                value
                    .as_int()
                    .map_err(|error| {
                        boundary_error(
                            "extension.net.query_value",
                            0..key.len(),
                            "integer",
                            &error.to_string(),
                        )
                    })?
                    .to_string()
            } else if value.is_float() {
                let float = value.as_float().map_err(|error| {
                    boundary_error(
                        "extension.net.query_value",
                        0..key.len(),
                        "finite_float",
                        &error.to_string(),
                    )
                })?;
                if !float.is_finite() {
                    return Err(boundary_error(
                        "extension.net.query_non_finite",
                        0..key.len(),
                        "finite_float",
                        &float.to_string(),
                    ));
                }
                float.to_string()
            } else if value.is_bool() {
                value
                    .as_bool()
                    .map_err(|error| {
                        boundary_error(
                            "extension.net.query_value",
                            0..key.len(),
                            "bool",
                            &error.to_string(),
                        )
                    })?
                    .to_string()
            } else {
                return Err(boundary_error(
                    "extension.net.query_value",
                    0..key.len(),
                    "string_integer_finite_float_or_bool",
                    &value.type_name(),
                ));
            };
            Ok(format!(
                "{}={}",
                url_encode(key.as_ref()),
                url_encode(&value)
            ))
        })
        .collect::<Result<Vec<_>, Box<EvalAltResult>>>()
        .map(|pairs| pairs.join("&"))
}

// ============================================================================
// JSON Utilities (Basic)
// ============================================================================

/// Parse a JSON string into a Rhai value.
///
/// This is a simple JSON parser for basic structures.
pub fn json_parse(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let canonical_rejection = serde_json::from_str::<serde_json::Value>(json).is_err();
    let result = json_parse_legacy(json);
    if canonical_rejection && result.is_ok() {
        log::warn!(
            "diagnostic.compat.parser_forgiving profile=compat.vibelang.v1 parser=json input={json:?} recovery=legacy_json_parser effective_value=parsed replacement=use_complete_valid_json"
        );
    }
    result
}

fn json_parse_legacy(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let json = json.trim();

    if json.starts_with('{') {
        parse_json_object(json)
    } else if json.starts_with('[') {
        parse_json_array(json)
    } else if json.starts_with('"') {
        Ok(Dynamic::from(parse_json_string(json)?))
    } else if json == "true" {
        Ok(Dynamic::from(true))
    } else if json == "false" {
        Ok(Dynamic::from(false))
    } else if json == "null" {
        Ok(Dynamic::UNIT)
    } else if let Ok(n) = json.parse::<i64>() {
        Ok(Dynamic::from(n))
    } else if let Ok(n) = json.parse::<f64>() {
        Ok(Dynamic::from(n))
    } else {
        Err(Box::new(EvalAltResult::ErrorRuntime(
            format!("Invalid JSON: {}", json).into(),
            rhai::Position::NONE,
        )))
    }
}

fn json_error_offset(json: &str, line: usize, column: usize) -> usize {
    let line_start = json
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let line_text = match json[line_start..].split('\n').next() {
        Some(line) => line,
        None => "",
    };
    line_start
        + line_text
            .char_indices()
            .nth(column.saturating_sub(1))
            .map_or(line_text.len(), |(index, _)| index)
}

fn serde_to_dynamic(value: serde_json::Value, json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    match value {
        serde_json::Value::Null => Ok(Dynamic::UNIT),
        serde_json::Value::Bool(value) => Ok(Dynamic::from(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Dynamic::from(value))
            } else if let Some(value) = value.as_u64() {
                i64::try_from(value).map(Dynamic::from).map_err(|_| {
                    boundary_error(
                        "extension.net.json.integer_range",
                        0..json.len(),
                        "i64",
                        json,
                    )
                })
            } else {
                value.as_f64().map(Dynamic::from).ok_or_else(|| {
                    boundary_error(
                        "extension.net.json.number",
                        0..json.len(),
                        "finite_json_number",
                        json,
                    )
                })
            }
        }
        serde_json::Value::String(value) => Ok(Dynamic::from(value)),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|value| serde_to_dynamic(value, json))
            .collect::<Result<Array, _>>()
            .map(Dynamic::from),
        serde_json::Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| Ok((key.into(), serde_to_dynamic(value, json)?)))
            .collect::<Result<Map, Box<EvalAltResult>>>()
            .map(Dynamic::from),
    }
}

fn json_parse_strict(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let value = serde_json::from_str::<serde_json::Value>(json).map_err(|error| {
        let start = json_error_offset(json, error.line(), error.column());
        let end = if start < json.len() { start + 1 } else { start };
        boundary_error(
            "extension.net.json.invalid",
            start..end,
            "complete_valid_json",
            &json[start..],
        )
    })?;
    serde_to_dynamic(value, json)
}

fn parse_json_string(s: &str) -> Result<String, Box<EvalAltResult>> {
    if !s.starts_with('"') || !s.ends_with('"') {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "Invalid JSON string".to_string().into(),
            rhai::Position::NONE,
        )));
    }

    let inner = &s[1..s.len() - 1];
    let mut result = String::new();
    let mut chars = inner.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('/') => result.push('/'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                        }
                    }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

fn parse_json_object(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    // This is a simplified parser - for production, use serde_json
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "Invalid JSON object".to_string().into(),
            rhai::Position::NONE,
        )));
    }

    let inner = json[1..json.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Dynamic::from(Map::new()));
    }

    let mut result = Map::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut start = 0;
    let chars: Vec<char> = inner.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if escape {
            escape = false;
            i += 1;
            continue;
        }

        if c == '\\' {
            escape = true;
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = !in_string;
        } else if !in_string {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    let pair: String = chars[start..i].iter().collect();
                    parse_json_pair(pair.trim(), &mut result)?;
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }

    // Last pair
    if start < chars.len() {
        let pair: String = chars[start..].iter().collect();
        parse_json_pair(pair.trim(), &mut result)?;
    }

    Ok(Dynamic::from(result))
}

fn parse_json_pair(pair: &str, result: &mut Map) -> Result<(), Box<EvalAltResult>> {
    let pair = pair.trim();
    if pair.is_empty() {
        return Ok(());
    }

    // Find the colon separator (outside of strings)
    let mut in_string = false;
    let mut escape = false;
    let chars: Vec<char> = pair.chars().collect();
    let mut colon_pos = None;

    for (i, &c) in chars.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
        } else if c == ':' && !in_string {
            colon_pos = Some(i);
            break;
        }
    }

    let colon_pos = colon_pos.ok_or_else(|| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Invalid JSON key-value pair: {}", pair).into(),
            rhai::Position::NONE,
        ))
    })?;

    let key: String = chars[..colon_pos].iter().collect();
    let value: String = chars[colon_pos + 1..].iter().collect();

    let key = parse_json_string(key.trim())?;
    let value = json_parse_legacy(value.trim())?;

    result.insert(key.into(), value);
    Ok(())
}

fn parse_json_array(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Box::new(EvalAltResult::ErrorRuntime(
            "Invalid JSON array".to_string().into(),
            rhai::Position::NONE,
        )));
    }

    let inner = json[1..json.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Dynamic::from(Array::new()));
    }

    let mut result = Array::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut start = 0;
    let chars: Vec<char> = inner.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if escape {
            escape = false;
            i += 1;
            continue;
        }

        if c == '\\' {
            escape = true;
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = !in_string;
        } else if !in_string {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    let elem: String = chars[start..i].iter().collect();
                    result.push(json_parse_legacy(elem.trim())?);
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }

    // Last element
    if start < chars.len() {
        let elem: String = chars[start..].iter().collect();
        let elem = elem.trim();
        if !elem.is_empty() {
            result.push(json_parse_legacy(elem)?);
        }
    }

    Ok(Dynamic::from(result))
}

/// Convert a Rhai value to JSON string.
pub fn json_stringify(value: Dynamic) -> Result<String, Box<EvalAltResult>> {
    let strict_rejection = dynamic_to_serde(&value, "$").is_err();
    let result = stringify_value(&value);
    if strict_rejection && result.is_ok() {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=json_stringify argument=value input_type={} recovery=legacy_non_json_representation effective_value=serialized replacement=use_finite_json_value",
            value.type_name()
        );
    }
    result
}

fn dynamic_to_serde(value: &Dynamic, path: &str) -> Result<serde_json::Value, Box<EvalAltResult>> {
    if value.is_unit() {
        Ok(serde_json::Value::Null)
    } else if let Some(value) = value.clone().try_cast::<bool>() {
        Ok(serde_json::Value::Bool(value))
    } else if let Some(value) = value.clone().try_cast::<i64>() {
        Ok(serde_json::Value::Number(value.into()))
    } else if let Some(value) = value.clone().try_cast::<f64>() {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                boundary_error(
                    "extension.net.json.non_finite",
                    0..path.len(),
                    "finite_number",
                    path,
                )
            })
    } else if let Some(value) = value.clone().try_cast::<String>() {
        Ok(serde_json::Value::String(value))
    } else if let Some(values) = value.clone().try_cast::<Array>() {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| dynamic_to_serde(value, &format!("{path}[{index}]")))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array)
    } else if let Some(values) = value.clone().try_cast::<Map>() {
        values
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.to_string(),
                    dynamic_to_serde(value, &format!("{path}.{key}"))?,
                ))
            })
            .collect::<Result<serde_json::Map<_, _>, Box<EvalAltResult>>>()
            .map(serde_json::Value::Object)
    } else {
        Err(boundary_error(
            "extension.net.json.unsupported_type",
            0..path.len(),
            "json_value",
            path,
        ))
    }
}

fn json_stringify_strict(value: Dynamic) -> Result<String, Box<EvalAltResult>> {
    serde_json::to_string(&dynamic_to_serde(&value, "$")?).map_err(|error| {
        boundary_error(
            "extension.net.json.serialize",
            0..1,
            "serializable_json_value",
            &error.to_string(),
        )
    })
}

fn json_stringify_map(map: &Map) -> Result<String, Box<EvalAltResult>> {
    let mut result = String::from("{");
    let mut first = true;

    for (key, value) in map {
        if !first {
            result.push(',');
        }
        first = false;

        result.push('"');
        result.push_str(&escape_json_string(key.as_ref()));
        result.push_str("\":");
        result.push_str(&stringify_value(value)?);
    }

    result.push('}');
    Ok(result)
}

fn stringify_value(value: &Dynamic) -> Result<String, Box<EvalAltResult>> {
    if value.is_unit() {
        Ok("null".to_string())
    } else if value.is_bool() {
        Ok(value.as_bool().unwrap().to_string())
    } else if value.is_int() {
        Ok(value.as_int().unwrap().to_string())
    } else if value.is_float() {
        Ok(value.as_float().unwrap().to_string())
    } else if value.is_string() {
        let s = value.clone().into_string().unwrap();
        Ok(format!("\"{}\"", escape_json_string(&s)))
    } else if value.is_array() {
        let arr: Array = value.clone().cast();
        let elements: Result<Vec<String>, _> = arr.iter().map(stringify_value).collect();
        Ok(format!("[{}]", elements?.join(",")))
    } else if value.is_map() {
        let map: Map = value.clone().cast();
        json_stringify_map(&map)
    } else {
        Ok(format!("\"{}\"", escape_json_string(&value.to_string())))
    }
}

fn escape_json_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strict_local_response(response: &'static [u8]) -> Result<String, Box<EvalAltResult>> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream.write_all(response).unwrap();
        });
        let result = http_get_strict(&format!("http://{address}/"));
        server.join().unwrap();
        result
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("a=b&c=d"), "a%3Db%26c%3Dd");
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("a%3Db%26c%3Dd"), "a=b&c=d");
    }

    #[test]
    fn test_json_parse_primitives() {
        assert_eq!(json_parse("42").unwrap().as_int().unwrap(), 42);
        assert_eq!(json_parse("3.14").unwrap().as_float().unwrap(), 3.14);
        assert!(json_parse("true").unwrap().as_bool().unwrap());
        assert!(!json_parse("false").unwrap().as_bool().unwrap());
        assert!(json_parse("null").unwrap().is_unit());
        assert_eq!(
            json_parse("\"hello\"").unwrap().into_string().unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_json_parse_array() {
        let arr: Array = json_parse("[1, 2, 3]").unwrap().cast();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_int().unwrap(), 1);
    }

    #[test]
    fn test_json_parse_object() {
        let obj: Map = json_parse("{\"name\": \"test\", \"value\": 42}")
            .unwrap()
            .cast();
        assert_eq!(
            obj.get("name").unwrap().clone().into_string().unwrap(),
            "test"
        );
        assert_eq!(obj.get("value").unwrap().as_int().unwrap(), 42);
    }

    #[test]
    fn test_json_stringify() {
        assert_eq!(json_stringify(Dynamic::from(42_i64)).unwrap(), "42");
        assert_eq!(json_stringify(Dynamic::from("hello")).unwrap(), "\"hello\"");
        assert_eq!(json_stringify(Dynamic::from(true)).unwrap(), "true");
    }

    #[test]
    fn v2_json_and_url_parsers_consume_complete_input() {
        assert_eq!(url_decode_strict("a%3Db").unwrap(), "a=b");
        let parsed = parse_url_strict("http://example.test:8080/path?q=1").unwrap();
        assert_eq!(
            parsed["host"].clone().into_string().unwrap(),
            "example.test"
        );
        assert_eq!(parsed["port"].as_int().unwrap(), 8080);
        assert_eq!(
            json_parse_strict(r#"{"nested":[1,true,null]}"#)
                .unwrap()
                .type_name(),
            "map"
        );

        for (error, diagnostic) in [
            (
                url_decode_strict("%G0").unwrap_err(),
                "extension.net.url_escape",
            ),
            (
                json_parse_strict(r#"{"ok": true} trailing"#).unwrap_err(),
                "extension.net.json.invalid",
            ),
            (
                json_parse_strict("NaN").unwrap_err(),
                "extension.net.json.invalid",
            ),
            (
                json_stringify_strict(Dynamic::from(f64::NAN)).unwrap_err(),
                "extension.net.json.non_finite",
            ),
            (
                parse_url_strict("ftp://example.test").unwrap_err(),
                "extension.net.url_scheme",
            ),
            (
                parse_url_strict("http://example.test:80x/").unwrap_err(),
                "extension.net.url_port",
            ),
            (
                parse_url_strict("http:///missing").unwrap_err(),
                "extension.net.url_host",
            ),
            (
                parse_url_strict("http://example.test/a b").unwrap_err(),
                "extension.net.url_character",
            ),
            (
                parse_url_strict("http://example.test/path#fragment").unwrap_err(),
                "extension.net.url_fragment",
            ),
            (
                http_get_strict("https://example.test").unwrap_err(),
                "extension.net.https_unsupported",
            ),
        ] {
            assert!(error.to_string().contains(diagnostic), "{error}");
        }

        let mut query = Map::new();
        query.insert("value".into(), Dynamic::from(f64::NAN));
        let error = build_query_string_strict(query).unwrap_err().to_string();
        assert!(error.contains("extension.net.query_non_finite"), "{error}");
    }

    #[test]
    fn v2_http_transport_rejects_partial_bodies() {
        assert_eq!(
            strict_local_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap(),
            "ok"
        );
        let error = strict_local_response(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nok")
            .unwrap_err()
            .to_string();
        assert!(error.contains("extension.net.body_length"), "{error}");
    }
}
