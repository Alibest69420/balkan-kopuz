//! Song filters: the `(TAG == 'VALUE')` expression grammar of MPD 0.21+, and
//! the older `TAG VALUE ...` pairs that clients still send.

use crate::library::{Song, Tag};
use crate::protocol::Ack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    StartsWith,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subject {
    Tag(Tag),
    File,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Filter {
    Compare {
        subject: Subject,
        op: Op,
        value: String,
        /// Set by an `_cs` / `_ci` operator; otherwise the command decides.
        fold: Option<bool>,
    },
    Base(String),
    Not(Box<Filter>),
    And(Vec<Filter>),
}

impl Filter {
    /// `fold` is the command's default: `search` ignores case, `find` does not.
    pub fn matches(&self, song: &Song, fold: bool) -> bool {
        match self {
            Filter::Compare {
                subject,
                op,
                value,
                fold: own,
            } => {
                let fold = own.unwrap_or(fold);
                let negated = matches!(op, Op::NotEquals | Op::NotContains);
                let positive = match op {
                    Op::Equals | Op::NotEquals => Op::Equals,
                    Op::Contains | Op::NotContains => Op::Contains,
                    Op::StartsWith => Op::StartsWith,
                };
                let hit = match subject {
                    Subject::Tag(tag) => compare(positive, &song.tag(*tag), value, fold),
                    Subject::File => compare(positive, &song.uri, value, fold),
                    Subject::Any => {
                        Tag::SUPPORTED
                            .iter()
                            .any(|(tag, _)| compare(positive, &song.tag(*tag), value, fold))
                            || compare(positive, &song.uri, value, fold)
                    }
                };
                hit != negated
            }
            Filter::Base(dir) => {
                let dir = dir.trim_matches('/');
                dir.is_empty()
                    || song
                        .uri
                        .strip_prefix(dir)
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            Filter::Not(inner) => !inner.matches(song, fold),
            Filter::And(all) => all.iter().all(|filter| filter.matches(song, fold)),
        }
    }
}

fn compare(op: Op, haystack: &str, needle: &str, fold: bool) -> bool {
    let (haystack, needle) = if fold {
        (haystack.to_lowercase(), needle.to_lowercase())
    } else {
        (haystack.to_string(), needle.to_string())
    };
    match op {
        Op::Equals | Op::NotEquals => haystack == needle,
        Op::Contains | Op::NotContains => haystack.contains(&needle),
        Op::StartsWith => haystack.starts_with(&needle),
    }
}

/// Words that end a filter and start a command's trailing options.
const KEYWORDS: [&str; 4] = ["sort", "window", "group", "position"];

/// Read a filter from the start of `args`: one expression, or `TAG VALUE`
/// pairs compared with `legacy` (exact for `find`, a substring for `search`).
/// Returns the filter, `None` when there is none, and the words it used.
pub(crate) fn parse(args: &[String], legacy: Op) -> Result<(Option<Filter>, usize), Ack> {
    match args.first() {
        None => Ok((None, 0)),
        Some(first) if first.starts_with('(') => {
            let mut parser = Parser {
                text: first.as_str(),
                pos: 0,
            };
            let filter = parser.expression()?;
            parser.space();
            if parser.pos != parser.text.len() {
                return Err(Ack::arg("Unparsed garbage after expression"));
            }
            Ok((Some(filter), 1))
        }
        Some(_) => {
            let mut all = Vec::new();
            let mut used = 0;
            while let Some(name) = args.get(used) {
                if KEYWORDS.contains(&name.to_ascii_lowercase().as_str()) {
                    break;
                }
                let value = args
                    .get(used + 1)
                    .ok_or_else(|| Ack::arg("Odd number of filter words"))?;
                all.push(legacy_pair(name, value.clone(), legacy)?);
                used += 2;
            }
            let filter = match all.len() {
                0 => None,
                1 => all.pop(),
                _ => Some(Filter::And(all)),
            };
            Ok((filter, used))
        }
    }
}

fn legacy_pair(name: &str, value: String, op: Op) -> Result<Filter, Ack> {
    if name.eq_ignore_ascii_case("base") {
        return Ok(Filter::Base(value));
    }
    Ok(Filter::Compare {
        subject: subject(name)?,
        op,
        value,
        fold: None,
    })
}

fn subject(name: &str) -> Result<Subject, Ack> {
    if name.eq_ignore_ascii_case("file") {
        return Ok(Subject::File);
    }
    if name.eq_ignore_ascii_case("any") {
        return Ok(Subject::Any);
    }
    Tag::parse(name)
        .map(Subject::Tag)
        .ok_or_else(|| Ack::arg(format!("Unknown filter type: {name}")))
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn space(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.text.len() - trimmed.len();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.space();
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), Ack> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(Ack::arg(format!("'{token}' expected")))
        }
    }

    fn word(&mut self) -> &str {
        self.space();
        let start = self.pos;
        let len = self
            .rest()
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '!'))
            .unwrap_or(self.rest().len());
        self.pos += len;
        &self.text[start..self.pos]
    }

    fn expression(&mut self) -> Result<Filter, Ack> {
        self.expect("(")?;
        if self.eat("!") {
            let inner = self.expression()?;
            self.expect(")")?;
            return Ok(Filter::Not(Box::new(inner)));
        }
        self.space();
        if self.rest().starts_with('(') {
            let mut all = vec![self.expression()?];
            while !self.eat(")") {
                if !self.word().eq_ignore_ascii_case("AND") {
                    return Err(Ack::arg("'AND' expected"));
                }
                all.push(self.expression()?);
            }
            return Ok(if all.len() == 1 {
                all.remove(0)
            } else {
                Filter::And(all)
            });
        }
        let name = self.word().to_string();
        if name.is_empty() {
            return Err(Ack::arg("Tag name expected"));
        }
        let lower = name.to_ascii_lowercase();
        if lower == "base" {
            let value = self.string()?;
            self.expect(")")?;
            return Ok(Filter::Base(value));
        }
        if matches!(
            lower.as_str(),
            "modified-since" | "added-since" | "audioformat" | "prio"
        ) {
            return Err(Ack::arg(format!(
                "{name} filters are not supported by Kopuz"
            )));
        }
        let subject = subject(&name)?;
        let (op, fold) = self.op()?;
        let value = self.string()?;
        self.expect(")")?;
        Ok(Filter::Compare {
            subject,
            op,
            value,
            fold,
        })
    }

    fn op(&mut self) -> Result<(Op, Option<bool>), Ack> {
        self.space();
        for (symbol, op) in [("==", Op::Equals), ("!=", Op::NotEquals)] {
            if self.eat(symbol) {
                return Ok((op, None));
            }
        }
        if self.rest().starts_with("=~") || self.rest().starts_with("!~") {
            return Err(Ack::arg("regular expressions are not supported by Kopuz"));
        }
        let word = self.word().to_ascii_lowercase();
        let (base, fold) = match word.rsplit_once('_') {
            Some((base, "cs")) => (base, Some(false)),
            Some((base, "ci")) => (base, Some(true)),
            _ => (word.as_str(), None),
        };
        let op = match base {
            "eq" => Op::Equals,
            "contains" => Op::Contains,
            "!contains" => Op::NotContains,
            "starts_with" => Op::StartsWith,
            _ => return Err(Ack::arg(format!("Unknown filter operator: {word}"))),
        };
        Ok((op, fold))
    }

    fn string(&mut self) -> Result<String, Ack> {
        self.space();
        let mut chars = self.rest().char_indices();
        let quote = match chars.next() {
            Some((_, quote @ ('"' | '\''))) => quote,
            _ => return Err(Ack::arg("Quoted string expected")),
        };
        let mut value = String::new();
        let mut escaped = false;
        for (index, c) in chars {
            if escaped {
                value.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == quote {
                self.pos += index + 1;
                return Ok(value);
            } else {
                value.push(c);
            }
        }
        Err(Ack::arg("Closing quote not found"))
    }
}

/// What may follow a filter: `sort [-]TAG`, `window START:END` and any
/// number of `group TAG`.
#[derive(Debug, Default)]
pub(crate) struct Trailing {
    pub sort: Option<(Tag, bool)>,
    pub window: Option<String>,
    pub groups: Vec<Tag>,
    pub position: Option<String>,
}

pub(crate) fn trailing(args: &[String]) -> Result<Trailing, Ack> {
    let mut out = Trailing::default();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let value = words
            .next()
            .ok_or_else(|| Ack::arg(format!("{word} needs a value")))?;
        match word.to_ascii_lowercase().as_str() {
            "sort" => {
                let (descending, name) = match value.strip_prefix('-') {
                    Some(name) => (true, name),
                    None => (false, value.as_str()),
                };
                // Sorting by modification time has nothing to sort on here.
                out.sort = Tag::parse(name).map(|tag| (tag, descending));
            }
            "window" => out.window = Some(value.clone()),
            "position" => out.position = Some(value.clone()),
            "group" => out
                .groups
                .push(Tag::parse(value).ok_or_else(|| Ack::arg(format!("Unknown tag: {value}")))?),
            _ => return Err(Ack::arg(format!("Unknown option: {word}"))),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::library;

    fn words(text: &str) -> Vec<String> {
        crate::protocol::tokenize(text).unwrap()
    }

    fn hits(text: &str, legacy: Op, fold: bool) -> Vec<String> {
        let args = words(text);
        let (filter, _) = parse(&args, legacy).unwrap();
        let filter = filter.unwrap();
        library()
            .songs
            .iter()
            .filter(|song| filter.matches(song, fold))
            .map(|song| song.track.title.clone())
            .collect()
    }

    #[test]
    fn an_expression_compares_one_tag() {
        assert_eq!(
            hits(r#""(Artist == 'Aphex Twin')""#, Op::Equals, false),
            ["Avril 14th"]
        );
    }

    #[test]
    fn and_not_and_case_folding_combine() {
        let text = r#""((AlbumArtist == 'boards of canada') AND (!(title contains 'Math')))""#;
        assert!(hits(text, Op::Equals, false).is_empty());
        assert_eq!(hits(text, Op::Equals, true), ["Ready Lets Go"]);
    }

    #[test]
    fn legacy_pairs_use_the_commands_operator() {
        assert_eq!(
            hits("album Geogaddi title Math", Op::Contains, false),
            ["Music Is Math"]
        );
        assert!(hits("album Geo", Op::Equals, false).is_empty());
    }

    #[test]
    fn any_and_base_and_file_match_the_uri() {
        assert_eq!(hits("any burial", Op::Contains, true), ["Archangel"]);
        assert_eq!(hits(r#""(base 'music/boc')""#, Op::Equals, false).len(), 2);
        assert_eq!(
            hits(r#""(file == 'music/loose.mp3')""#, Op::Equals, false),
            ["Avril 14th"]
        );
    }

    #[test]
    fn suffixed_operators_set_their_own_case() {
        assert_eq!(
            hits(r#""(title starts_with_ci 'avril')""#, Op::Equals, false),
            ["Avril 14th"]
        );
        assert!(hits(r#""(title eq_cs 'avril 14th')""#, Op::Equals, true).is_empty());
    }

    #[test]
    fn a_filter_stops_at_trailing_options() {
        let args = words("artist X sort -Title window 0:2 group album");
        let (_, used) = parse(&args, Op::Equals).unwrap();
        assert_eq!(used, 2);
        let options = trailing(&args[used..]).unwrap();
        assert_eq!(options.sort, Some((Tag::Title, true)));
        assert_eq!(options.window.as_deref(), Some("0:2"));
        assert_eq!(options.groups, [Tag::Album]);
    }

    #[test]
    fn malformed_expressions_are_refused() {
        for text in [
            r#""(Artist == 'x'""#,
            r#""(Artist =~ 'x')""#,
            r#""(Nonsense == 'x')""#,
            r#""(Artist == x)""#,
            "artist",
        ] {
            assert!(parse(&words(text), Op::Equals).is_err(), "{text}");
        }
    }
}
