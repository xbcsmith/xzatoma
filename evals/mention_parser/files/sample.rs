// sample.rs - fixture for mention parser eval testing.
// Used by eval tests in evals/mention_parser/.

use std::fmt;

/// A sample struct used in eval testing.
pub struct Sample {
    pub name: String,
    pub value: i32,
}

impl Sample {
    /// Create a new Sample.
    pub fn new(name: &str, value: i32) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }
}
