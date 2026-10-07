//! Fuzzy filtering for pickers, with Helix's matcher.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

/// An item and its position in the original list.
struct Indexed<'a>(usize, &'a str);

impl AsRef<str> for Indexed<'_> {
    fn as_ref(&self) -> &str {
        self.1
    }
}

/// Indexes of `items` matching `query`, best first; all of them, in order,
/// for an empty query.
pub(crate) fn filter(query: &str, items: &[String]) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    pattern
        .match_list(
            items.iter().enumerate().map(|(i, s)| Indexed(i, s)),
            &mut matcher,
        )
        .into_iter()
        .map(|(item, _)| item.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_path_fragments_best_first() {
        let items = ["/u/code/makima", "/u/code/marimo", "/u/notes"].map(String::from);
        assert_eq!(filter("mak", &items), [0]);
        assert_eq!(filter("code ma", &items).len(), 2);
        assert_eq!(filter("", &items), [0, 1, 2]);
    }
}
