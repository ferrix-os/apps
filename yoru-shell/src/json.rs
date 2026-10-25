//! A JSON value: enough of it for what hyprix's IPC answers.

/// A value.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string.
    Str(String),
    /// A list.
    Array(Vec<Json>),
    /// An object, its members in order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// `text` read whole, or `None` when it is not JSON.
    #[must_use]
    pub fn parse(text: &str) -> Option<Json> {
        let mut reader = Reader {
            chars: text.chars().collect(),
            at: 0,
        };
        let value = reader.value()?;
        reader.space();
        (reader.at == reader.chars.len()).then_some(value)
    }

    /// An object's member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A list's items; nothing for anything else.
    #[must_use]
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Array(items) => items,
            _ => &[],
        }
    }

    /// The value as a string.
    #[must_use]
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The value as a number.
    #[must_use]
    pub fn num(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The value as a whole number.
    #[must_use]
    pub fn int(&self) -> Option<i64> {
        match self {
            #[expect(clippy::cast_possible_truncation, reason = "ids and counts")]
            Json::Number(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }
}

/// A reader over JSON text.
struct Reader {
    chars: Vec<char>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, word: &str) -> bool {
        let end = self.at + word.chars().count();
        if self
            .chars
            .get(self.at..end)
            .is_some_and(|s| s.iter().copied().eq(word.chars()))
        {
            self.at = end;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.space();
        match self.peek()? {
            '{' => {
                self.at += 1;
                let mut members = Vec::new();
                if self.eat('}') {
                    return Some(Json::Object(members));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if !self.eat(':') {
                        return None;
                    }
                    members.push((key, self.value()?));
                    if self.eat(',') {
                        continue;
                    }
                    return self.eat('}').then_some(Json::Object(members));
                }
            }
            '[' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.eat(']') {
                    return Some(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(',') {
                        continue;
                    }
                    return self.eat(']').then_some(Json::Array(items));
                }
            }
            '"' => self.string().map(Json::Str),
            't' => self.word("true").then_some(Json::Bool(true)),
            'f' => self.word("false").then_some(Json::Bool(false)),
            'n' => self.word("null").then_some(Json::Null),
            _ => self.number(),
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.peek()? != '"' {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.at += 1;
            match c {
                '"' => return Some(out),
                '\\' => {
                    let escaped = self.peek()?;
                    self.at += 1;
                    match escaped {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let hex: String =
                                self.chars.get(self.at..self.at + 4)?.iter().collect();
                            self.at += 4;
                            let code = u32::from_str_radix(&hex, 16).ok()?;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
        {
            self.at += 1;
        }
        let text: String = self.chars.get(start..self.at)?.iter().collect();
        text.parse().ok().map(Json::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::Json;

    /// What hyprctl answers reads back.
    #[test]
    fn reads_an_answer() {
        let json = Json::parse(
            r#"[{"id": 2, "name": "2", "windows": 3, "x": -1.5, "t": "a\"bé"}, null, true]"#,
        )
        .expect("parses");
        let first = &json.items()[0];
        assert_eq!(first.get("id").and_then(Json::int), Some(2));
        assert_eq!(first.get("windows").and_then(Json::int), Some(3));
        assert_eq!(first.get("x"), Some(&Json::Number(-1.5)));
        assert_eq!(first.get("t").and_then(Json::str), Some("a\"bé"));
        assert_eq!(json.items()[2], Json::Bool(true));
        assert_eq!(Json::parse("{"), None);
        assert_eq!(Json::parse("[1] x"), None);
    }
}
