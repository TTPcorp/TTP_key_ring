//! Offline checks for the web addresses stored in entries.
//!
//! Nothing here touches the network (a project rule): it cannot tell
//! whether a site exists or is legitimate. It checks that the text is a
//! well-formed `http`/`https` address and flags the usual tricks and
//! risks: unencrypted `http`, raw IP addresses, look-alike (IDN)
//! characters, `user@host` credentials and program names typed as sites.
//! Parsing follows the WHATWG URL standard via the `url` crate; nothing is
//! parsed by hand.

use serde::Serialize;
use url::{Host, Url};

/// Why a text is not accepted as a web address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WebAddressError {
    /// Empty, contains spaces, or does not parse as a web address.
    #[error("not a web address")]
    NotAWebAddress,
    /// A scheme other than `http`/`https` (`file:`, `javascript:`, ...).
    #[error("only http and https addresses are supported")]
    UnsupportedScheme,
    /// Credentials before the host (`https://bank.example@evil.example`):
    /// a classic phishing trick where the real site is the part after `@`.
    #[error("the address contains a user name or password")]
    EmbeddedCredentials,
}

/// Risks worth showing to the user. They do not block saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WebWarning {
    /// `http://`: traffic is not encrypted.
    NotEncrypted,
    /// The host is an IP address instead of a domain name.
    IpAddress,
    /// The domain has non-ASCII characters (stored as `xn--`), which can
    /// imitate another site (e.g. a Cyrillic "а" instead of a Latin "a").
    LookalikeCharacters,
    /// A host without dots (`intranet`, `localhost`): only valid inside
    /// a local network.
    SingleLabelHost,
    /// The last part looks like a program (`notepad.exe`), not a site.
    LooksLikeProgram,
}

/// A checked web address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAddress {
    /// Normalized form, e.g. `https://example.com/` for `Example.com`.
    pub url: String,
    /// Host as stored (lowercase; IDN in `xn--` form).
    pub host: String,
    /// Risks found, in a fixed order, without duplicates.
    pub warnings: Vec<WebWarning>,
}

/// Schemes that must never be turned into `https://...` by adding a
/// prefix. Without `//` they would otherwise look like `host:port`.
const NON_WEB_SCHEMES: &[&str] = &[
    "javascript:",
    "data:",
    "file:",
    "mailto:",
    "ftp:",
    "vbscript:",
    "about:",
    "blob:",
];

/// File extensions that are not top-level domains and usually mean a
/// program was typed as a site. (`.app` and `.com` are real TLDs.)
const PROGRAM_EXTENSIONS: &[&str] = &[
    "exe", "msi", "bat", "cmd", "lnk", "ps1", "dll", "vbs", "scr",
];

/// Checks and normalizes a web address typed by the user. A missing
/// scheme means `https://`.
pub fn check_web_address(input: &str) -> Result<WebAddress, WebAddressError> {
    let text = input.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return Err(WebAddressError::NotAWebAddress);
    }
    let lower = text.to_ascii_lowercase();
    let url = if text.contains("://") {
        Url::parse(text).map_err(|_| WebAddressError::NotAWebAddress)?
    } else if NON_WEB_SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return Err(WebAddressError::UnsupportedScheme);
    } else {
        Url::parse(&format!("https://{text}")).map_err(|_| WebAddressError::NotAWebAddress)?
    };

    let mut warnings = Vec::new();
    match url.scheme() {
        "https" => {}
        "http" => warnings.push(WebWarning::NotEncrypted),
        _ => return Err(WebAddressError::UnsupportedScheme),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(WebAddressError::EmbeddedCredentials);
    }

    let host = match url.host() {
        Some(Host::Domain(domain)) => {
            let labels: Vec<&str> = domain.split('.').collect();
            let tld = labels.last().copied().unwrap_or_default();
            // Real top-level domains are letters only, or IDN (`xn--`).
            let tld_ok = tld.starts_with("xn--")
                || (tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()));
            if labels.len() > 1 && !tld_ok {
                return Err(WebAddressError::NotAWebAddress);
            }
            if labels.iter().any(|l| l.starts_with("xn--")) {
                warnings.push(WebWarning::LookalikeCharacters);
            }
            if labels.len() == 1 {
                warnings.push(WebWarning::SingleLabelHost);
            }
            if labels.len() > 1 && PROGRAM_EXTENSIONS.contains(&tld) {
                warnings.push(WebWarning::LooksLikeProgram);
            }
            domain.to_owned()
        }
        Some(ip @ (Host::Ipv4(_) | Host::Ipv6(_))) => {
            warnings.push(WebWarning::IpAddress);
            ip.to_string()
        }
        None => return Err(WebAddressError::NotAWebAddress),
    };

    Ok(WebAddress {
        url: url.to_string(),
        host,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(input: &str) -> WebAddress {
        check_web_address(input).unwrap()
    }

    fn err(input: &str) -> WebAddressError {
        check_web_address(input).unwrap_err()
    }

    #[test]
    fn normal_sites_are_accepted_and_normalized() {
        let a = ok("Example.COM");
        assert_eq!(a.url, "https://example.com/");
        assert_eq!(a.host, "example.com");
        assert!(a.warnings.is_empty());

        let b = ok("  https://mail.example.org/login?next=/inbox  ");
        assert_eq!(b.url, "https://mail.example.org/login?next=/inbox");
        assert!(b.warnings.is_empty());

        assert!(ok("https://example.com:8443/").warnings.is_empty());
        assert!(ok("example.co.uk/path").warnings.is_empty());
    }

    #[test]
    fn non_addresses_are_rejected() {
        for input in [
            "",
            "   ",
            "my bank",
            "https://",
            "https://exa mple.com",
            "http://example.c0m",
            "example.1",
        ] {
            assert_eq!(err(input), WebAddressError::NotAWebAddress, "{input:?}");
        }
    }

    #[test]
    fn other_schemes_are_rejected() {
        for input in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,hi",
            "file:///C:/Windows",
            "ftp://example.com",
            "mailto:me@example.com",
            "chrome://settings",
        ] {
            assert_eq!(err(input), WebAddressError::UnsupportedScheme, "{input:?}");
        }
    }

    #[test]
    fn credentials_before_the_host_are_rejected() {
        assert_eq!(
            err("https://bank.example@evil.example"),
            WebAddressError::EmbeddedCredentials
        );
        assert_eq!(
            err("user:pass@example.com"),
            WebAddressError::EmbeddedCredentials
        );
    }

    #[test]
    fn risks_are_reported() {
        assert_eq!(
            ok("http://example.com").warnings,
            [WebWarning::NotEncrypted]
        );
        assert_eq!(ok("192.168.1.10").warnings, [WebWarning::IpAddress]);
        assert_eq!(ok("https://[::1]:8080").warnings, [WebWarning::IpAddress]);
        assert_eq!(ok("intranet").warnings, [WebWarning::SingleLabelHost]);
        assert_eq!(ok("notepad.exe").warnings, [WebWarning::LooksLikeProgram]);
        // Real TLDs that look like extensions are fine.
        assert!(ok("example.app").warnings.is_empty());
        assert!(ok("example.com").warnings.is_empty());
        // Several at once.
        assert_eq!(
            ok("http://10.0.0.1").warnings,
            [WebWarning::NotEncrypted, WebWarning::IpAddress]
        );
    }

    #[test]
    fn lookalike_domains_are_flagged() {
        // Cyrillic "а" (U+0430) in place of the Latin "a".
        let fake = ok("https://p\u{0430}ypal.com");
        assert!(fake.host.starts_with("xn--"));
        assert_eq!(fake.warnings, [WebWarning::LookalikeCharacters]);
        // Already in punycode form.
        assert_eq!(
            ok("xn--80ak6aa92e.com").warnings,
            [WebWarning::LookalikeCharacters]
        );
        // Legitimate IDN domains are flagged too: the user decides.
        assert_eq!(ok("españa.es").warnings, [WebWarning::LookalikeCharacters]);
    }

    #[test]
    fn warnings_serialize_as_codes() {
        let json = serde_json::to_string(&WebWarning::LookalikeCharacters).unwrap();
        assert_eq!(json, r#""lookalikeCharacters""#);
    }
}
