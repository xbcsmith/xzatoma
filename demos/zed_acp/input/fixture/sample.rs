// XZatoma Zed ACP demo fixture: sample Rust snippet
//
// This file demonstrates a simple greeting function. It is intentionally
// minimal so the agent can describe it clearly.

/// Returns a greeting string for the given name.
///
/// # Arguments
///
/// * `name` - The name to greet.
///
/// # Returns
///
/// Returns a greeting string.
///
/// # Examples
///
/// ```
/// let greeting = greet("world");
/// assert_eq!(greeting, "Hello, world!");
/// ```
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

// TODO: add a farewell function
// TODO: support multiple languages

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet_returns_expected_greeting() {
        assert_eq!(greet("world"), "Hello, world!");
    }
}
