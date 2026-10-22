//! The MPD line protocol: argument quoting, `ACK` errors and the response buffer.

use std::fmt::Display;

/// The protocol version announced in the greeting. 0.23 is the newest one
/// whose filter grammar and `list ... group` output this server speaks.
pub(crate) const VERSION: &str = "0.23.5";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckCode {
    NotList = 1,
    Arg = 2,
    Password = 3,
    Permission = 4,
    Unknown = 5,
    NoExist = 50,
    System = 52,
    Exist = 56,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ack {
    pub code: AckCode,
    pub message: String,
}

impl Ack {
    pub fn new(code: AckCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn arg(message: impl Into<String>) -> Self {
        Self::new(AckCode::Arg, message)
    }

    pub fn no_exist(message: impl Into<String>) -> Self {
        Self::new(AckCode::NoExist, message)
    }

    pub fn unsupported(what: &str) -> Self {
        Self::new(
            AckCode::Unknown,
            format!("{what} is not supported by Kopuz"),
        )
    }

    pub fn render(&self, index: usize, command: &str) -> String {
        format!(
            "ACK [{}@{index}] {{{command}}} {}\n",
            self.code as u8,
            clean(&self.message)
        )
    }
}

impl From<api::ApiError> for Ack {
    fn from(error: api::ApiError) -> Self {
        let code = match error.code {
            api::ErrorCode::InvalidInput => AckCode::Arg,
            api::ErrorCode::NotFound | api::ErrorCode::ItemUnavailable => AckCode::NoExist,
            api::ErrorCode::Conflict => AckCode::Exist,
            _ => AckCode::System,
        };
        Self::new(code, error.message)
    }
}

/// Split a command line into its words. A word is either bare (up to the next
/// whitespace) or double-quoted, where a backslash escapes the next character.
pub(crate) fn tokenize(line: &str) -> Result<Vec<String>, Ack> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(&first) = chars.peek() else {
            return Ok(words);
        };
        let mut word = String::new();
        if first == '"' {
            chars.next();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(c) => word.push(c),
                        None => return Err(Ack::arg("Missing closing '\"'")),
                    },
                    Some(c) => word.push(c),
                    None => return Err(Ack::arg("Missing closing '\"'")),
                }
            }
            if chars.peek().is_some_and(|c| !c.is_whitespace()) {
                return Err(Ack::arg("Space expected after closing '\"'"));
            }
        } else {
            while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                word.push(c);
            }
        }
        words.push(word);
    }
}

/// A value on one response line: a newline in a tag would end the line early.
fn clean(value: &str) -> std::borrow::Cow<'_, str> {
    if value.contains(['\n', '\r']) {
        value.replace(['\n', '\r'], " ").into()
    } else {
        value.into()
    }
}

/// One command's answer, before its `OK` / `list_OK`.
#[derive(Debug, Default)]
pub(crate) struct Response {
    pub bytes: Vec<u8>,
}

impl Response {
    pub fn field(&mut self, key: &str, value: impl Display) {
        let value = value.to_string();
        self.bytes.extend_from_slice(key.as_bytes());
        self.bytes.extend_from_slice(b": ");
        self.bytes.extend_from_slice(clean(&value).as_bytes());
        self.bytes.push(b'\n');
    }

    /// A non-empty optional value; MPD omits a tag a song does not have.
    pub fn tag(&mut self, key: &str, value: &str) {
        if !value.is_empty() {
            self.field(key, value);
        }
    }

    pub fn binary(&mut self, chunk: &[u8]) {
        self.field("binary", chunk.len());
        self.bytes.extend_from_slice(chunk);
        self.bytes.push(b'\n');
    }
}

/// `START:END`, `START:` or a lone `POS`, as a half-open range clamped to `len`.
pub(crate) fn range(word: &str, len: u32) -> Result<std::ops::Range<u32>, Ack> {
    let number = |text: &str| {
        text.parse::<u32>()
            .map_err(|_| Ack::arg(format!("Number expected: {word}")))
    };
    let (start, end) = match word.split_once(':') {
        Some((start, "")) => (number(start)?, len),
        Some((start, end)) => (number(start)?, number(end)?),
        None => {
            let pos = number(word)?;
            (pos, pos.saturating_add(1))
        }
    };
    if start > end {
        return Err(Ack::arg(format!("Bad range: {word}")));
    }
    Ok(start.min(len)..end.min(len))
}

pub(crate) fn number(word: &str) -> Result<u32, Ack> {
    word.parse()
        .map_err(|_| Ack::arg(format!("Integer expected: {word}")))
}

pub(crate) fn flag(word: &str) -> Result<bool, Ack> {
    match word {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(Ack::arg(format!("Boolean (0/1) expected: {word}"))),
    }
}

/// Seconds with an optional fraction ("12", "12.5"), as milliseconds.
pub(crate) fn seconds(word: &str) -> Result<u64, Ack> {
    let value: f64 = word
        .parse()
        .map_err(|_| Ack::arg(format!("Float expected: {word}")))?;
    if !value.is_finite() || value < 0.0 {
        return Err(Ack::arg(format!("Bad time: {word}")));
    }
    Ok((value * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_on_whitespace_and_quotes_group() {
        assert_eq!(
            tokenize(r#"find  artist "Boards of Canada"  "#).unwrap(),
            ["find", "artist", "Boards of Canada"]
        );
    }

    #[test]
    fn a_backslash_escapes_inside_quotes() {
        assert_eq!(
            tokenize(r#"find "(Artist == \"A\\B\")""#).unwrap(),
            ["find", r#"(Artist == "A\B")"#]
        );
    }

    #[test]
    fn an_unterminated_quote_is_refused() {
        assert_eq!(tokenize(r#"add "x"#).unwrap_err().code, AckCode::Arg);
        assert_eq!(tokenize(r#"add "x"y"#).unwrap_err().code, AckCode::Arg);
    }

    #[test]
    fn ranges_are_half_open_and_clamped() {
        assert_eq!(range("2", 10).unwrap(), 2..3);
        assert_eq!(range("2:5", 10).unwrap(), 2..5);
        assert_eq!(range("2:", 10).unwrap(), 2..10);
        assert_eq!(range("8:20", 10).unwrap(), 8..10);
        assert!(range("5:2", 10).is_err());
        assert!(range("x", 10).is_err());
    }

    #[test]
    fn an_ack_names_its_code_index_and_command() {
        let ack = Ack::no_exist("No such song\nhere");
        assert_eq!(ack.render(2, "add"), "ACK [50@2] {add} No such song here\n");
    }

    #[test]
    fn seconds_take_fractions() {
        assert_eq!(seconds("12.5").unwrap(), 12_500);
        assert!(seconds("-1").is_err());
    }
}
