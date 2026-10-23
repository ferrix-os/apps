//! A JSONL export read back into lode's records: `lode -o json`, and what
//! `cargo xtask box-log` keeps.
//!
//! Each line is one object whose values are strings, or arrays of byte
//! values for a value that is not UTF-8 (journald's `json`, which lode
//! writes). A line that is not such an object -- box-log's first line,
//! which names the host and the commit -- is skipped and counted.

use std::rc::Rc;

use ferrix_lode_core::record::{Field, Record, UNSPECIFIED};
use ferrix_lode_core::segment;

/// What a file of lines gave.
#[derive(Debug, Default)]
pub struct Read {
    /// The records, in the file's order.
    pub records: Vec<Record>,
    /// Lines that were not a record.
    pub skipped: usize,
}

/// Read every line of `text`.
#[must_use]
pub fn read(text: &str) -> Read {
    let mut read = Read::default();
    let mut hosts: Vec<Rc<str>> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match object(line).and_then(|pairs| record(pairs, &mut hosts)) {
            Some(record) => read.records.push(record),
            None => read.skipped += 1,
        }
    }
    read
}

/// One line's record, or `None` when the line is not one of lode's.
#[must_use]
pub fn record_line(line: &str) -> Option<Record> {
    object(line.trim()).and_then(|pairs| record(pairs, &mut Vec::new()))
}

/// The record one object's pairs make, or `None` when it has no sequence
/// number and so is not one of lode's.
fn record(pairs: Vec<(String, Vec<u8>)>, hosts: &mut Vec<Rc<str>>) -> Option<Record> {
    let text = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| std::str::from_utf8(value).ok())
    };
    let number = |name: &str| text(name).and_then(|value| value.trim().parse::<u64>().ok());
    let seqnum = number("__SEQNUM")?;
    let level = match text("FERRIX_LEVEL") {
        Some("unspecified") => UNSPECIFIED,
        Some(value) => value.parse::<u8>().unwrap_or(UNSPECIFIED),
        None => text("PRIORITY")
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(UNSPECIFIED),
    };
    let mut record = Record::new(
        level,
        number("__REALTIME_TIMESTAMP").unwrap_or(0),
        number("__MONOTONIC_TIMESTAMP").unwrap_or(0),
    );
    record.seqnum = seqnum;
    record.utc_offset_s = text("__UTC_OFFSET")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(id) = text("_BOOT_ID").and_then(segment::parse_id) {
        record.boot_id = id;
    }
    if let Some(host) = text("_HOSTNAME") {
        record.host = match hosts.iter().find(|known| &***known == host) {
            Some(known) => Rc::clone(known),
            None => {
                let fresh: Rc<str> = Rc::from(host);
                hosts.push(Rc::clone(&fresh));
                fresh
            }
        };
    }
    for (name, value) in pairs {
        // The store's own stamps and what an output makes from the record
        // are the record's members, not its fields.
        if name.starts_with("__")
            || matches!(
                name.as_str(),
                "PRIORITY" | "FERRIX_LEVEL" | "_BOOT_ID" | "_HOSTNAME"
            )
        {
            continue;
        }
        record.fields.push(Field { name, value });
    }
    Some(record)
}

/// One object's pairs, or `None` when the line is not an object of strings
/// and byte arrays.
#[must_use]
pub fn object(line: &str) -> Option<Vec<(String, Vec<u8>)>> {
    let mut reader = Reader {
        bytes: line.as_bytes(),
        at: 0,
    };
    reader.space();
    reader.eat(b'{')?;
    let mut pairs = Vec::new();
    reader.space();
    if reader.peek() == Some(b'}') {
        return Some(pairs);
    }
    loop {
        reader.space();
        let name = String::from_utf8(reader.string()?).ok()?;
        reader.space();
        reader.eat(b':')?;
        reader.space();
        let value = match reader.peek()? {
            b'"' => reader.string()?,
            b'[' => reader.bytes()?,
            _ => reader.bare()?,
        };
        pairs.push((name, value));
        reader.space();
        match reader.next()? {
            b',' => {}
            b'}' => return Some(pairs),
            _ => return None,
        }
    }
}

/// A cursor over one line.
struct Reader<'a> {
    /// The line.
    bytes: &'a [u8],
    /// Where the next byte is.
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    fn eat(&mut self, wanted: u8) -> Option<()> {
        (self.next()? == wanted).then_some(())
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }

    /// A string's bytes, its escapes undone.
    fn string(&mut self) -> Option<Vec<u8>> {
        self.eat(b'"')?;
        let mut out = Vec::new();
        loop {
            match self.next()? {
                b'"' => return Some(out),
                b'\\' => match self.next()? {
                    b'"' => out.push(b'"'),
                    b'\\' => out.push(b'\\'),
                    b'/' => out.push(b'/'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'u' => {
                        let mut code = self.hex4()?;
                        if (0xd800..0xdc00).contains(&code) {
                            // A surrogate pair.
                            self.eat(b'\\')?;
                            self.eat(b'u')?;
                            let low = self.hex4()?;
                            code = 0x10000 + ((code - 0xd800) << 10) + (low.checked_sub(0xdc00)?);
                        }
                        let character = char::from_u32(code).unwrap_or('\u{fffd}');
                        let mut buffer = [0_u8; 4];
                        out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                    }
                    _ => return None,
                },
                byte => out.push(byte),
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut code = 0_u32;
        for _ in 0..4 {
            let digit = char::from(self.next()?).to_digit(16)?;
            code = code * 16 + digit;
        }
        Some(code)
    }

    /// An array of byte values: a value that is not UTF-8.
    fn bytes(&mut self) -> Option<Vec<u8>> {
        self.eat(b'[')?;
        let mut out = Vec::new();
        loop {
            self.space();
            if self.peek()? == b']' {
                self.at += 1;
                return Some(out);
            }
            let start = self.at;
            while self.peek()?.is_ascii_digit() {
                self.at += 1;
            }
            let digits = std::str::from_utf8(self.bytes.get(start..self.at)?).ok()?;
            out.push(digits.parse::<u8>().ok()?);
            self.space();
            if self.peek()? == b',' {
                self.at += 1;
            }
        }
    }

    /// A number, `true`, `false` or `null`: kept as its text.
    fn bare(&mut self) -> Option<Vec<u8>> {
        let start = self.at;
        while let Some(byte) = self.peek() {
            if matches!(byte, b',' | b'}' | b' ' | b'\t' | b'\r' | b'\n') {
                break;
            }
            self.at += 1;
        }
        let word = self.bytes.get(start..self.at)?;
        (!word.is_empty() && word != b"null").then(|| word.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::{object, read};

    #[test]
    fn an_object_of_strings_and_byte_arrays() {
        let pairs = object(r#"{"A":"x\"y\\n\né","B":[255,0, 1],"C":12}"#).expect("an object");
        assert_eq!(
            pairs[0],
            ("A".to_owned(), "x\"y\\n\n\u{e9}".as_bytes().to_vec())
        );
        assert_eq!(pairs[1], ("B".to_owned(), vec![255, 0, 1]));
        assert_eq!(pairs[2], ("C".to_owned(), b"12".to_vec()));
        assert!(object("not json").is_none());
        assert!(object(r#"{"A":"unterminated}"#).is_none());
    }

    #[test]
    fn a_line_of_lode_json_is_a_record_and_a_header_line_is_skipped() {
        let text = concat!(
            r#"{"box-log":"nazuna","commit":"abc"}"#,
            "\n",
            r#"{"__CURSOR":"i=7;b=062359e1029c423c0c0b4c9cf5216a14","__REALTIME_TIMESTAMP":"1792776109770158","__MONOTONIC_TIMESTAMP":"22957520","__SEQNUM":"7","_BOOT_ID":"062359e1029c423c0c0b4c9cf5216a14","FERRIX_LEVEL":"unspecified","_HOSTNAME":"ferrix","MESSAGE":"hello","SYSLOG_IDENTIFIER":"hyprix","_TRANSPORT":"stdout"}"#,
            "\n",
        );
        let read = read(text);
        assert_eq!(read.skipped, 1);
        let record = &read.records[0];
        assert_eq!(record.seqnum, 7);
        assert_eq!(record.priority, ferrix_lode_core::record::UNSPECIFIED);
        assert_eq!(record.realtime_us, 1_792_776_109_770_158);
        assert_eq!(&*record.host, "ferrix");
        assert_eq!(record.message(), b"hello");
        assert_eq!(record.boot_id[0], 0x06);
        assert!(record.get("__CURSOR").is_none());
    }
}
