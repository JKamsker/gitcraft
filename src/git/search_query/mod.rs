//! Search operators for the graph's search field: `author:`, `path:`, `after:`… parsed out of
//! what is typed, turned into `git log` options (each one argument, never through a shell),
//! and run. A query without operators is searched as it always was.

mod args;
mod dates;
mod operator;
mod parse;
mod run;
mod token;

#[cfg(test)]
mod tests;

pub use self::args::{Plan, prepare};
pub use self::dates::date_suggestions;
pub use self::operator::Operator;
pub use self::run::{Person, people, run};
pub use self::token::{quote_value, token_at};

/// The longest query with operators searched: past it, the command line itself could get
/// too long for the system (a plain query is not limited, as before).
pub const MAX_QUERY_CHARS: usize = 4096;
