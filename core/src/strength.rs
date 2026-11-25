//! Strength estimate for passwords typed by the user.
//!
//! Generated passwords have an exact entropy (see `generator`); typed
//! ones do not, and counting character classes would hugely overrate
//! things like `Password123!`. This module combines two views:
//!
//! - **Patterns** (common passwords, dictionary words, names, dates,
//!   keyboard rows, repeats, sequences, data of the entry itself) are
//!   found by `zxcvbn` and cost what it estimates for each one.
//! - **Random stretches** between patterns cost their real entropy:
//!   length × log2(size of the character pool the password uses).
//!
//! zxcvbn alone is not enough: it caps estimates at 2^64 guesses and
//! models random characters as digits, so a random 20-character password
//! would read as ~64 bits instead of ~130. The result is an estimate for
//! a strength bar, not a guarantee.

use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use zxcvbn::feedback::Warning;
use zxcvbn::matching::patterns::MatchPattern;

/// zxcvbn only analyses this many characters (protection against very
/// long inputs). The rest count as random characters.
pub const MAX_ANALYZED_CHARS: usize = 100;

/// Main weakness found, if any, as a stable code for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrengthWarning {
    /// Contains the entry name, user name or site.
    ContainsEntryData,
    /// A common password or very similar to one.
    CommonPassword,
    /// Rows or short patterns of the keyboard (`qwerty`, `asdf`).
    KeyboardPattern,
    /// Repeated characters or blocks (`aaa`, `abcabc`).
    Repeats,
    /// Sequences (`abc`, `1234`).
    Sequence,
    /// Dates or recent years.
    Dates,
    /// A single word.
    SingleWord,
    /// First names or surnames.
    Names,
}

/// Estimated strength of a typed password.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrengthEstimate {
    /// Estimated bits (log2 of the guesses needed).
    pub bits: f64,
    /// Main weakness, if any.
    pub warning: Option<StrengthWarning>,
}

/// Size of the character pool the password draws from: each class used
/// counts whole, as an attacker would have to try all of it.
fn pool_size(password: &str) -> f64 {
    let has = |f: fn(char) -> bool| password.chars().any(f);
    let mut size = 0.0;
    if has(|c| c.is_ascii_lowercase()) {
        size += 26.0;
    }
    if has(|c| c.is_ascii_uppercase()) {
        size += 26.0;
    }
    if has(|c| c.is_ascii_digit()) {
        size += 10.0;
    }
    if has(|c| c.is_ascii() && !c.is_ascii_alphanumeric()) {
        size += 33.0;
    }
    if has(|c| !c.is_ascii()) {
        // Accented letters, ñ, other scripts: a rough, conservative pool.
        size += 100.0;
    }
    size
}

fn map_warning(warning: Warning) -> StrengthWarning {
    match warning {
        Warning::StraightRowsOfKeysAreEasyToGuess
        | Warning::ShortKeyboardPatternsAreEasyToGuess => StrengthWarning::KeyboardPattern,
        Warning::RepeatsLikeAaaAreEasyToGuess
        | Warning::RepeatsLikeAbcAbcAreOnlySlightlyHarderToGuess => StrengthWarning::Repeats,
        Warning::ThisIsATop10Password
        | Warning::ThisIsATop100Password
        | Warning::ThisIsACommonPassword
        | Warning::ThisIsSimilarToACommonlyUsedPassword => StrengthWarning::CommonPassword,
        Warning::SequencesLikeAbcAreEasyToGuess => StrengthWarning::Sequence,
        Warning::RecentYearsAreEasyToGuess | Warning::DatesAreOftenEasyToGuess => {
            StrengthWarning::Dates
        }
        Warning::AWordByItselfIsEasyToGuess => StrengthWarning::SingleWord,
        Warning::NamesAndSurnamesByThemselvesAreEasyToGuess
        | Warning::CommonNamesAndSurnamesAreEasyToGuess => StrengthWarning::Names,
    }
}

/// Segments longer than this are split when zxcvbn saturates on them.
const SPLIT_ABOVE_CHARS: usize = 12;

/// Bits of one segment: patterns cost what zxcvbn estimates (never more
/// than the same text as random characters); random stretches cost
/// length × `bits_per_char`.
///
/// zxcvbn counts guesses in a `u64`. Once a segment needs more than 2^64
/// guesses it stops looking for patterns and treats long stretches as
/// random (so a passphrase would look like random text). In that case
/// the segment is split at the separator closest to its middle (or at
/// the middle) and each half is analysed on its own.
fn segment_bits(
    chars: &[char],
    context: &[&str],
    bits_per_char: f64,
    uses_entry_data: &mut bool,
    warning: &mut Option<Warning>,
) -> f64 {
    let text: String = chars.iter().collect();
    let result = zxcvbn::zxcvbn(&text, context);
    let saturated = result.guesses() == u64::MAX;
    if saturated && chars.len() > SPLIT_ABOVE_CHARS {
        let middle = chars.len() / 2;
        let split = (1..chars.len())
            .filter(|&i| !chars[i].is_alphanumeric())
            .min_by_key(|&i| i.abs_diff(middle))
            .filter(|&i| i.abs_diff(middle) <= middle / 2)
            .unwrap_or(middle);
        let (a, b) = chars.split_at(split);
        return segment_bits(a, context, bits_per_char, uses_entry_data, warning)
            + segment_bits(b, context, bits_per_char, uses_entry_data, warning);
    }

    if warning.is_none() {
        *warning = result.feedback().and_then(|f| f.warning());
    }
    let mut bits = 0.0;
    for m in result.sequence() {
        let random_bits = m.token.chars().count() as f64 * bits_per_char;
        match &m.pattern {
            MatchPattern::BruteForce => bits += random_bits,
            pattern => {
                // The dictionary type is private in zxcvbn: compare the
                // matched word with the context words instead.
                if let MatchPattern::Dictionary(d) = pattern
                    && context.contains(&d.matched_word.as_str())
                {
                    *uses_entry_data = true;
                }
                let pattern_bits = (m.guesses.unwrap_or(1).max(1) as f64).log2();
                bits += pattern_bits.min(random_bits);
            }
        }
    }
    bits
}

/// Estimates the strength of `password`. `context` holds words of the
/// entry (name, user name, site...): using them makes a password weak.
/// Words shorter than 3 characters are ignored.
pub fn estimate(password: &SecretString, context: &[&str]) -> StrengthEstimate {
    let text = password.expose_secret();
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return StrengthEstimate {
            bits: 0.0,
            warning: None,
        };
    }
    let bits_per_char = pool_size(text).log2().max(0.0);
    let context: Vec<String> = context
        .iter()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.chars().count() >= 3)
        .collect();
    let context: Vec<&str> = context.iter().map(String::as_str).collect();

    let analyzed = chars.len().min(MAX_ANALYZED_CHARS);
    let mut uses_entry_data = false;
    let mut warning = None;
    let mut bits = segment_bits(
        &chars[..analyzed],
        &context,
        bits_per_char,
        &mut uses_entry_data,
        &mut warning,
    );
    bits += (chars.len() - analyzed) as f64 * bits_per_char;

    let warning = if uses_entry_data {
        Some(StrengthWarning::ContainsEntryData)
    } else {
        warning.map(map_warning)
    };
    StrengthEstimate { bits, warning }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{self, PasswordOptions};

    fn est(password: &str) -> StrengthEstimate {
        estimate(&SecretString::from(password), &[])
    }

    #[test]
    fn empty_password_has_no_strength() {
        assert_eq!(est("").bits, 0.0);
        assert_eq!(est("").warning, None);
    }

    #[test]
    fn common_passwords_are_very_weak() {
        for pw in [
            "123456",
            "password",
            "qwerty",
            "Password123!",
            "Barcelona2024",
        ] {
            let e = est(pw);
            assert!(e.bits < 30.0, "{pw}: {}", e.bits);
            assert!(e.warning.is_some(), "{pw}");
        }
        assert_eq!(
            est("password").warning,
            Some(StrengthWarning::CommonPassword)
        );
    }

    #[test]
    fn random_passwords_get_their_real_entropy() {
        for target in [64, 128, 256] {
            let options = PasswordOptions {
                length: generator::length_for_entropy(&PasswordOptions::default(), target).unwrap(),
                ..PasswordOptions::default()
            };
            let exact = generator::entropy_bits(&options);
            let pw = generator::generate(&options).unwrap();
            let e = estimate(&pw, &[]);
            // Random text can contain short accidental "patterns": allow
            // some loss, but far above zxcvbn's 64-bit ceiling.
            assert!(e.bits >= exact * 0.8, "{target}: {} < {exact}", e.bits);
            // The estimator does not know the generator's alphabet (90
            // characters) and counts all 95 printable ASCII characters.
            let max = options.length as f64 * 95f64.log2();
            assert!(e.bits <= max + 0.001, "{target}: {} > {max}", e.bits);
        }
    }

    #[test]
    fn passphrases_rank_between() {
        // Four common words: far below the same length of random text.
        let e = est("correct horse battery staple");
        assert!(e.bits > 40.0 && e.bits < 100.0, "{}", e.bits);
        let random = est("kQ7#mZ2!pL9@xW4$tR8&bH3*nJ6(cV1");
        assert!(random.bits > e.bits + 40.0, "{}", random.bits);
    }

    #[test]
    fn entry_data_makes_a_password_weak() {
        let pw = SecretString::from("juanperez1990");
        let with = estimate(&pw, &["juanperez", "example.com", "x"]);
        let without = estimate(&pw, &[]);
        assert_eq!(with.warning, Some(StrengthWarning::ContainsEntryData));
        assert!(with.bits < without.bits);
    }

    #[test]
    fn characters_beyond_the_analysed_part_still_count() {
        let short: String = "aG7!kP2#qZ9$wM4%tR8&".repeat(5); // 100 chars
        let long = format!("{short}xY3!");
        assert!(est(&long).bits > est(&short).bits);
    }

    #[test]
    fn more_variety_and_length_never_hurt_random_text() {
        assert!(est("kq7mz2pl9xw4").bits < est("kQ7#mZ2!pL9@xW4$").bits);
        assert!(est("kQ7#mZ2!").bits < est("kQ7#mZ2!pL9@xW4$").bits);
    }

    #[test]
    fn non_ascii_passwords_are_handled() {
        let e = est("ñandú-pingüino-cigüeña");
        assert!(e.bits > 0.0 && e.bits.is_finite());
    }

    #[test]
    fn warnings_serialize_as_codes() {
        let json = serde_json::to_string(&StrengthWarning::ContainsEntryData).unwrap();
        assert_eq!(json, r#""containsEntryData""#);
    }
}
