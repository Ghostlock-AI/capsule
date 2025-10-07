//! Utility functions for parsing strace output
//!
//! This module provides reusable parsing functions for common patterns in strace output,
//! including flags, strings, structs, arrays, and numeric values.

use crate::parse_syscall::types::{FileDescriptor, NumericValue, SockAddr};
use lazy_static::lazy_static;
use nom::{
    bytes::complete::{escaped, is_not, tag, take_until, take_while1},
    character::complete::{char, multispace0},
    combinator::{map, opt},
    multi::separated_list0,
    sequence::{delimited, preceded, separated_pair, terminated},
    IResult,
};
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during parsing
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("nom parsing failed: {0}")]
    NomError(String),

    #[error("Invalid numeric value: {0}")]
    InvalidNumber(String),

    #[error("Regex match failed: {0}")]
    RegexError(String),
}

// ============================================================================
// CONSTANT TABLES
// ============================================================================

lazy_static! {
    /// Table of known syscall constants and their values
    static ref CONSTANTS: HashMap<&'static str, i64> = {
        let mut m = HashMap::new();
        // File constants
        m.insert("AT_FDCWD", -100);
        m.insert("O_RDONLY", 0);
        m.insert("O_WRONLY", 1);
        m.insert("O_RDWR", 2);
        m.insert("O_CREAT", 64);
        m.insert("O_EXCL", 128);
        m.insert("O_NOCTTY", 256);
        m.insert("O_TRUNC", 512);
        m.insert("O_APPEND", 1024);
        m.insert("O_NONBLOCK", 2048);
        m.insert("O_CLOEXEC", 524288);

        // Network constants
        m.insert("AF_INET", 2);
        m.insert("AF_INET6", 10);
        m.insert("AF_UNIX", 1);
        m.insert("SOCK_STREAM", 1);
        m.insert("SOCK_DGRAM", 2);
        m.insert("SOCK_RAW", 3);
        m.insert("SOCK_CLOEXEC", 524288);
        m.insert("SOCK_NONBLOCK", 2048);
        m.insert("IPPROTO_IP", 0);
        m.insert("IPPROTO_TCP", 6);
        m.insert("IPPROTO_UDP", 17);

        // Process constants
        m.insert("CLONE_CHILD_CLEARTID", 0x00200000);
        m.insert("CLONE_CHILD_SETTID", 0x01000000);
        m.insert("SIGCHLD", 17);

        m
    };

    /// Regex for parsing file descriptors
    static ref FD_REGEX: Regex = Regex::new(r"^(\d+)(?:<(.+?)>)?$").unwrap();
}

// ============================================================================
// FLAG PARSING
// ============================================================================

/// Parse bitwise OR'ed flags
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_flags;
///
/// let flags = parse_flags("O_RDONLY|O_CLOEXEC");
/// assert_eq!(flags, vec!["O_RDONLY", "O_CLOEXEC"]);
///
/// let no_flags = parse_flags("0");
/// assert_eq!(no_flags, Vec::<String>::new());
/// ```
pub fn parse_flags(input: &str) -> Vec<String> {
    let trimmed = input.trim();

    // Handle special cases
    if trimmed == "0" || trimmed.is_empty() {
        return vec![];
    }

    // Split by | and trim each part
    trimmed
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ============================================================================
// STRING PARSING
// ============================================================================

/// Parse an escaped, quoted string using nom
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_escaped_string;
///
/// let (_, s) = parse_escaped_string("\"/etc/hosts\"").unwrap();
/// assert_eq!(s, "/etc/hosts");
///
/// let (_, s) = parse_escaped_string("\"/path/with\\\"quotes\"").unwrap();
/// assert_eq!(s, "/path/with\"quotes");
/// ```
pub fn parse_escaped_string(input: &str) -> IResult<&str, String> {
    delimited(
        char('"'),
        map(
            escaped(is_not("\\\""), '\\', char('"')),
            |s: &str| s.to_string(),
        ),
        char('"'),
    )(input)
}

/// Parse a string argument (wrapper that handles errors)
pub fn parse_string(input: &str) -> Result<String, ParseError> {
    let trimmed = input.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        // Simple approach: just strip quotes, keep everything inside as-is
        Ok(trimmed[1..trimmed.len()-1].to_string())
    } else {
        Err(ParseError::InvalidFormat(format!("Expected quoted string, got: {}", input)))
    }
}

// ============================================================================
// ARRAY PARSING
// ============================================================================

/// Parse an array of strings using nom
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_array;
///
/// let (_, arr) = parse_array_nom(r#"["python3", "script.py"]"#).unwrap();
/// assert_eq!(arr, vec!["python3", "script.py"]);
/// ```
pub fn parse_array_nom(input: &str) -> IResult<&str, Vec<String>> {
    delimited(
        char('['),
        separated_list0(
            delimited(multispace0, char(','), multispace0),
            parse_escaped_string,
        ),
        char(']'),
    )(input)
}

/// Parse an array of strings (wrapper that handles errors)
pub fn parse_array(input: &str) -> Result<Vec<String>, ParseError> {
    parse_array_nom(input.trim())
        .map(|(_, arr)| arr)
        .map_err(|e| ParseError::NomError(format!("Failed to parse array: {}", e)))
}

// ============================================================================
// STRUCT PARSING
// ============================================================================

/// Helper function to take characters until comma or closing brace, handling nesting
fn take_until_comma_or_brace(input: &str) -> IResult<&str, &str> {
    let mut depth = 0;
    let mut idx = 0;

    for (i, ch) in input.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth -= 1,
            '}' if depth == 0 => return Ok((&input[i..], &input[..i])),
            ',' if depth == 0 => return Ok((&input[i..], &input[..i])),
            _ => {}
        }
        idx = i + ch.len_utf8();
    }

    Ok(("", &input[..idx]))
}

/// Parse a single key=value pair from a struct
fn parse_struct_pair(input: &str) -> IResult<&str, (&str, &str)> {
    separated_pair(take_until("="), char('='), take_until_comma_or_brace)(input)
}

/// Parse the contents of a struct (comma-separated key=value pairs)
fn parse_struct_content(input: &str) -> IResult<&str, Vec<(&str, &str)>> {
    separated_list0(
        delimited(multispace0, char(','), multispace0),
        parse_struct_pair,
    )(input)
}

/// Parse a struct into a HashMap using nom
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_struct_nom;
///
/// let input = "{sa_family=AF_INET, sin_port=htons(9999)}";
/// let (_, map) = parse_struct_nom(input).unwrap();
/// assert_eq!(map.get("sa_family"), Some(&"AF_INET".to_string()));
/// ```
pub fn parse_struct_nom(input: &str) -> IResult<&str, HashMap<String, String>> {
    let (input, pairs) = delimited(char('{'), parse_struct_content, char('}'))(input)?;

    let map = pairs
        .into_iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();

    Ok((input, map))
}

/// Parse a struct (wrapper that handles errors)
pub fn parse_struct(input: &str) -> Result<HashMap<String, String>, ParseError> {
    parse_struct_nom(input.trim())
        .map(|(_, map)| map)
        .map_err(|e| ParseError::NomError(format!("Failed to parse struct: {}", e)))
}

// ============================================================================
// FILE DESCRIPTOR PARSING
// ============================================================================

/// Parse a file descriptor with optional annotation
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_fd;
///
/// let fd = parse_fd("3</etc/hosts>").unwrap();
/// assert_eq!(fd.fd, 3);
/// assert_eq!(fd.annotation, Some("/etc/hosts".to_string()));
///
/// let fd = parse_fd("3<TCP:[127.0.0.1:9999]>").unwrap();
/// assert_eq!(fd.fd, 3);
/// assert_eq!(fd.annotation, Some("TCP:[127.0.0.1:9999]".to_string()));
///
/// let fd = parse_fd("3").unwrap();
/// assert_eq!(fd.fd, 3);
/// assert_eq!(fd.annotation, None);
/// ```
pub fn parse_fd(input: &str) -> Result<FileDescriptor, ParseError> {
    let caps = FD_REGEX
        .captures(input.trim())
        .ok_or_else(|| ParseError::RegexError(format!("Invalid FD format: {}", input)))?;

    let fd = caps[1]
        .parse::<u32>()
        .map_err(|e| ParseError::InvalidNumber(format!("Invalid FD number: {}", e)))?;

    let annotation = caps.get(2).map(|m| m.as_str().to_string());

    Ok(FileDescriptor { fd, annotation })
}

// ============================================================================
// NUMERIC PARSING
// ============================================================================

/// Parse a numeric value (constant or literal)
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_numeric;
/// use parse_syscall::types::NumericValue;
///
/// let val = parse_numeric("AT_FDCWD").unwrap();
/// assert!(matches!(val, NumericValue::Constant { .. }));
///
/// let val = parse_numeric("123").unwrap();
/// assert_eq!(val, NumericValue::Literal(123));
///
/// let val = parse_numeric("0x7b").unwrap();
/// assert_eq!(val, NumericValue::Literal(123));
///
/// let val = parse_numeric("0755").unwrap();
/// assert_eq!(val, NumericValue::Literal(493));
/// ```
pub fn parse_numeric(input: &str) -> Result<NumericValue, ParseError> {
    let trimmed = input.trim();

    // Check if it's a known constant
    if let Some(&value) = CONSTANTS.get(trimmed) {
        return Ok(NumericValue::Constant {
            name: trimmed.to_string(),
            value: Some(value),
        });
    }

    // Check if it looks like a constant (starts with letter, all caps/underscores/digits)
    // Constants must start with a letter to avoid matching pure numbers
    if !trimmed.is_empty()
        && trimmed.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
        && trimmed
            .chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
    {
        return Ok(NumericValue::Constant {
            name: trimmed.to_string(),
            value: None, // Unknown constant
        });
    }

    // Try parsing as hex
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        let num = i64::from_str_radix(&trimmed[2..], 16)
            .map_err(|e| ParseError::InvalidNumber(format!("Invalid hex: {}", e)))?;
        return Ok(NumericValue::Literal(num));
    }

    // Try parsing as octal (leading 0, but not 0x)
    if trimmed.starts_with('0') && trimmed.len() > 1 && trimmed[1..].chars().all(|c| c.is_digit(8))
    {
        let num = i64::from_str_radix(&trimmed[1..], 8)
            .map_err(|e| ParseError::InvalidNumber(format!("Invalid octal: {}", e)))?;
        return Ok(NumericValue::Literal(num));
    }

    // Parse as decimal (including negative)
    let num = trimmed
        .parse::<i64>()
        .map_err(|e| ParseError::InvalidNumber(format!("Invalid decimal: {}", e)))?;
    Ok(NumericValue::Literal(num))
}

// ============================================================================
// SOCKET ADDRESS PARSING
// ============================================================================

/// Parse a socket address struct into a SockAddr enum
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_sockaddr;
///
/// let input = "{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr(\"127.0.0.1\")}";
/// let addr = parse_sockaddr(input).unwrap();
/// // addr is SockAddr::Inet { ip: "127.0.0.1", port: 9999 }
/// ```
pub fn parse_sockaddr(input: &str) -> Result<SockAddr, ParseError> {
    let struct_map = parse_struct(input)?;

    // Get the address family
    let family = struct_map
        .get("sa_family")
        .ok_or_else(|| ParseError::MissingField("sa_family".to_string()))?;

    match family.as_str() {
        "AF_INET" => {
            // Extract IP from inet_addr("...")
            let sin_addr = struct_map
                .get("sin_addr")
                .ok_or_else(|| ParseError::MissingField("sin_addr".to_string()))?;

            let ip = extract_inet_addr(sin_addr)?;

            // Extract port from htons(...)
            let sin_port = struct_map
                .get("sin_port")
                .ok_or_else(|| ParseError::MissingField("sin_port".to_string()))?;

            let port = extract_htons(sin_port)?;

            Ok(SockAddr::Inet { ip, port })
        }
        "AF_INET6" => {
            // TODO: Implement IPv6 parsing
            Ok(SockAddr::Unknown {
                family_name: "AF_INET6".to_string(),
                raw: input.to_string(),
            })
        }
        "AF_UNIX" => {
            // TODO: Implement Unix socket parsing
            Ok(SockAddr::Unknown {
                family_name: "AF_UNIX".to_string(),
                raw: input.to_string(),
            })
        }
        _ => Ok(SockAddr::Unknown {
            family_name: family.to_string(),
            raw: input.to_string(),
        }),
    }
}

/// Extract IP address from inet_addr("...") format
fn extract_inet_addr(input: &str) -> Result<String, ParseError> {
    lazy_static! {
        static ref INET_REGEX: Regex = Regex::new(r#"inet_addr\("([^"]+)"\)"#).unwrap();
    }

    INET_REGEX
        .captures(input)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ParseError::InvalidFormat(format!("Invalid inet_addr format: {}", input)))
}

/// Extract port number from htons(...) format
fn extract_htons(input: &str) -> Result<u16, ParseError> {
    lazy_static! {
        static ref HTONS_REGEX: Regex = Regex::new(r"htons\((\d+)\)").unwrap();
    }

    HTONS_REGEX
        .captures(input)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok())
        .ok_or_else(|| ParseError::InvalidFormat(format!("Invalid htons format: {}", input)))
}

// ============================================================================
// RETURN VALUE PARSING
// ============================================================================

/// Parse a return value that may contain a file descriptor with annotation
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_return_fd;
///
/// let (fd, path) = parse_return_fd("3</etc/hosts>").unwrap();
/// assert_eq!(fd, 3);
/// assert_eq!(path, Some("/etc/hosts".to_string()));
///
/// let (fd, path) = parse_return_fd("0").unwrap();
/// assert_eq!(fd, 0);
/// assert_eq!(path, None);
/// ```
pub fn parse_return_fd(input: &str) -> Result<(u32, Option<String>), ParseError> {
    let fd = parse_fd(input)?;
    Ok((fd.fd, fd.annotation))
}

/// Parse a return code (handles errors like "-1 ENOENT")
///
/// # Examples
///
/// ```
/// use parse_syscall::utils::parse_return_code;
///
/// assert_eq!(parse_return_code("0").unwrap(), 0);
/// assert_eq!(parse_return_code("-1 ENOENT (No such file)").unwrap(), -1);
/// ```
pub fn parse_return_code(input: &str) -> Result<i32, ParseError> {
    let trimmed = input.trim();

    // Try to extract just the number part (before any error message)
    let num_part = trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed);

    num_part
        .parse::<i32>()
        .map_err(|e| ParseError::InvalidNumber(format!("Invalid return code: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== FLAG TESTS ==========

    #[test]
    fn test_parse_flags() {
        assert_eq!(
            parse_flags("O_RDONLY|O_CLOEXEC"),
            vec!["O_RDONLY", "O_CLOEXEC"]
        );
        assert_eq!(parse_flags("O_RDONLY"), vec!["O_RDONLY"]);
        assert_eq!(parse_flags("0"), Vec::<String>::new());
        assert_eq!(parse_flags(""), Vec::<String>::new());
        assert_eq!(
            parse_flags("  O_RDONLY | O_WRONLY  "),
            vec!["O_RDONLY", "O_WRONLY"]
        );
    }

    // ========== STRING TESTS ==========

    #[test]
    fn test_parse_escaped_string() {
        let (_, s) = parse_escaped_string("\"/etc/hosts\"").unwrap();
        assert_eq!(s, "/etc/hosts");

        let (_, s) = parse_escaped_string("\"/path/with spaces\"").unwrap();
        assert_eq!(s, "/path/with spaces");
    }

    #[test]
    fn test_parse_string() {
        assert_eq!(parse_string("\"/etc/hosts\"").unwrap(), "/etc/hosts");
        assert_eq!(
            parse_string("  \"/etc/hosts\"  ").unwrap(),
            "/etc/hosts"
        );
    }

    // ========== ARRAY TESTS ==========

    #[test]
    fn test_parse_array() {
        let arr = parse_array(r#"["python3", "script.py", "arg1"]"#).unwrap();
        assert_eq!(arr, vec!["python3", "script.py", "arg1"]);

        let arr = parse_array(r#"[]"#).unwrap();
        assert_eq!(arr, Vec::<String>::new());

        let arr = parse_array(r#"["single"]"#).unwrap();
        assert_eq!(arr, vec!["single"]);
    }

    // ========== STRUCT TESTS ==========

    #[test]
    fn test_parse_struct() {
        let input = "{sa_family=AF_INET, sin_port=htons(9999)}";
        let map = parse_struct(input).unwrap();
        assert_eq!(map.get("sa_family"), Some(&"AF_INET".to_string()));
        assert_eq!(map.get("sin_port"), Some(&"htons(9999)".to_string()));
    }

    #[test]
    fn test_parse_struct_with_nested_parens() {
        let input = "{st_dev=makedev(0, 0x4c), st_ino=8815141}";
        let map = parse_struct(input).unwrap();
        assert_eq!(map.get("st_dev"), Some(&"makedev(0, 0x4c)".to_string()));
        assert_eq!(map.get("st_ino"), Some(&"8815141".to_string()));
    }

    // ========== FD TESTS ==========

    #[test]
    fn test_parse_fd() {
        let fd = parse_fd("3</etc/hosts>").unwrap();
        assert_eq!(fd.fd, 3);
        assert_eq!(fd.annotation, Some("/etc/hosts".to_string()));

        let fd = parse_fd("3<TCP:[127.0.0.1:9999]>").unwrap();
        assert_eq!(fd.fd, 3);
        assert_eq!(fd.annotation, Some("TCP:[127.0.0.1:9999]".to_string()));

        let fd = parse_fd("3").unwrap();
        assert_eq!(fd.fd, 3);
        assert_eq!(fd.annotation, None);
    }

    // ========== NUMERIC TESTS ==========

    #[test]
    fn test_parse_numeric() {
        // Known constant
        let val = parse_numeric("AT_FDCWD").unwrap();
        assert!(matches!(
            val,
            NumericValue::Constant {
                name,
                value: Some(-100)
            } if name == "AT_FDCWD"
        ));

        // Unknown constant
        let val = parse_numeric("SOME_CONSTANT").unwrap();
        assert!(matches!(
            val,
            NumericValue::Constant { name, value: None } if name == "SOME_CONSTANT"
        ));

        // Decimal
        assert_eq!(parse_numeric("123").unwrap(), NumericValue::Literal(123));
        assert_eq!(parse_numeric("-1").unwrap(), NumericValue::Literal(-1));

        // Hex
        assert_eq!(parse_numeric("0x7b").unwrap(), NumericValue::Literal(123));
        assert_eq!(parse_numeric("0X7B").unwrap(), NumericValue::Literal(123));

        // Octal
        assert_eq!(parse_numeric("0755").unwrap(), NumericValue::Literal(493));
    }

    // ========== SOCKADDR TESTS ==========

    #[test]
    fn test_parse_sockaddr() {
        let input =
            "{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr(\"127.0.0.1\")}";
        let addr = parse_sockaddr(input).unwrap();

        match addr {
            SockAddr::Inet { ip, port } => {
                assert_eq!(ip, "127.0.0.1");
                assert_eq!(port, 9999);
            }
            _ => panic!("Expected Inet variant"),
        }
    }

    #[test]
    fn test_extract_inet_addr() {
        let ip = extract_inet_addr("inet_addr(\"127.0.0.1\")").unwrap();
        assert_eq!(ip, "127.0.0.1");

        let ip = extract_inet_addr("inet_addr(\"192.168.1.1\")").unwrap();
        assert_eq!(ip, "192.168.1.1");
    }

    #[test]
    fn test_extract_htons() {
        assert_eq!(extract_htons("htons(9999)").unwrap(), 9999);
        assert_eq!(extract_htons("htons(80)").unwrap(), 80);
    }

    // ========== RETURN VALUE TESTS ==========

    #[test]
    fn test_parse_return_code() {
        assert_eq!(parse_return_code("0").unwrap(), 0);
        assert_eq!(parse_return_code("-1 ENOENT (No such file)").unwrap(), -1);
        assert_eq!(parse_return_code("123").unwrap(), 123);
    }

    #[test]
    fn test_parse_return_fd() {
        let (fd, path) = parse_return_fd("3</etc/hosts>").unwrap();
        assert_eq!(fd, 3);
        assert_eq!(path, Some("/etc/hosts".to_string()));

        let (fd, path) = parse_return_fd("0").unwrap();
        assert_eq!(fd, 0);
        assert_eq!(path, None);
    }
}
