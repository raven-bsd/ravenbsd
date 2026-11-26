// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room parser and evaluator for the tcpdump filter subset used by the fixtures.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// A compiled filter expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    expression: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Protocol(Protocol),
    Host {
        direction: Direction,
        address: Address,
    },
    Port {
        direction: Direction,
        port: u16,
    },
    Byte {
        protocol: ByteProtocol,
        offset: usize,
        size: usize,
        mask: u32,
        comparison: Comparison,
        value: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Icmp6,
    Ip,
    Ip6,
    Arp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteProtocol {
    Ip,
    Ip6,
    Tcp,
    Udp,
    Icmp,
    Icmp6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Any,
    Src,
    Dst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Address {
    V4([u8; 4]),
    V6([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

/// A malformed filter expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// The expression is empty.
    Empty,
    /// A token or construct is not supported.
    Syntax(&'static str),
    /// A numeric literal is invalid.
    Number(String),
    /// An address literal is invalid.
    Address(String),
}

impl core::fmt::Display for FilterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty filter expression"),
            Self::Syntax(message) => write!(f, "filter syntax error: {message}"),
            Self::Number(literal) => write!(f, "invalid numeric literal: {literal}"),
            Self::Address(literal) => write!(f, "invalid address literal: {literal}"),
        }
    }
}

impl Filter {
    /// Whether `frame` (an Ethernet II frame) passes the expression.
    #[must_use]
    pub fn matches(&self, frame: &[u8]) -> bool {
        let Some(facts) = Facts::parse(frame) else {
            return false;
        };
        evaluate(&self.expression, &facts)
    }
}

/// Compile one tcpdump filter expression.
///
/// # Errors
/// Returns [`FilterError`] for an empty expression, an unsupported primitive or
/// an invalid numeric/address literal.
pub fn compile(expression: &str) -> Result<Filter, FilterError> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser { tokens, index: 0 };
    let parsed = parser.parse_or()?;
    if parser.index != parser.tokens.len() {
        return Err(FilterError::Syntax("trailing tokens"));
    }
    Ok(Filter { expression: parsed })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Amp,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

fn tokenize(input: &str) -> Result<Vec<Token>, FilterError> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut chars = input.chars().peekable();
    let flush = |word: &mut String, tokens: &mut Vec<Token>| {
        if !word.is_empty() {
            tokens.push(Token::Word(core::mem::take(word)));
        }
    };
    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            flush(&mut word, &mut tokens);
            continue;
        }
        match character {
            '(' => {
                flush(&mut word, &mut tokens);
                tokens.push(Token::LParen);
            }
            ')' => {
                flush(&mut word, &mut tokens);
                tokens.push(Token::RParen);
            }
            '[' => {
                flush(&mut word, &mut tokens);
                tokens.push(Token::LBracket);
            }
            ']' => {
                flush(&mut word, &mut tokens);
                tokens.push(Token::RBracket);
            }
            '&' => {
                flush(&mut word, &mut tokens);
                tokens.push(Token::Amp);
            }
            '!' => {
                flush(&mut word, &mut tokens);
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Ne);
                } else {
                    return Err(FilterError::Syntax("expected !="));
                }
            }
            '=' => {
                flush(&mut word, &mut tokens);
                if chars.peek() == Some(&'=') {
                    chars.next();
                }
                tokens.push(Token::Eq);
            }
            '>' => {
                flush(&mut word, &mut tokens);
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Ge);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '<' => {
                flush(&mut word, &mut tokens);
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Le);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            _ => word.push(character),
        }
    }
    flush(&mut word, &mut tokens);
    if tokens.is_empty() {
        Err(FilterError::Empty)
    } else {
        Ok(tokens)
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }

    fn eat_word(&mut self, expected: &str) -> bool {
        if matches!(self.peek(), Some(Token::Word(word)) if word == expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, message: &'static str) -> Result<String, FilterError> {
        match self.next() {
            Some(Token::Word(word)) => Ok(word),
            _ => Err(FilterError::Syntax(message)),
        }
    }

    fn parse_or(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.parse_and()?;
        while self.eat_word("or") {
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                None | Some(Token::RParen) => break,
                Some(Token::Word(word)) if word == "or" => break,
                Some(Token::Word(word)) if word == "and" => {
                    self.index += 1;
                }
                Some(Token::Word(_)) | Some(Token::LParen) => {}
                _ => return Err(FilterError::Syntax("expected a filter primitive")),
            }
            let right = self.parse_unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, FilterError> {
        if self.eat_word("not") {
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        if matches!(self.peek(), Some(Token::LParen)) {
            self.index += 1;
            let expression = self.parse_or()?;
            if !matches!(self.next(), Some(Token::RParen)) {
                return Err(FilterError::Syntax("missing )"));
            }
            return Ok(expression);
        }
        self.parse_primitive()
    }

    fn parse_primitive(&mut self) -> Result<Expr, FilterError> {
        let word = self.expect_word("expected a filter primitive")?;
        match word.as_str() {
            "tcp" => self.protocol_or_byte(Protocol::Tcp, ByteProtocol::Tcp),
            "udp" => self.protocol_or_byte(Protocol::Udp, ByteProtocol::Udp),
            "icmp" => self.protocol_or_byte(Protocol::Icmp, ByteProtocol::Icmp),
            "icmp6" => self.protocol_or_byte(Protocol::Icmp6, ByteProtocol::Icmp6),
            "ip" => self.protocol_or_byte(Protocol::Ip, ByteProtocol::Ip),
            "ip6" => self.protocol_or_byte(Protocol::Ip6, ByteProtocol::Ip6),
            "arp" => Ok(Expr::Protocol(Protocol::Arp)),
            "src" | "dst" => {
                let direction = if word == "src" {
                    Direction::Src
                } else {
                    Direction::Dst
                };
                let qualifier = self.expect_word("expected host or port after src/dst")?;
                match qualifier.as_str() {
                    "host" => {
                        let address = parse_address(&self.expect_word("expected an address")?)?;
                        Ok(Expr::Host { direction, address })
                    }
                    "port" => {
                        let port = parse_port(&self.expect_word("expected a port")?)?;
                        Ok(Expr::Port { direction, port })
                    }
                    _ => Err(FilterError::Syntax("expected host or port after src/dst")),
                }
            }
            "host" => {
                let address = parse_address(&self.expect_word("expected an address")?)?;
                Ok(Expr::Host {
                    direction: Direction::Any,
                    address,
                })
            }
            "port" => {
                let port = parse_port(&self.expect_word("expected a port")?)?;
                Ok(Expr::Port {
                    direction: Direction::Any,
                    port,
                })
            }
            _ => Err(FilterError::Syntax("unknown filter primitive")),
        }
    }

    fn protocol_or_byte(
        &mut self,
        protocol: Protocol,
        byte_protocol: ByteProtocol,
    ) -> Result<Expr, FilterError> {
        if matches!(self.peek(), Some(Token::LBracket)) {
            self.index += 1;
            self.parse_byte(byte_protocol)
        } else {
            Ok(Expr::Protocol(protocol))
        }
    }

    fn parse_byte(&mut self, protocol: ByteProtocol) -> Result<Expr, FilterError> {
        let field = self.expect_word("expected a byte offset")?;
        let (offset, size) = if let Some((offset, size)) = field.split_once(':') {
            (parse_usize(offset)?, parse_usize(size)?)
        } else if let Some(offset) = named_field(&field) {
            (offset, 1)
        } else {
            (parse_usize(&field)?, 1)
        };
        if !matches!(size, 1 | 2 | 4) {
            return Err(FilterError::Syntax("byte field size must be 1, 2 or 4"));
        }
        if !matches!(self.next(), Some(Token::RBracket)) {
            return Err(FilterError::Syntax("missing ]"));
        }
        let mask = if matches!(self.peek(), Some(Token::Amp)) {
            self.index += 1;
            let literal = self.expect_word("expected a mask")?;
            named_constant(&literal).map_or_else(|| parse_u32(&literal), Ok)?
        } else {
            u32::MAX
        };
        let comparison = match self.next() {
            Some(Token::Eq) => Comparison::Eq,
            Some(Token::Ne) => Comparison::Ne,
            Some(Token::Gt) => Comparison::Gt,
            Some(Token::Lt) => Comparison::Lt,
            Some(Token::Ge) => Comparison::Ge,
            Some(Token::Le) => Comparison::Le,
            _ => return Err(FilterError::Syntax("expected a comparison")),
        };
        let literal = self.expect_word("expected a value")?;
        let value = named_constant(&literal).map_or_else(|| parse_u32(&literal), Ok)?;
        Ok(Expr::Byte {
            protocol,
            offset,
            size,
            mask,
            comparison,
            value,
        })
    }
}

fn named_field(field: &str) -> Option<usize> {
    match field {
        "tcpflags" => Some(13),
        "icmptype" => Some(0),
        "icmpcode" => Some(1),
        _ => None,
    }
}

fn named_constant(value: &str) -> Option<u32> {
    match value {
        "tcp-fin" => Some(0x01),
        "tcp-syn" => Some(0x02),
        "tcp-rst" => Some(0x04),
        "tcp-push" => Some(0x08),
        "tcp-ack" => Some(0x10),
        "tcp-urg" => Some(0x20),
        "icmp-echo" => Some(8),
        "icmp-echoreply" => Some(0),
        "icmp6-echo" => Some(128),
        "icmp6-echoreply" => Some(129),
        _ => None,
    }
}

fn parse_port(value: &str) -> Result<u16, FilterError> {
    let parsed = parse_u32(value)?;
    u16::try_from(parsed).map_err(|_| FilterError::Number(value.to_string()))
}

fn parse_usize(value: &str) -> Result<usize, FilterError> {
    let parsed = parse_u32(value)?;
    usize::try_from(parsed).map_err(|_| FilterError::Number(value.to_string()))
}

fn parse_u32(value: &str) -> Result<u32, FilterError> {
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16)
    } else {
        value.parse::<u32>()
    };
    parsed.map_err(|_| FilterError::Number(value.to_string()))
}

fn parse_address(value: &str) -> Result<Address, FilterError> {
    if value.contains(':') {
        parse_ipv6(value).map(Address::V6)
    } else {
        parse_ipv4(value).map(Address::V4)
    }
}

fn parse_ipv4(value: &str) -> Result<[u8; 4], FilterError> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in value.split('.') {
        if count == 4 {
            return Err(FilterError::Address(value.to_string()));
        }
        let octet = part
            .parse::<u8>()
            .map_err(|_| FilterError::Address(value.to_string()))?;
        octets[count] = octet;
        count += 1;
    }
    if count == 4 {
        Ok(octets)
    } else {
        Err(FilterError::Address(value.to_string()))
    }
}

fn parse_ipv6(value: &str) -> Result<[u8; 16], FilterError> {
    let (head, tail) = if let Some(position) = value.find("::") {
        if value
            .get(position + 2..)
            .is_some_and(|rest| rest.contains("::"))
        {
            return Err(FilterError::Address(value.to_string()));
        }
        (&value[..position], Some(&value[position + 2..]))
    } else {
        (value, None)
    };
    let compressed = tail.is_some();
    let head = parse_ipv6_groups(head)?;
    let tail = match tail {
        Some(tail) => parse_ipv6_groups(tail)?,
        None => Vec::new(),
    };
    if !compressed && head.len() != 8 {
        return Err(FilterError::Address(value.to_string()));
    }
    if head.len() + tail.len() > 8 {
        return Err(FilterError::Address(value.to_string()));
    }
    let mut groups = [0u16; 8];
    for (index, group) in head.iter().enumerate() {
        groups[index] = *group;
    }
    let start = 8 - tail.len();
    for (index, group) in tail.iter().enumerate() {
        groups[start + index] = *group;
    }
    let mut octets = [0u8; 16];
    for (index, group) in groups.iter().enumerate() {
        let bytes = group.to_be_bytes();
        octets[index * 2] = bytes[0];
        octets[index * 2 + 1] = bytes[1];
    }
    Ok(octets)
}

fn parse_ipv6_groups(value: &str) -> Result<Vec<u16>, FilterError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(':')
        .map(|group| {
            u16::from_str_radix(group, 16).map_err(|_| FilterError::Address(value.to_string()))
        })
        .collect()
}

struct Facts<'a> {
    v4: Option<Facts4<'a>>,
    v6: Option<Facts6<'a>>,
    arp: bool,
}

struct Facts4<'a> {
    header: &'a [u8],
    transport: Option<&'a [u8]>,
    proto: u8,
    src: [u8; 4],
    dst: [u8; 4],
}

struct Facts6<'a> {
    header: &'a [u8],
    transport: Option<&'a [u8]>,
    proto: u8,
    src: [u8; 16],
    dst: [u8; 16],
}

impl<'a> Facts<'a> {
    fn parse(frame: &'a [u8]) -> Option<Self> {
        let ether_type = read_u16(frame, 12)?;
        let payload = frame.get(14..)?;
        match ether_type {
            0x0800 => {
                if payload.len() < 20 || payload[0] >> 4 != 4 {
                    return Some(Self {
                        v4: None,
                        v6: None,
                        arp: false,
                    });
                }
                let header_len = usize::from(payload[0] & 0x0f).checked_mul(4)?;
                if header_len < 20 || header_len > payload.len() {
                    return Some(Self {
                        v4: None,
                        v6: None,
                        arp: false,
                    });
                }
                let fragment = read_u16(payload, 6)?;
                let transport = if fragment & 0x1fff == 0 {
                    payload.get(header_len..)
                } else {
                    None
                };
                Some(Self {
                    v4: Some(Facts4 {
                        header: payload.get(..header_len)?,
                        transport,
                        proto: *payload.get(9)?,
                        src: payload.get(12..16)?.try_into().ok()?,
                        dst: payload.get(16..20)?.try_into().ok()?,
                    }),
                    v6: None,
                    arp: false,
                })
            }
            0x86dd => {
                if payload.len() < 40 || payload[0] >> 4 != 6 {
                    return Some(Self {
                        v4: None,
                        v6: None,
                        arp: false,
                    });
                }
                let (proto, transport) = walk_ipv6(payload)?;
                Some(Self {
                    v4: None,
                    v6: Some(Facts6 {
                        header: payload.get(..40)?,
                        transport,
                        proto,
                        src: payload.get(8..24)?.try_into().ok()?,
                        dst: payload.get(24..40)?.try_into().ok()?,
                    }),
                    arp: false,
                })
            }
            0x0806 => Some(Self {
                v4: None,
                v6: None,
                arp: true,
            }),
            _ => Some(Self {
                v4: None,
                v6: None,
                arp: false,
            }),
        }
    }
}

fn walk_ipv6(payload: &[u8]) -> Option<(u8, Option<&[u8]>)> {
    let mut proto = *payload.get(6)?;
    let mut cursor = 40usize;
    loop {
        match proto {
            0 | 43 | 60 => {
                let next = *payload.get(cursor)?;
                let length = usize::from(*payload.get(cursor + 1)?)
                    .checked_add(1)?
                    .checked_mul(8)?;
                cursor = cursor.checked_add(length)?;
                proto = next;
            }
            44 => {
                let next = *payload.get(cursor)?;
                let flags = read_u16(payload, cursor + 2)?;
                cursor = cursor.checked_add(8)?;
                proto = next;
                if flags & 0xfff8 != 0 {
                    return Some((proto, None));
                }
            }
            51 => {
                let next = *payload.get(cursor)?;
                let length = usize::from(*payload.get(cursor + 1)?)
                    .checked_add(2)?
                    .checked_mul(4)?;
                cursor = cursor.checked_add(length)?;
                proto = next;
            }
            _ => return Some((proto, payload.get(cursor..))),
        }
        if cursor > payload.len() {
            return None;
        }
    }
}

fn evaluate(expression: &Expr, facts: &Facts<'_>) -> bool {
    match expression {
        Expr::And(left, right) => evaluate(left, facts) && evaluate(right, facts),
        Expr::Or(left, right) => evaluate(left, facts) || evaluate(right, facts),
        Expr::Not(inner) => !evaluate(inner, facts),
        Expr::Protocol(protocol) => match protocol {
            Protocol::Tcp => transport_proto(facts) == Some(6),
            Protocol::Udp => transport_proto(facts) == Some(17),
            Protocol::Icmp => facts.v4.as_ref().is_some_and(|v4| v4.proto == 1),
            Protocol::Icmp6 => facts
                .v6
                .as_ref()
                .is_some_and(|v6| v6.proto == 58 && v6.transport.is_some()),
            Protocol::Ip => facts.v4.is_some(),
            Protocol::Ip6 => facts.v6.is_some(),
            Protocol::Arp => facts.arp,
        },
        Expr::Host { direction, address } => match address {
            Address::V4(address) => facts.v4.as_ref().is_some_and(|v4| match direction {
                Direction::Any => v4.src == *address || v4.dst == *address,
                Direction::Src => v4.src == *address,
                Direction::Dst => v4.dst == *address,
            }),
            Address::V6(address) => facts.v6.as_ref().is_some_and(|v6| match direction {
                Direction::Any => v6.src == *address || v6.dst == *address,
                Direction::Src => v6.src == *address,
                Direction::Dst => v6.dst == *address,
            }),
        },
        Expr::Port { direction, port } => {
            let Some((source, destination)) = transport_ports(facts) else {
                return false;
            };
            match direction {
                Direction::Any => source == *port || destination == *port,
                Direction::Src => source == *port,
                Direction::Dst => destination == *port,
            }
        }
        Expr::Byte {
            protocol,
            offset,
            size,
            mask,
            comparison,
            value,
        } => {
            let Some(slice) = byte_slice(facts, *protocol) else {
                return false;
            };
            let Some(end) = offset.checked_add(*size) else {
                return false;
            };
            let Some(bytes) = slice.get(*offset..end) else {
                return false;
            };
            let loaded = match bytes {
                [byte] => u32::from(*byte),
                [high, low] => u32::from(u16::from_be_bytes([*high, *low])),
                [a, b, c, d] => u32::from_be_bytes([*a, *b, *c, *d]),
                _ => return false,
            };
            let loaded = loaded & *mask;
            match comparison {
                Comparison::Eq => loaded == *value,
                Comparison::Ne => loaded != *value,
                Comparison::Gt => loaded > *value,
                Comparison::Lt => loaded < *value,
                Comparison::Ge => loaded >= *value,
                Comparison::Le => loaded <= *value,
            }
        }
    }
}

fn transport_proto(facts: &Facts<'_>) -> Option<u8> {
    if let Some(v4) = &facts.v4
        && v4.transport.is_some()
    {
        return Some(v4.proto);
    }
    if let Some(v6) = &facts.v6
        && v6.transport.is_some()
    {
        return Some(v6.proto);
    }
    None
}

fn transport_ports(facts: &Facts<'_>) -> Option<(u16, u16)> {
    let (proto, transport) = if let Some(v4) = &facts.v4 {
        (v4.proto, v4.transport)
    } else if let Some(v6) = &facts.v6 {
        (v6.proto, v6.transport)
    } else {
        return None;
    };
    if !matches!(proto, 6 | 17) {
        return None;
    }
    let transport = transport?;
    Some((read_u16(transport, 0)?, read_u16(transport, 2)?))
}

fn byte_slice<'a>(facts: &Facts<'a>, protocol: ByteProtocol) -> Option<&'a [u8]> {
    match protocol {
        ByteProtocol::Ip => facts.v4.as_ref().map(|v4| v4.header),
        ByteProtocol::Ip6 => facts.v6.as_ref().map(|v6| v6.header),
        ByteProtocol::Tcp => facts
            .v4
            .as_ref()
            .filter(|v4| v4.proto == 6)
            .and_then(|v4| v4.transport)
            .or_else(|| {
                facts
                    .v6
                    .as_ref()
                    .filter(|v6| v6.proto == 6)
                    .and_then(|v6| v6.transport)
            }),
        ByteProtocol::Udp => facts
            .v4
            .as_ref()
            .filter(|v4| v4.proto == 17)
            .and_then(|v4| v4.transport)
            .or_else(|| {
                facts
                    .v6
                    .as_ref()
                    .filter(|v6| v6.proto == 17)
                    .and_then(|v6| v6.transport)
            }),
        ByteProtocol::Icmp => facts
            .v4
            .as_ref()
            .filter(|v4| v4.proto == 1)
            .and_then(|v4| v4.transport),
        ByteProtocol::Icmp6 => facts
            .v6
            .as_ref()
            .filter(|v6| v6.proto == 58)
            .and_then(|v6| v6.transport),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([pair[0], pair[1]]))
}

#[cfg(test)]
#[path = "filter_ut.rs"]
mod tests;
