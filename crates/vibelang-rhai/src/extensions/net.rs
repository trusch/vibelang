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
                result.push(b'%');
                result.extend(hex.as_bytes());
            }
            _ => result.extend(c.to_string().as_bytes()),
        }
    }

    String::from_utf8_lossy(&result).to_string()
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
    result.insert(
        "query".into(),
        Dynamic::from(query.unwrap_or("").to_string()),
    );

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

// ============================================================================
// JSON Utilities (Basic)
// ============================================================================

/// Parse a JSON string into a Rhai value.
///
/// This is a simple JSON parser for basic structures.
pub fn json_parse(json: &str) -> Result<Dynamic, Box<EvalAltResult>> {
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
    let value = json_parse(value.trim())?;

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
                    result.push(json_parse(elem.trim())?);
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
            result.push(json_parse(elem)?);
        }
    }

    Ok(Dynamic::from(result))
}

/// Convert a Rhai value to JSON string.
pub fn json_stringify(value: Dynamic) -> Result<String, Box<EvalAltResult>> {
    stringify_value(&value)
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
}
