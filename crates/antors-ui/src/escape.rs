//! Escaping text on its way into markup.
//!
//! Almost everything the shell emits is either already-rendered markup — an
//! article, a navigation entry's inline `AsciiDoc` — or a value that came from
//! a configuration file. The first must not be escaped and the second must be,
//! and the two are told apart by which function is called, so a reader of this
//! crate can see at each site which one it is.

/// Escape text for a text node.
pub fn text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }

    escaped
}

/// Escape text for a double-quoted attribute value.
pub fn attr(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }

    escaped
}

/// Strip the tags from rendered markup, for somewhere only text may go.
///
/// A page title may carry `<code>` or `<em>`, and the browser's tab and a
/// `<meta>` description can hold neither. Removing the tags keeps the words,
/// which is the part that was worth having.
pub fn detag(html: &str) -> String {
    let mut plain = String::with_capacity(html.len());
    let mut depth = 0_usize;

    for character in html.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            other if depth == 0 => plain.push(other),
            _ => {}
        }
    }

    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_the_three_that_matter() {
        assert_eq!(text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }

    #[test]
    fn text_leaves_quotes_alone() {
        // A quote in a text node is a quote, and escaping it there only makes
        // the source harder to read.
        assert_eq!(text(r#"say "hello""#), r#"say "hello""#);
    }

    #[test]
    fn attr_escapes_the_quote_too() {
        assert_eq!(attr(r#"say "hello""#), "say &quot;hello&quot;");
    }

    #[test]
    fn detag_keeps_the_words() {
        assert_eq!(detag("A <code>page</code> title"), "A page title");
    }

    #[test]
    fn detag_treats_a_stray_bracket_as_a_tag() {
        // The input is always rendered markup, where a literal `<` has already
        // become `&lt;` — so an unmatched one is an opening tag that was never
        // closed, and everything after it is markup rather than words.
        assert_eq!(detag("a < b"), "a ");
        assert_eq!(detag("a &lt; b"), "a &lt; b");
    }
}
