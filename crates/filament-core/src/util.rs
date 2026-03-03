/// Truncate a string to `max_chars` characters, appending "..." if truncated.
/// Safe for multi-byte UTF-8 strings (operates on char boundaries).
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    if max_chars < 4 {
        return s.chars().take(max_chars).collect();
    }
    let truncated: String = s.chars().take(max_chars - 3).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_when_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
    }

    #[test]
    fn exact_length_no_truncation() {
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn small_max_no_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 3), "hel");
    }

    #[test]
    fn zero_max() {
        assert_eq!(truncate_with_ellipsis("hello", 0), "");
    }
}
