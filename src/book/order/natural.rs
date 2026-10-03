// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Natural ordering of file names: case-insensitive, with digit runs compared as numbers.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// Compare two names naturally, so that `ch2` sorts before `ch10` and `B` next to `b`.
///
/// Names that compare equal that way (`a` and `A`, `1` and `01`) fall back to plain string
/// order, so the result is a total order.
pub(super) fn natural_cmp(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.chars().peekable();
    let mut right_chars = right.chars().peekable();
    loop {
        let ordering = match (left_chars.peek(), right_chars.peek()) {
            (None, None) => return left.cmp(right),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => compare_numbers(
                &take_digits(&mut left_chars),
                &take_digits(&mut right_chars),
            ),
            (Some(l), Some(r)) => {
                let ordering = l.to_lowercase().cmp(r.to_lowercase());
                left_chars.next();
                right_chars.next();
                ordering
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

fn take_digits(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    digits
}

/// Compare two runs of ASCII digits by value, without overflowing on long runs.
fn compare_numbers(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(ToString::to_string).collect();
        names.sort_by(|a, b| natural_cmp(a, b));
        names
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(&["ch10.md", "ch2.md", "ch1.md", "ch02b.md"]),
            vec!["ch1.md", "ch2.md", "ch02b.md", "ch10.md"]
        );
        assert_eq!(
            sorted(&["99999999999999999999999.md", "100000000000000000000000.md"]),
            vec!["99999999999999999999999.md", "100000000000000000000000.md"]
        );
    }

    #[test]
    fn case_is_ignored_but_order_stays_total() {
        assert_eq!(
            sorted(&["beta.md", "Alpha.md", "alpha.md", "Gamma.md"]),
            vec!["Alpha.md", "alpha.md", "beta.md", "Gamma.md"]
        );
        assert_eq!(natural_cmp("a1", "a01"), "a1".cmp("a01"));
    }

    #[test]
    fn dated_names_sort_chronologically() {
        assert_eq!(
            sorted(&["2026-10-03-b.md", "2026-9-30-a.md", "2025-12-01-c.md"]),
            vec!["2025-12-01-c.md", "2026-9-30-a.md", "2026-10-03-b.md"]
        );
    }
}
