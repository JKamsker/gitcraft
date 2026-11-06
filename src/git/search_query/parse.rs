//! Reading a query with operators into what it asks for.

use super::dates::{GitDate, git_date};
use super::token::{Token, TokenProblem, tokens};
use super::{MAX_QUERY_CHARS, Operator};

/// What a query with operators asks for. Values are as typed, quotes taken off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// The free text, its words (and quoted phrases) joined by single spaces.
    pub text: String,
    pub authors: Vec<String>,
    pub committers: Vec<String>,
    pub paths: Vec<String>,
    pub messages: Vec<String>,
    /// `--since=` / `--until=` values.
    pub after: Option<String>,
    pub before: Option<String>,
    /// `-S` (`contents:`) or `-G` (`regex:`): git takes one at a time.
    pub contents: Option<String>,
    pub regex: Option<String>,
    /// `merges:` (only merges) or `-merges:` (no merges).
    pub merges: Option<bool>,
}

/// Why a query can't be searched, and where (in characters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryError {
    pub message: String,
    pub at: usize,
}

/// What `query` asks for, or `None` when it has no operator at all: then it is plain text,
/// searched as it always was (quotes and all).
pub fn parse(query: &str) -> Result<Option<SearchQuery>, QueryError> {
    let pieces = tokens(query);
    if pieces.iter().all(|t| t.op.is_none()) {
        return Ok(None);
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(QueryError {
            message: format!(
                "This search is too long: at most {MAX_QUERY_CHARS} characters with operators"
            ),
            at: 0,
        });
    }
    let mut parser = Parser::new(query);
    for token in &pieces {
        parser.take(token)?;
    }
    parser.finish()
}

struct Parser<'a> {
    query: &'a str,
    out: SearchQuery,
    words: Vec<String>,
    after_day: Option<(i32, u32, u32)>,
    before_day: Option<(i32, u32, u32)>,
    /// Where `after:` was, for an error about the two dates.
    after_at: usize,
}

impl<'a> Parser<'a> {
    fn new(query: &'a str) -> Self {
        Parser {
            query,
            out: SearchQuery::default(),
            words: Vec::new(),
            after_day: None,
            before_day: None,
            after_at: 0,
        }
    }

    /// Characters before byte `byte`, as the field counts them.
    fn chars_to(&self, byte: usize) -> usize {
        self.query[..byte].chars().count()
    }

    fn error(&self, byte: usize, message: String) -> QueryError {
        QueryError {
            message,
            at: self.chars_to(byte),
        }
    }

    fn take(&mut self, token: &Token) -> Result<(), QueryError> {
        match token.problem {
            Some(TokenProblem::Unclosed) => {
                return Err(self.error(
                    token.value_start,
                    "This quote is not closed: add a \" at the end of the value".to_owned(),
                ));
            }
            Some(TokenProblem::AfterQuote) => {
                return Err(self.error(
                    token.value_start,
                    "Leave a space after the closing quote".to_owned(),
                ));
            }
            None => {}
        }
        if token.value.contains('\0') {
            return Err(self.error(
                token.value_start,
                "A NUL character can't be searched for".to_owned(),
            ));
        }
        match token.op {
            None => {
                self.words.push(token.value.clone());
                Ok(())
            }
            Some(op) => self.take_operator(op, token),
        }
    }

    fn take_operator(&mut self, op: Operator, token: &Token) -> Result<(), QueryError> {
        let start = token.span.start;
        let name = op.name();
        if token.negated && !op.negatable() {
            return Err(self.error(
                start,
                format!("-{name}: isn't supported: only merges: can be turned around with a -"),
            ));
        }
        if op == Operator::Merges {
            return self.take_merges(token);
        }
        let value = token.value.clone();
        if value.is_empty() {
            return Err(self.error(
                start,
                format!(
                    "{name}: needs {} right after the colon (quote it if it has spaces)",
                    op.needs()
                ),
            ));
        }
        match op {
            Operator::Author => self.out.authors.push(value),
            Operator::Committer => self.out.committers.push(value),
            Operator::Path => self.out.paths.push(value),
            Operator::Message => self.out.messages.push(value),
            Operator::After | Operator::Before => self.take_date(op, token)?,
            Operator::Contents | Operator::Regex => self.take_pickaxe(op, token)?,
            Operator::Merges => {}
        }
        Ok(())
    }

    fn take_merges(&mut self, token: &Token) -> Result<(), QueryError> {
        let start = token.span.start;
        let only = match token.value.to_lowercase().as_str() {
            "" | "yes" | "true" | "only" => !token.negated,
            "no" | "false" => token.negated,
            _ => {
                return Err(self.error(
                    start,
                    "merges: takes no value: write merges: for only merges, -merges: for none"
                        .to_owned(),
                ));
            }
        };
        match self.out.merges {
            Some(before) if before != only => Err(self.error(
                start,
                "merges: and -merges: together leave nothing: keep one of them".to_owned(),
            )),
            _ => {
                self.out.merges = Some(only);
                Ok(())
            }
        }
    }

    fn take_date(&mut self, op: Operator, token: &Token) -> Result<(), QueryError> {
        let start = token.span.start;
        let name = op.name();
        let GitDate { arg, day } = git_date(&token.value)
            .map_err(|e| self.error(token.value_start, format!("{name}: {e}")))?;
        let (slot, day_slot) = if op == Operator::After {
            self.after_at = start;
            (&mut self.out.after, &mut self.after_day)
        } else {
            (&mut self.out.before, &mut self.before_day)
        };
        if slot.is_some() {
            return Err(self.error(start, format!("Only one {name}: per search")));
        }
        *slot = Some(arg);
        *day_slot = day;
        Ok(())
    }

    fn take_pickaxe(&mut self, op: Operator, token: &Token) -> Result<(), QueryError> {
        if self.out.contents.is_some() || self.out.regex.is_some() {
            return Err(self.error(
                token.span.start,
                "Only one contents: or regex: per search: git looks for one change at a time"
                    .to_owned(),
            ));
        }
        let value = Some(token.value.clone());
        if op == Operator::Contents {
            self.out.contents = value;
        } else {
            self.out.regex = value;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Option<SearchQuery>, QueryError> {
        if let (Some(after), Some(before)) = (self.after_day, self.before_day)
            && after >= before
        {
            return Err(self.error(
                self.after_at,
                "The after: date must be earlier than the before: date".to_owned(),
            ));
        }
        self.out.text = self.words.join(" ");
        Ok(Some(self.out))
    }
}
