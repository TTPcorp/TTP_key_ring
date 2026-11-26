//! Auto-type: what to type into a program window and which entries
//! belong to that window.
//!
//! This module is pure logic. It parses the auto-type sequence of an
//! entry into steps (text, special keys, pauses) and decides whether an
//! entry is associated with a given window. Pressing the keys, reading
//! the active window and the global hotkey are OS-specific and live in
//! `llavero-platform`.
//!
//! # Sequence syntax
//!
//! Literal text is typed as is. Placeholders go between braces and their
//! names are case-insensitive:
//!
//! | Placeholder   | Meaning                                   |
//! |---------------|-------------------------------------------|
//! | `{USERNAME}`  | the entry's user name                     |
//! | `{PASSWORD}`  | the entry's password                      |
//! | `{TAB}`       | the Tab key                               |
//! | `{ENTER}`     | the Enter key                             |
//! | `{DELAY n}`   | wait `n` milliseconds (1..=[`MAX_DELAY_MS`]) |
//! | `{{}` / `{}}` | a literal `{` / `}` (as in KeePass)       |
//!
//! Anything else (an unknown placeholder, a stray `}`, an unclosed `{`)
//! is rejected: typing a misspelt placeholder literally into a login form
//! would be worse than refusing.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::model::{Entry, VaultData, WindowMatch};

/// Sequence used when an entry has none of its own.
pub const DEFAULT_SEQUENCE: &str = "{USERNAME}{TAB}{PASSWORD}{ENTER}";

/// Longest accepted sequence, in characters.
pub const MAX_SEQUENCE_CHARS: usize = 1000;

/// Longest single `{DELAY n}`, in milliseconds.
pub const MAX_DELAY_MS: u32 = 10_000;

/// Why a sequence is rejected. Positions are character offsets of the
/// offending `{` or `}`; the sequence text itself is never included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SequenceError {
    /// The sequence is empty or only whitespace.
    #[error("the auto-type sequence is empty")]
    Empty,
    /// More than [`MAX_SEQUENCE_CHARS`] characters.
    #[error("the auto-type sequence is too long")]
    TooLong,
    /// A `{` without its closing `}`.
    #[error("unclosed brace in the auto-type sequence at position {0}")]
    UnclosedBrace(usize),
    /// A `}` with no `{` before it (write `{}}` for a literal one).
    #[error("unexpected closing brace in the auto-type sequence at position {0}")]
    UnexpectedClosingBrace(usize),
    /// A placeholder that does not exist, or with an argument it does not take.
    #[error("unknown placeholder in the auto-type sequence at position {0}")]
    UnknownPlaceholder(usize),
    /// `{DELAY}` without a number, with zero, or above [`MAX_DELAY_MS`].
    #[error("invalid delay in the auto-type sequence at position {0}")]
    InvalidDelay(usize),
}

/// A special key that auto-type can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Text(String),
    Username,
    Password,
    Key(Key),
    Delay(u32),
}

/// A parsed, valid auto-type sequence. It holds no secrets: user name and
/// password are only filled in by [`Sequence::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    tokens: Vec<Token>,
}

/// One step to perform, in order.
#[derive(Debug)]
pub enum Step {
    /// Type this text. Wrapped as a secret because it may be the
    /// password; it is wiped on drop and redacted in `Debug` output.
    Text(SecretString),
    /// Press and release a special key.
    Key(Key),
    /// Wait before the next step.
    Delay(Duration),
}

impl Sequence {
    /// Parses and validates a sequence.
    pub fn parse(text: &str) -> Result<Self, SequenceError> {
        if text.trim().is_empty() {
            return Err(SequenceError::Empty);
        }
        if text.chars().count() > MAX_SEQUENCE_CHARS {
            return Err(SequenceError::TooLong);
        }

        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while let Some(&c) = chars.get(i) {
            match c {
                '{' => {
                    let start = i;
                    // `{}}` is a literal `}`: the only placeholder whose
                    // content is itself a closing brace.
                    if chars.get(i + 1) == Some(&'}') && chars.get(i + 2) == Some(&'}') {
                        literal.push('}');
                        i += 3;
                        continue;
                    }
                    let close = chars
                        .iter()
                        .skip(i + 1)
                        .position(|&ch| ch == '}')
                        .map(|offset| i + 1 + offset)
                        .ok_or(SequenceError::UnclosedBrace(start))?;
                    let content: String =
                        chars.get(i + 1..close).unwrap_or_default().iter().collect();
                    i = close + 1;
                    match parse_placeholder(&content, start)? {
                        Placeholder::Literal(ch) => literal.push(ch),
                        Placeholder::Token(token) => {
                            if !literal.is_empty() {
                                tokens.push(Token::Text(std::mem::take(&mut literal)));
                            }
                            tokens.push(token);
                        }
                    }
                }
                '}' => return Err(SequenceError::UnexpectedClosingBrace(i)),
                _ => {
                    literal.push(c);
                    i += 1;
                }
            }
        }
        if !literal.is_empty() {
            tokens.push(Token::Text(literal));
        }
        Ok(Self { tokens })
    }

    /// The default sequence, `{USERNAME}{TAB}{PASSWORD}{ENTER}`.
    pub fn default_sequence() -> Self {
        Self {
            tokens: vec![
                Token::Username,
                Token::Key(Key::Tab),
                Token::Password,
                Token::Key(Key::Enter),
            ],
        }
    }

    /// The entry's own sequence, or the default one if it has none (or
    /// only whitespace).
    pub fn for_entry(entry: &Entry) -> Result<Self, SequenceError> {
        match entry.autotype() {
            Some(text) if !text.trim().is_empty() => Self::parse(text),
            _ => Ok(Self::default_sequence()),
        }
    }

    /// Whether the sequence types the password.
    pub fn uses_password(&self) -> bool {
        self.tokens.contains(&Token::Password)
    }

    /// Steps to perform for `entry`. Empty user names or passwords
    /// produce no step.
    pub fn resolve(&self, entry: &Entry) -> Vec<Step> {
        self.tokens
            .iter()
            .filter_map(|token| match token {
                Token::Text(text) => Some(Step::Text(SecretString::from(text.as_str()))),
                Token::Username => non_empty(entry.username()),
                Token::Password => non_empty(entry.password().expose_secret()),
                Token::Key(key) => Some(Step::Key(*key)),
                Token::Delay(ms) => Some(Step::Delay(Duration::from_millis(u64::from(*ms)))),
            })
            .collect()
    }
}

fn non_empty(text: &str) -> Option<Step> {
    (!text.is_empty()).then(|| Step::Text(SecretString::from(text)))
}

enum Placeholder {
    Literal(char),
    Token(Token),
}

fn parse_placeholder(content: &str, position: usize) -> Result<Placeholder, SequenceError> {
    if content == "{" {
        return Ok(Placeholder::Literal('{'));
    }
    let (name, argument) = match content.split_once(' ') {
        Some((name, argument)) => (name, Some(argument.trim())),
        None => (content, None),
    };
    let token = match (name.to_ascii_uppercase().as_str(), argument) {
        ("USERNAME", None) => Token::Username,
        ("PASSWORD", None) => Token::Password,
        ("TAB", None) => Token::Key(Key::Tab),
        ("ENTER", None) => Token::Key(Key::Enter),
        ("DELAY", Some(ms)) => Token::Delay(parse_delay(ms, position)?),
        ("DELAY", None) => return Err(SequenceError::InvalidDelay(position)),
        _ => return Err(SequenceError::UnknownPlaceholder(position)),
    };
    Ok(Placeholder::Token(token))
}

fn parse_delay(text: &str, position: usize) -> Result<u32, SequenceError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(SequenceError::InvalidDelay(position));
    }
    match text.parse::<u32>() {
        Ok(ms) if (1..=MAX_DELAY_MS).contains(&ms) => Ok(ms),
        _ => Err(SequenceError::InvalidDelay(position)),
    }
}

/// The window auto-type would type into, as reported by the OS.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetWindow {
    /// Window title.
    pub title: String,
    /// Executable file name (e.g. `program.exe`), if known.
    pub process: Option<String>,
}

impl WindowMatch {
    /// Whether this association applies to `window`.
    ///
    /// - The title is compared ignoring case; `*` matches any run of
    ///   characters. Without `*` the whole title must be equal.
    /// - The process is compared ignoring case against the executable
    ///   file name only (any folder in the pattern is ignored).
    /// - A blank field matches any value, but an association with both
    ///   fields blank matches **no** window: it must never send a
    ///   password to whatever window happens to be active.
    pub fn matches(&self, window: &TargetWindow) -> bool {
        let title = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let process = self
            .process
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        if title.is_none() && process.is_none() {
            return false;
        }
        let title_ok = title.is_none_or(|pattern| wildcard_match(pattern, &window.title));
        let process_ok = process.is_none_or(|pattern| {
            window.process.as_deref().is_some_and(|actual| {
                file_name(pattern).to_lowercase() == file_name(actual).to_lowercase()
            })
        });
        title_ok && process_ok
    }
}

/// Last component of a Windows or Unix path.
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Case-insensitive match where `*` stands for any run of characters.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    // Classic greedy matcher with backtracking to the last `*`.
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if text.get(t) == Some(&c) => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern.iter().skip(p).all(|&c| c == '*')
}

impl WindowMatch {
    /// Whether this association names an executable. A window title can be
    /// set by any program or web page; the executable cannot be faked that
    /// easily, so only associations that check it allow typing without
    /// asking first.
    pub fn checks_process(&self) -> bool {
        self.process
            .as_deref()
            .is_some_and(|p| !p.trim().is_empty())
    }
}

/// What auto-type should do for the active window.
#[derive(Debug)]
pub enum AutoTypeChoice<'a> {
    /// No entry is associated with the window: type nothing.
    NoMatch,
    /// Exactly one entry matches, through an association that checks the
    /// executable: type it without asking (decided by the project owner,
    /// 2026-11-26).
    TypeDirectly(&'a Entry),
    /// Several entries match, or the only match relies on the window
    /// title alone: the user must pick (or confirm) the entry first.
    Choose(Vec<&'a Entry>),
}

impl VaultData {
    /// Decides between typing directly and asking the user first.
    pub fn autotype_choice<'a>(&'a self, window: &'a TargetWindow) -> AutoTypeChoice<'a> {
        let entries: Vec<&Entry> = self.entries_for_window(window).collect();
        match entries.as_slice() {
            [] => AutoTypeChoice::NoMatch,
            [entry]
                if entry
                    .windows()
                    .iter()
                    .any(|w| w.checks_process() && w.matches(window)) =>
            {
                AutoTypeChoice::TypeDirectly(entry)
            }
            _ => AutoTypeChoice::Choose(entries),
        }
    }

    /// Entries associated with `window`, in vault order.
    pub fn entries_for_window<'a>(
        &'a self,
        window: &'a TargetWindow,
    ) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries()
            .iter()
            .filter(move |entry| entry.windows().iter().any(|w| w.matches(window)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryInput;

    fn entry_with(
        username: &str,
        password: &str,
        autotype: Option<&str>,
    ) -> (VaultData, uuid::Uuid) {
        let mut vault = VaultData::new().unwrap();
        let mut input = EntryInput::new("Test", username, SecretString::from(password));
        input.autotype = autotype.map(str::to_owned);
        let id = vault.add_entry(input).unwrap();
        (vault, id)
    }

    /// Renders steps for comparison: text as is, keys and delays in braces.
    fn render(steps: &[Step]) -> String {
        steps
            .iter()
            .map(|step| match step {
                Step::Text(text) => text.expose_secret().to_owned(),
                Step::Key(Key::Tab) => "<TAB>".to_owned(),
                Step::Key(Key::Enter) => "<ENTER>".to_owned(),
                Step::Delay(d) => format!("<{}ms>", d.as_millis()),
            })
            .collect()
    }

    #[test]
    fn default_sequence_matches_its_text() {
        assert_eq!(
            Sequence::parse(DEFAULT_SEQUENCE).unwrap(),
            Sequence::default_sequence()
        );
    }

    #[test]
    fn entry_without_sequence_uses_the_default() {
        for autotype in [None, Some(""), Some("   ")] {
            let (vault, id) = entry_with("ana", "s3cret", autotype);
            let entry = vault.entry(id).unwrap();
            let steps = Sequence::for_entry(entry).unwrap().resolve(entry);
            assert_eq!(render(&steps), "ana<TAB>s3cret<ENTER>");
        }
    }

    #[test]
    fn custom_sequence_with_text_delay_and_any_case() {
        let (vault, id) = entry_with(
            "ana",
            "pw",
            Some("{username}x{Tab}{DELAY 250}{PASSWORD}{enter}"),
        );
        let entry = vault.entry(id).unwrap();
        let steps = Sequence::for_entry(entry).unwrap().resolve(entry);
        assert_eq!(render(&steps), "anax<TAB><250ms>pw<ENTER>");
    }

    #[test]
    fn literal_braces_and_unicode_text() {
        let sequence = Sequence::parse("{{}ñ{}}€").unwrap();
        let (vault, id) = entry_with("u", "p", None);
        let steps = sequence.resolve(vault.entry(id).unwrap());
        assert_eq!(render(&steps), "{ñ}€");
        assert_eq!(steps.len(), 1, "adjacent literals are typed in one step");
    }

    #[test]
    fn empty_fields_produce_no_steps() {
        let (vault, id) = entry_with("", "", None);
        let entry = vault.entry(id).unwrap();
        let steps = Sequence::for_entry(entry).unwrap().resolve(entry);
        assert_eq!(render(&steps), "<TAB><ENTER>");
    }

    #[test]
    fn uses_password_is_reported() {
        assert!(Sequence::default_sequence().uses_password());
        assert!(
            !Sequence::parse("{USERNAME}{ENTER}")
                .unwrap()
                .uses_password()
        );
    }

    #[test]
    fn invalid_sequences_are_rejected_with_position() {
        use SequenceError::*;
        let cases = [
            ("", Empty),
            ("  ", Empty),
            ("ab{USERNAME", UnclosedBrace(2)),
            ("{", UnclosedBrace(0)),
            ("a}b", UnexpectedClosingBrace(1)),
            ("{USERNAME}}", UnexpectedClosingBrace(10)),
            ("x{USER}", UnknownPlaceholder(1)),
            ("{}", UnknownPlaceholder(0)),
            ("{TAB 2}", UnknownPlaceholder(0)),
            ("{ENTER}{DELAY}", InvalidDelay(7)),
            ("{DELAY 0}", InvalidDelay(0)),
            ("{DELAY -5}", InvalidDelay(0)),
            ("{DELAY 10001}", InvalidDelay(0)),
            ("{DELAY 99999999999}", InvalidDelay(0)),
            ("{DELAY 1.5}", InvalidDelay(0)),
        ];
        for (text, expected) in cases {
            assert_eq!(Sequence::parse(text), Err(expected), "sequence {text:?}");
        }
        assert!(Sequence::parse(&format!("{{DELAY {MAX_DELAY_MS}}}")).is_ok());
    }

    #[test]
    fn length_limit_counts_characters() {
        assert!(Sequence::parse(&"ñ".repeat(MAX_SEQUENCE_CHARS)).is_ok());
        assert_eq!(
            Sequence::parse(&"a".repeat(MAX_SEQUENCE_CHARS + 1)),
            Err(SequenceError::TooLong)
        );
    }

    #[test]
    fn errors_and_debug_never_show_secrets() {
        let (vault, id) = entry_with(
            "ana",
            "very-secret-pw",
            Some("{PASSWORD}{OOPS very-secret-pw}"),
        );
        let entry = vault.entry(id).unwrap();
        let err = Sequence::for_entry(entry).unwrap_err();
        assert!(!format!("{err} {err:?}").contains("very-secret-pw"));

        let steps = Sequence::default_sequence().resolve(entry);
        assert!(!format!("{steps:?}").contains("very-secret-pw"));
    }

    fn window(title: &str, process: Option<&str>) -> TargetWindow {
        TargetWindow {
            title: title.to_owned(),
            process: process.map(str::to_owned),
        }
    }

    fn matcher(title: Option<&str>, process: Option<&str>) -> WindowMatch {
        WindowMatch {
            title: title.map(str::to_owned),
            process: process.map(str::to_owned),
        }
    }

    #[test]
    fn title_matching_is_whole_and_case_insensitive() {
        let m = matcher(Some("Ejemplo - Login"), None);
        assert!(m.matches(&window("EJEMPLO - login", None)));
        assert!(!m.matches(&window("Ejemplo - Login (copia)", None)));
        assert!(!m.matches(&window("Otro", None)));
    }

    #[test]
    fn title_wildcards() {
        let m = matcher(Some("*Banco*Acceso"), None);
        assert!(m.matches(&window("Mi Banco - Acceso", None)));
        assert!(m.matches(&window("banco acceso", None)));
        assert!(!m.matches(&window("Mi Banco - Acceso seguro", None)));
        assert!(matcher(Some("*"), Some("a.exe")).matches(&window("", Some("a.exe"))));
        assert!(matcher(Some("a*b*c"), None).matches(&window("aXbYbZc", None)));
        assert!(!matcher(Some("a*b*c"), None).matches(&window("aXcYb", None)));
    }

    #[test]
    fn process_matches_file_name_only() {
        let m = matcher(None, Some("Ejemplo.EXE"));
        assert!(m.matches(&window("Anything", Some("ejemplo.exe"))));
        assert!(m.matches(&window(
            "Anything",
            Some(r"C:\Program Files\Ejemplo\ejemplo.exe")
        )));
        assert!(matcher(None, Some(r"C:\x\ejemplo.exe")).matches(&window("", Some("ejemplo.exe"))));
        assert!(!m.matches(&window("Anything", Some("otro.exe"))));
        assert!(
            !m.matches(&window("Anything", None)),
            "unknown process never matches"
        );
    }

    #[test]
    fn both_fields_must_match() {
        let m = matcher(Some("Login"), Some("erp.exe"));
        assert!(m.matches(&window("login", Some("ERP.exe"))));
        assert!(!m.matches(&window("login", Some("other.exe"))));
        assert!(!m.matches(&window("Other", Some("erp.exe"))));
    }

    #[test]
    fn blank_association_matches_nothing() {
        for m in [
            matcher(None, None),
            matcher(Some(""), Some("  ")),
            matcher(Some(" "), None),
        ] {
            assert!(!m.matches(&window("", None)));
            assert!(!m.matches(&window("Anything", Some("any.exe"))));
        }
    }

    #[test]
    fn entries_for_window_filters_in_order() {
        let mut vault = VaultData::new().unwrap();
        let mut a = EntryInput::new("A", "u", SecretString::from("p"));
        a.windows = vec![matcher(None, Some("erp.exe"))];
        let mut b = EntryInput::new("B", "u", SecretString::from("p"));
        b.windows = vec![matcher(Some("Other"), None), matcher(Some("ERP*"), None)];
        let c = EntryInput::new("C", "u", SecretString::from("p"));
        vault.add_entry(a).unwrap();
        vault.add_entry(b).unwrap();
        vault.add_entry(c).unwrap();

        let target = window("ERP - Login", Some("erp.exe"));
        let names: Vec<&str> = vault.entries_for_window(&target).map(Entry::name).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(vault.entries_for_window(&window("x", None)).count(), 0);
    }

    fn choice_names(choice: &AutoTypeChoice<'_>) -> (&'static str, Vec<String>) {
        match choice {
            AutoTypeChoice::NoMatch => ("none", vec![]),
            AutoTypeChoice::TypeDirectly(e) => ("direct", vec![e.name().to_owned()]),
            AutoTypeChoice::Choose(list) => {
                ("choose", list.iter().map(|e| e.name().to_owned()).collect())
            }
        }
    }

    fn vault_of(entries: Vec<(&str, Vec<WindowMatch>)>) -> VaultData {
        let mut vault = VaultData::new().unwrap();
        for (name, windows) in entries {
            let mut input = EntryInput::new(name, "u", SecretString::from("p"));
            input.windows = windows;
            vault.add_entry(input).unwrap();
        }
        vault
    }

    #[test]
    fn checks_process_ignores_blank_names() {
        assert!(matcher(None, Some("erp.exe")).checks_process());
        assert!(!matcher(Some("ERP*"), None).checks_process());
        assert!(!matcher(Some("ERP*"), Some("  ")).checks_process());
    }

    #[test]
    fn single_match_by_executable_types_directly() {
        let vault = vault_of(vec![
            ("A", vec![matcher(None, Some("erp.exe"))]),
            ("B", vec![matcher(Some("Other"), None)]),
        ]);
        let target = window("ERP - Login", Some("erp.exe"));
        assert_eq!(
            choice_names(&vault.autotype_choice(&target)),
            ("direct", vec!["A".to_owned()])
        );
        // Title and executable together also count as checking it.
        let vault = vault_of(vec![("A", vec![matcher(Some("ERP*"), Some("erp.exe"))])]);
        assert_eq!(choice_names(&vault.autotype_choice(&target)).0, "direct");
    }

    #[test]
    fn single_match_by_title_only_asks_first() {
        // The executable-based association does not match this window;
        // only the title one does.
        let vault = vault_of(vec![(
            "A",
            vec![
                matcher(None, Some("other.exe")),
                matcher(Some("ERP*"), None),
            ],
        )]);
        let target = window("ERP - Login", Some("erp.exe"));
        assert_eq!(
            choice_names(&vault.autotype_choice(&target)),
            ("choose", vec!["A".to_owned()])
        );
    }

    #[test]
    fn several_matches_ask_first_even_by_executable() {
        let vault = vault_of(vec![
            ("A", vec![matcher(None, Some("erp.exe"))]),
            ("B", vec![matcher(None, Some("ERP.EXE"))]),
        ]);
        let target = window("ERP", Some("erp.exe"));
        assert_eq!(
            choice_names(&vault.autotype_choice(&target)),
            ("choose", vec!["A".to_owned(), "B".to_owned()])
        );
    }

    #[test]
    fn no_match_types_nothing() {
        let vault = vault_of(vec![("A", vec![matcher(None, Some("erp.exe"))])]);
        assert_eq!(
            choice_names(&vault.autotype_choice(&window("ERP", None))).0,
            "none"
        );
    }
}
