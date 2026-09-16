//! Which of a component's versions is the latest.
//!
//! Antora does not require versions to be semver, and plenty of real ones are
//! not — `2.0`, `v3`, `1.0.0-beta.2`, `next`. So ordering is done the way a
//! person reads a version rather than the way a parser does: the string is
//! split into runs of digits and runs of everything else, and digit runs are
//! compared as numbers so `10` sorts after `9`.
//!
//! The *latest* version is then the greatest one that is not a prerelease. A
//! prerelease is declared in the component descriptor rather than inferred, so
//! a `1.0.0-beta` that nobody marked is an ordinary version and will be
//! published as the latest if it is the greatest — which is Antora's behavior
//! and is the reason `prerelease:` exists.

use std::cmp::Ordering;

/// Compare two version strings the way a reader would.
///
/// An empty version — an unversioned component — is greater than every other,
/// so a component that is partly unversioned still resolves to the
/// unversioned one. In practice a component is either versioned or not, and
/// this only decides a case that should not arise.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    let mut left = segments(a);
    let mut right = segments(b);

    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,

            // `1.0` and `1.0.1` share a prefix, and the one that continues is
            // the greater — except that a *prerelease* suffix makes it lesser,
            // which is why a trailing non-numeric run sorts below nothing.
            (Some(Segment::Text(_)), None) | (None, Some(Segment::Number(_))) => {
                return Ordering::Less;
            }

            (None, Some(Segment::Text(_))) | (Some(Segment::Number(_)), None) => {
                return Ordering::Greater;
            }

            (Some(left), Some(right)) => match left.cmp(&right) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
}

/// One run of a version string: digits read as a number, anything else as
/// text.
#[derive(Debug, Eq, PartialEq)]
enum Segment<'a> {
    Number(u64),
    Text(&'a str),
}

impl Ord for Segment<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),

            // A number outranks text at the same position: `2` is a later
            // version than `2-rc`, and `next` is not a number at all.
            (Self::Number(_), Self::Text(_)) => Ordering::Greater,
            (Self::Text(_), Self::Number(_)) => Ordering::Less,
        }
    }
}

impl PartialOrd for Segment<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Split a version into its runs, dropping the separators between them.
fn segments(version: &str) -> impl Iterator<Item = Segment<'_>> {
    let mut rest = version;

    std::iter::from_fn(move || {
        // Separators carry no ordering of their own; `1.0`, `1-0` and `1_0`
        // are the same sequence of runs.
        rest = rest.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());

        if rest.is_empty() {
            return None;
        }

        let digits = rest.starts_with(|c: char| c.is_ascii_digit());

        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() || c.is_ascii_digit() != digits)
            .unwrap_or(rest.len());

        let (head, tail) = rest.split_at(end);
        rest = tail;

        Some(if digits {
            // A version segment longer than a `u64` is not a version anyone
            // typed; treating it as text keeps the comparison total.
            head.parse().map_or(Segment::Text(head), Segment::Number)
        } else {
            Segment::Text(head)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn newer(a: &str, b: &str) {
        assert_eq!(compare(a, b), Ordering::Greater, "{a} should outrank {b}");
        assert_eq!(compare(b, a), Ordering::Less, "{b} should rank below {a}");
    }

    #[test]
    fn numbers_compare_as_numbers() {
        newer("10.0", "9.0");
        newer("2.10", "2.9");
    }

    #[test]
    fn a_longer_version_is_newer() {
        newer("1.0.1", "1.0");
    }

    #[test]
    fn a_prerelease_suffix_ranks_below_the_release() {
        newer("1.0.0", "1.0.0-beta");
        newer("2.0", "2.0-rc.1");
    }

    #[test]
    fn separators_do_not_matter() {
        assert_eq!(compare("1.0", "1-0"), Ordering::Equal);
    }

    #[test]
    fn a_leading_v_is_text_and_compares_as_text() {
        assert_eq!(compare("v2.0", "v2.0"), Ordering::Equal);
        newer("v2.0", "v1.0");
    }

    #[test]
    fn unversioned_outranks_everything() {
        newer("", "9.9.9");
    }

    #[test]
    fn a_name_ranks_below_a_number() {
        newer("1.0", "next");
    }
}
