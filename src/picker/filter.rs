use regex::RegexBuilder;

/// Indices of `haystacks` that match `query` as a case-insensitive regex; if
/// `query` fails to compile as a regex (common mid-keystroke, e.g. an
/// unclosed paren), falls back to a literal case-insensitive substring match
/// so the list never blanks out or errors while typing.
pub fn filter_indices(query: &str, haystacks: &[String]) -> Vec<usize> {
    if query.is_empty() {
        return (0..haystacks.len()).collect();
    }

    if let Ok(re) = RegexBuilder::new(query).case_insensitive(true).build() {
        return haystacks
            .iter()
            .enumerate()
            .filter(|(_, h)| re.is_match(h))
            .map(|(i, _)| i)
            .collect();
    }

    let needle = query.to_lowercase();
    haystacks
        .iter()
        .enumerate()
        .filter(|(_, h)| h.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        let hay = vec!["a".to_string(), "b".to_string()];
        assert_eq!(filter_indices("", &hay), vec![0, 1]);
    }

    #[test]
    fn valid_regex_filters() {
        let hay = vec!["gti-vpn".to_string(), "mpd-vpn".to_string(), "netbird-up".to_string()];
        assert_eq!(filter_indices("^gti", &hay), vec![0]);
        assert_eq!(filter_indices("vpn", &hay), vec![0, 1]);
    }

    #[test]
    fn invalid_regex_does_not_panic_and_returns_no_matches() {
        let hay = vec!["gti-vpn".to_string(), "mpd-vpn".to_string()];
        // "(" is an invalid/incomplete regex mid-keystroke; falls back to a
        // literal substring search, which finds nothing here.
        assert_eq!(filter_indices("(", &hay), Vec::<usize>::new());
    }

    #[test]
    fn invalid_regex_falls_back_to_literal_substring_match() {
        // "a(" fails to compile as a regex (unclosed group); the literal
        // substring "a(" is still findable in ordinary text.
        let hay = vec!["a(b".to_string(), "unrelated".to_string()];
        assert_eq!(filter_indices("a(", &hay), vec![0]);
    }

    #[test]
    fn case_insensitive() {
        let hay = vec!["GTI-VPN".to_string()];
        assert_eq!(filter_indices("gti", &hay), vec![0]);
    }
}
