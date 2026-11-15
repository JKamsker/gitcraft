//! Assertions that preserve fixture errors under the repository's no-unwrap lint policy.
use std::fmt::Debug;

pub(in crate::app) trait AssertValue {
    type Value;
    fn assert_value(self) -> Self::Value;
}

impl<T, E: Debug> AssertValue for Result<T, E> {
    type Value = T;
    fn assert_value(self) -> T {
        assert!(self.is_ok(), "fixture failed: {:?}", self.as_ref().err());
        match self {
            Ok(value) => value,
            Err(_) => unreachable!(),
        }
    }
}

impl<T> AssertValue for Option<T> {
    type Value = T;
    fn assert_value(self) -> T {
        assert!(self.is_some(), "expected fixture value");
        match self {
            Some(value) => value,
            None => unreachable!(),
        }
    }
}
