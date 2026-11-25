//! Random password generator.
//!
//! Characters are drawn uniformly from the selected classes with the
//! operating system CSPRNG, using rand's audited uniform sampling and
//! Fisher–Yates shuffle (no hand-written sampling).

use rand::rngs::SysRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{TryCryptoRng, TryRng};
use secrecy::SecretString;
use std::convert::Infallible;
use zeroize::Zeroizing;

const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
/// Symbols accepted by most sites and programs. Quotes, backslash,
/// backtick and space are excluded on purpose: they often break forms,
/// shells and configuration files.
const SYMBOLS: &[u8] = b"!#$%&()*+,-./:;<=>?@[]^_{|}~";
/// Characters easily confused when read or typed by hand.
const AMBIGUOUS: &[u8] = b"Il1|O0o";

/// Errors of the password generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GeneratorError {
    /// Length outside [`PasswordOptions::MIN_LENGTH`]..=[`PasswordOptions::MAX_LENGTH`].
    #[error("the password length is outside the allowed range")]
    InvalidLength,
    /// No character class was selected.
    #[error("at least one character type must be selected")]
    NoCharacterClass,
    /// Fewer characters than required classes.
    #[error("the password is too short to include every selected character type")]
    TooShortForClasses,
    /// The system random number generator failed.
    #[error("the system random number generator is unavailable")]
    RandomUnavailable,
}

/// What kind of password to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordOptions {
    /// Number of characters.
    pub length: usize,
    /// Include `a-z`.
    pub lowercase: bool,
    /// Include `A-Z`.
    pub uppercase: bool,
    /// Include `0-9`.
    pub digits: bool,
    /// Include symbols such as `!#$%&`.
    pub symbols: bool,
    /// Leave out look-alike characters (`I l 1 | O 0 o`).
    pub exclude_ambiguous: bool,
    /// Guarantee at least one character of every selected class.
    pub require_each_class: bool,
}

impl PasswordOptions {
    /// Shortest password allowed (some legacy systems cap passwords at 8).
    pub const MIN_LENGTH: usize = 8;
    /// Longest password allowed.
    pub const MAX_LENGTH: usize = 256;
}

impl Default for PasswordOptions {
    /// 20 characters from all classes: about 130 bits of entropy.
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
            require_each_class: true,
        }
    }
}

/// Character sets of the selected classes, after removing ambiguous
/// characters if requested. Empty classes are dropped.
fn classes(options: &PasswordOptions) -> Vec<Vec<u8>> {
    [
        (options.lowercase, LOWERCASE),
        (options.uppercase, UPPERCASE),
        (options.digits, DIGITS),
        (options.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, set)| {
        set.iter()
            .copied()
            .filter(|c| !(options.exclude_ambiguous && AMBIGUOUS.contains(c)))
            .collect::<Vec<u8>>()
    })
    .filter(|set| !set.is_empty())
    .collect()
}

/// Number of distinct characters the options can produce.
pub fn alphabet_size(options: &PasswordOptions) -> usize {
    classes(options).iter().map(Vec::len).sum()
}

/// Approximate entropy in bits, `length × log2(alphabet size)`, for
/// strength indicators. Slightly overestimates when `require_each_class`
/// is on (the loss is well under one bit for usual settings).
pub fn entropy_bits(options: &PasswordOptions) -> f64 {
    let size = alphabet_size(options);
    if size == 0 {
        return 0.0;
    }
    options.length as f64 * (size as f64).log2()
}

/// Entropy target used when the user does not choose one. A power of two,
/// like the other usual targets (64, 128, 256).
pub const DEFAULT_TARGET_BITS: u32 = 128;

/// Shortest length whose estimated entropy (see [`entropy_bits`]) reaches
/// `target_bits` with the character classes of `options`; its `length`
/// field is ignored.
///
/// When `require_each_class` is on, one extra bit is required to cover
/// the small overestimate of [`entropy_bits`], so the real entropy never
/// falls below the target. The result is at least
/// [`PasswordOptions::MIN_LENGTH`].
pub fn length_for_entropy(
    options: &PasswordOptions,
    target_bits: u32,
) -> Result<usize, GeneratorError> {
    let classes = classes(options);
    let size: usize = classes.iter().map(Vec::len).sum();
    if size == 0 {
        return Err(GeneratorError::NoCharacterClass);
    }
    let bits_per_char = (size as f64).log2();
    if bits_per_char <= 0.0 {
        return Err(GeneratorError::InvalidLength);
    }
    let margin = if options.require_each_class { 1.0 } else { 0.0 };
    let needed = ((f64::from(target_bits) + margin) / bits_per_char).ceil();
    if needed > PasswordOptions::MAX_LENGTH as f64 {
        return Err(GeneratorError::InvalidLength);
    }
    // In range [0, MAX_LENGTH] after the check above.
    let length =
        (needed as usize)
            .max(PasswordOptions::MIN_LENGTH)
            .max(if options.require_each_class {
                classes.len()
            } else {
                0
            });
    Ok(length)
}

/// Adapter that lets rand's infallible sampling run on the fallible OS
/// RNG without panicking: the first error is recorded and zeros are
/// returned from then on. The caller must check [`Self::failed`] and
/// discard the result if it is set.
struct CheckedRng<R: TryRng> {
    inner: R,
    failed: bool,
}

impl<R: TryRng> TryRng for CheckedRng<R> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        let mut buf = [0u8; 4];
        self.try_fill_bytes(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        let mut buf = [0u8; 8];
        self.try_fill_bytes(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        if self.failed || self.inner.try_fill_bytes(dst).is_err() {
            self.failed = true;
            dst.fill(0);
        }
        Ok(())
    }
}

impl<R: TryCryptoRng> TryCryptoRng for CheckedRng<R> {}

/// Generates a random password with the operating system CSPRNG.
pub fn generate(options: &PasswordOptions) -> Result<SecretString, GeneratorError> {
    generate_with(options, SysRng)
}

fn generate_with<R: TryCryptoRng>(
    options: &PasswordOptions,
    source: R,
) -> Result<SecretString, GeneratorError> {
    if !(PasswordOptions::MIN_LENGTH..=PasswordOptions::MAX_LENGTH).contains(&options.length) {
        return Err(GeneratorError::InvalidLength);
    }
    let classes = classes(options);
    if classes.is_empty() {
        return Err(GeneratorError::NoCharacterClass);
    }
    if options.require_each_class && options.length < classes.len() {
        return Err(GeneratorError::TooShortForClasses);
    }
    let alphabet: Vec<u8> = classes.concat();

    let mut rng = CheckedRng {
        inner: source,
        failed: false,
    };
    // Exact capacity: the buffer never reallocates (no unwiped copies),
    // and converting it to a boxed str later needs no shrink either.
    let mut chars = Zeroizing::new(Vec::with_capacity(options.length));
    if options.require_each_class {
        for class in &classes {
            if let Some(&c) = class.choose(&mut rng) {
                chars.push(c);
            }
        }
    }
    while chars.len() < options.length {
        if let Some(&c) = alphabet.choose(&mut rng) {
            chars.push(c);
        }
    }
    chars.shuffle(&mut rng);

    if rng.failed {
        return Err(GeneratorError::RandomUnavailable);
    }
    // ASCII only, so this cannot fail; `take` leaves an empty Vec behind.
    let password = String::from_utf8(std::mem::take(&mut *chars))
        .map_err(|_| GeneratorError::RandomUnavailable)?;
    Ok(SecretString::from(password))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::collections::HashSet;

    fn opts() -> PasswordOptions {
        PasswordOptions::default()
    }

    fn has_any(s: &str, set: &[u8]) -> bool {
        s.bytes().any(|b| set.contains(&b))
    }

    #[test]
    fn default_password_has_every_class() {
        for _ in 0..200 {
            let p = generate(&opts()).unwrap();
            let p = p.expose_secret();
            assert_eq!(p.len(), 20);
            assert!(has_any(p, LOWERCASE));
            assert!(has_any(p, UPPERCASE));
            assert!(has_any(p, DIGITS));
            assert!(has_any(p, SYMBOLS));
        }
    }

    #[test]
    fn length_bounds() {
        for length in [PasswordOptions::MIN_LENGTH, 64, PasswordOptions::MAX_LENGTH] {
            let p = generate(&PasswordOptions { length, ..opts() }).unwrap();
            assert_eq!(p.expose_secret().len(), length);
        }
        for length in [0, 7, PasswordOptions::MAX_LENGTH + 1] {
            assert_eq!(
                generate(&PasswordOptions { length, ..opts() }).unwrap_err(),
                GeneratorError::InvalidLength
            );
        }
    }

    #[test]
    fn only_selected_classes_are_used() {
        let digits_only = PasswordOptions {
            lowercase: false,
            uppercase: false,
            symbols: false,
            ..opts()
        };
        let p = generate(&digits_only).unwrap();
        assert!(p.expose_secret().bytes().all(|b| b.is_ascii_digit()));

        let letters = PasswordOptions {
            digits: false,
            symbols: false,
            length: 128,
            ..opts()
        };
        let p = generate(&letters).unwrap();
        assert!(p.expose_secret().bytes().all(|b| b.is_ascii_alphabetic()));
    }

    #[test]
    fn no_class_is_an_error() {
        let none = PasswordOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..opts()
        };
        assert_eq!(
            generate(&none).unwrap_err(),
            GeneratorError::NoCharacterClass
        );
        assert_eq!(entropy_bits(&none), 0.0);
    }

    #[test]
    fn ambiguous_characters_can_be_excluded() {
        let o = PasswordOptions {
            exclude_ambiguous: true,
            length: 256,
            ..opts()
        };
        for _ in 0..20 {
            assert!(!has_any(generate(&o).unwrap().expose_secret(), AMBIGUOUS));
        }
        assert_eq!(alphabet_size(&o), alphabet_size(&opts()) - AMBIGUOUS.len());
    }

    #[test]
    fn output_alphabet_is_safe_ascii() {
        let p = generate(&PasswordOptions {
            length: 256,
            ..opts()
        })
        .unwrap();
        for b in p.expose_secret().bytes() {
            assert!(b.is_ascii_graphic(), "{b}");
            assert!(!b"\"'`\\".contains(&b), "{b}");
        }
    }

    #[test]
    fn passwords_do_not_repeat() {
        let set: HashSet<String> = (0..1000)
            .map(|_| generate(&opts()).unwrap().expose_secret().to_owned())
            .collect();
        assert_eq!(set.len(), 1000);
    }

    /// Every character of the alphabet appears with roughly equal
    /// frequency (chi-squared test, very loose bound to avoid flakiness).
    #[test]
    fn characters_are_uniformly_distributed() {
        let o = PasswordOptions {
            length: 256,
            require_each_class: false,
            ..opts()
        };
        let alphabet: Vec<u8> = classes(&o).concat();
        let mut counts = [0usize; 256];
        let rounds = 400;
        for _ in 0..rounds {
            for b in generate(&o).unwrap().expose_secret().bytes() {
                counts[b as usize] += 1;
            }
        }
        let total = (rounds * 256) as f64;
        let expected = total / alphabet.len() as f64;
        let chi2: f64 = alphabet
            .iter()
            .map(|&c| {
                let d = counts[c as usize] as f64 - expected;
                d * d / expected
            })
            .sum();
        // 89 degrees of freedom: mean 89, p(chi2 > 200) is astronomically small.
        assert!(chi2 < 200.0, "chi2 = {chi2}");
        assert!(alphabet.iter().all(|&c| counts[c as usize] > 0));
    }

    #[test]
    fn length_for_usual_entropy_targets() {
        // 90 characters: about 6.49 bits each.
        let all = PasswordOptions::default();
        assert_eq!(length_for_entropy(&all, 64).unwrap(), 11);
        assert_eq!(length_for_entropy(&all, DEFAULT_TARGET_BITS).unwrap(), 20);
        assert_eq!(length_for_entropy(&all, 256).unwrap(), 40);
        // Digits only: about 3.32 bits each.
        let digits = PasswordOptions {
            lowercase: false,
            uppercase: false,
            symbols: false,
            ..all
        };
        assert_eq!(length_for_entropy(&digits, 128).unwrap(), 39);
    }

    #[test]
    fn length_for_entropy_is_the_shortest_that_reaches_the_target() {
        let variants = [
            PasswordOptions::default(),
            PasswordOptions {
                symbols: false,
                exclude_ambiguous: true,
                ..PasswordOptions::default()
            },
            PasswordOptions {
                uppercase: false,
                digits: false,
                symbols: false,
                require_each_class: false,
                ..PasswordOptions::default()
            },
        ];
        for base in variants {
            for target in [32, 64, 100, 128, 200, 256, 512] {
                let length = length_for_entropy(&base, target).unwrap();
                let margin = if base.require_each_class { 1.0 } else { 0.0 };
                let goal = f64::from(target) + margin;
                let with = |n| entropy_bits(&PasswordOptions { length: n, ..base });
                assert!(with(length) >= goal, "{target} bits, length {length}");
                assert!(
                    length == PasswordOptions::MIN_LENGTH || with(length - 1) < goal,
                    "{target} bits: {length} is not the shortest"
                );
                // And the generator accepts it.
                let pw = generate(&PasswordOptions { length, ..base }).unwrap();
                assert_eq!(pw.expose_secret().len(), length);
            }
        }
    }

    #[test]
    fn length_for_entropy_limits() {
        let all = PasswordOptions::default();
        // Tiny targets still give the minimum length.
        assert_eq!(
            length_for_entropy(&all, 1).unwrap(),
            PasswordOptions::MIN_LENGTH
        );
        // Unreachable targets are an error, not a truncated password.
        assert_eq!(
            length_for_entropy(&all, 4096).unwrap_err(),
            GeneratorError::InvalidLength
        );
        let none = PasswordOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..all
        };
        assert_eq!(
            length_for_entropy(&none, 128).unwrap_err(),
            GeneratorError::NoCharacterClass
        );
    }

    #[test]
    fn entropy_estimate() {
        // 26 + 26 + 10 + 28 symbols = 90: log2(90) ≈ 6.49 bits per character.
        assert_eq!(SYMBOLS.len(), 28);
        assert_eq!(alphabet_size(&opts()), 90);
        let bits = entropy_bits(&opts());
        assert!((bits - 20.0 * 90f64.log2()).abs() < 1e-9);
        assert!(bits > 128.0);
    }

    /// A source that fails after `ok_bytes` bytes.
    struct FailingRng {
        ok_bytes: usize,
    }
    impl TryRng for FailingRng {
        type Error = std::io::Error;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            let mut b = [0; 4];
            self.try_fill_bytes(&mut b)?;
            Ok(u32::from_le_bytes(b))
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            let mut b = [0; 8];
            self.try_fill_bytes(&mut b)?;
            Ok(u64::from_le_bytes(b))
        }
        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
            if self.ok_bytes < dst.len() {
                return Err(std::io::Error::other("rng failure"));
            }
            self.ok_bytes -= dst.len();
            dst.fill(7);
            Ok(())
        }
    }
    impl TryCryptoRng for FailingRng {}

    #[test]
    fn rng_failure_is_reported_not_hidden() {
        for ok_bytes in [0, 4, 40] {
            assert_eq!(
                generate_with(&opts(), FailingRng { ok_bytes }).unwrap_err(),
                GeneratorError::RandomUnavailable,
                "fails after {ok_bytes} bytes"
            );
        }
    }

    #[test]
    fn debug_output_redacts_password() {
        let p = generate(&opts()).unwrap();
        let shown = format!("{p:?}");
        assert!(!shown.contains(p.expose_secret()));
    }
}
