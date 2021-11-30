// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian package version string handling. */

use {
    std::{
        cmp::Ordering,
        fmt::{Display, Formatter},
        num::ParseIntError,
        str::FromStr,
    },
    thiserror::Error,
};

#[derive(Clone, Debug, Error)]
pub enum VersionError {
    #[error("error parsing string to integer: {0}")]
    ParseInt(#[from] ParseIntError),

    #[error("the epoch component has non-digit characters: {0}")]
    EpochNonNumeric(String),

    #[error("upstream_version component has illegal character: {0}")]
    UpstreamVersionIllegalChar(String),

    #[error("debian_revision component has illegal character: {0}")]
    DebianRevisionIllegalChar(String),
}

pub type Result<T> = std::result::Result<T, VersionError>;

/// A Debian package version.
///
/// Debian package versions consist of multiple sub-components and have rules about
/// sorting. The semantics are defined at
/// <https://www.debian.org/doc/debian-policy/ch-controlfields.html#version>. This type
/// attempts to implement all the details.
///
/// The concise version is the format is `[epoch:]upstream_version[-debian_revision]`
/// and each component has rules about what characters are allowed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageVersion {
    epoch: Option<u32>,
    upstream_version: String,
    debian_revision: Option<String>,
}

impl PackageVersion {
    /// Construct an instance by parsing a version string.
    pub fn parse(s: &str) -> Result<Self> {
        // Epoch is the part before a colon, if present.
        // upstream_version and debian_revision are discovered by splitting on last hyphen.

        let (epoch, remainder) = if let Some(pos) = s.find(':') {
            (Some(&s[0..pos]), &s[pos + 1..])
        } else {
            (None, s)
        };

        let (upstream, debian) = if let Some(pos) = remainder.rfind('-') {
            (&remainder[0..pos], Some(&remainder[pos + 1..]))
        } else {
            (remainder, None)
        };

        // Now do our validation.

        // The epoch is numeric.
        let epoch = if let Some(epoch) = epoch {
            if !epoch.chars().all(|c| c.is_ascii_digit()) {
                return Err(VersionError::EpochNonNumeric(s.to_string()));
            }

            Some(u32::from_str(epoch)?)
        } else {
            None
        };

        // The upstream_version must contain only alphanumerics and the characters . + - ~ (full stop,
        // plus, hyphen, tilde) and should start with a digit. If there is no debian_revision then
        // hyphens are not allowed.
        if !upstream.chars().all(|c| match c {
            c if c.is_ascii_alphanumeric() => true,
            '.' | '+' | '~' => true,
            '-' => debian.is_some(),
            _ => false,
        }) {
            return Err(VersionError::UpstreamVersionIllegalChar(s.to_string()));
        }

        let upstream_version = upstream.to_string();

        let debian_revision = if let Some(debian) = debian {
            // It must contain only alphanumerics and the characters + . ~ (plus, full stop, tilde)
            if !debian.chars().all(|c| match c {
                c if c.is_ascii_alphanumeric() => true,
                '+' | '.' | '~' => true,
                _ => false,
            }) {
                return Err(VersionError::DebianRevisionIllegalChar(s.to_string()));
            }

            Some(debian.to_string())
        } else {
            None
        };

        Ok(Self {
            epoch,
            upstream_version,
            debian_revision,
        })
    }

    /// The `epoch` component of the version string.
    ///
    /// Only `Some` if present or defined explicitly.
    pub fn epoch(&self) -> Option<u32> {
        self.epoch
    }

    /// Assumed value of `epoch` component.
    ///
    /// If the component isn't explicitly defined, a default of `0` will be assumed.
    pub fn epoch_assumed(&self) -> u32 {
        if let Some(epoch) = &self.epoch {
            *epoch
        } else {
            0
        }
    }

    /// `upstream` component of the version string.
    ///
    /// This is the main part of the version number.
    ///
    /// It is typically the original version of the software from which this package came. Although
    /// it may be massaged to be compatible with packaging requirements.
    pub fn upstream_version(&self) -> &str {
        &self.upstream_version
    }

    /// `debian_revision` component of the version string.
    ///
    /// The part of the version string that specifies the version of the Debian package based on
    /// the upstream version.
    pub fn debian_revision(&self) -> Option<&str> {
        self.debian_revision.as_deref()
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // [epoch:]upstream_version[-debian_revision]
        write!(
            f,
            "{}{}{}{}{}",
            if let Some(epoch) = self.epoch {
                format!("{}", epoch)
            } else {
                "".to_string()
            },
            if self.epoch.is_some() { ":" } else { "" },
            self.upstream_version,
            if self.debian_revision.is_some() {
                "-"
            } else {
                ""
            },
            if let Some(v) = &self.debian_revision {
                v
            } else {
                ""
            }
        )
    }
}

/// Split a string on the first non-digit character.
///
/// Returns the leading component with non-digits and everything else afterwards.
/// Either value can be an empty string.
fn split_first_digit(s: &str) -> (&str, &str) {
    let first_nondigit_index = s.chars().position(|c| c.is_ascii_digit());

    match first_nondigit_index {
        Some(0) => ("", s),
        Some(pos) => (&s[0..pos], &s[pos..]),
        None => (s, ""),
    }
}

fn split_first_nondigit(s: &str) -> (&str, &str) {
    let pos = s.chars().position(|c| !c.is_ascii_digit());

    match pos {
        Some(0) => ("", s),
        Some(pos) => (&s[0..pos], &s[pos..]),
        None => (s, ""),
    }
}

/// Split a string on the first non-digit character and convert the leading digits to an integer.
fn split_first_digit_number(s: &str) -> (u64, &str) {
    let (digits, remaining) = split_first_nondigit(s);

    let numeric = if digits.is_empty() {
        0
    } else {
        u64::from_str(digits).expect("digits should deserialize to string")
    };

    (numeric, remaining)
}

fn lexical_compare(a: &str, b: &str) -> Ordering {
    // The lexical comparison is a comparison of ASCII values modified so that all the letters sort
    // earlier than all the non-letters and so that a tilde sorts before anything, even the end of a
    // part.

    // We change the order of the strings so tildes come first, followed by letters, followed
    // by everything else.
    let compare_char = |a: &char, b: &char| -> Ordering {
        match (a, b) {
            ('~', '~') => Ordering::Equal,
            ('~', _) => Ordering::Less,
            (_, '~') => Ordering::Greater,
            (a, b) if a.is_ascii_alphabetic() && !b.is_ascii_alphabetic() => Ordering::Less,
            (a, b) if !a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => Ordering::Greater,
            (_, _) => Ordering::Equal,
        }
    };

    let mut a_chars = a.chars().collect::<Vec<_>>();
    let mut b_chars = b.chars().collect::<Vec<_>>();
    a_chars.sort_by(compare_char);
    b_chars.sort_by(compare_char);

    // We then compare character by character, taking our modified lexical sort into
    // consideration. This gets funky when string lengths are different. Normally the
    // shorter string would sort lower. But our custom lexical compare applies when comparing
    // against a missing character!

    for pos in 0..std::cmp::max(a_chars.len(), b_chars.len()) {
        let a_char = a_chars.get(pos);
        let b_char = b_chars.get(pos);

        match (a_char, b_char) {
            (Some(a_char), None) if *a_char == '~' => {
                return Ordering::Less;
            }
            (Some(_), None) => {
                return Ordering::Greater;
            }
            (None, Some(b_char)) if *b_char == '~' => {
                return Ordering::Greater;
            }
            (None, Some(_)) => {
                return Ordering::Less;
            }
            (Some(a_char), Some(b_char)) => match compare_char(a_char, b_char) {
                Ordering::Equal => {}
                res => {
                    return res;
                }
            },
            (None, None) => {
                panic!("None, None variant should never happen");
            }
        }
    }

    Ordering::Equal
}

/// Compare a version component string using Debian rules.
fn compare_component(a: &str, b: &str) -> Ordering {
    // The comparison consists of iterations of a 2 step process until both inputs are exhausted.
    //
    // Step 1: Initial part of each string consisting of non-digit characters is compared using
    // a custom lexical sort.
    //
    // Step 2: Initial part of remaining string consisting of digit characters is compared using
    // numerical sort.
    let mut a_remaining = a;
    let mut b_remaining = b;

    loop {
        let a_res = split_first_digit(a_remaining);
        let a_leading_nondigit = a_res.0;
        a_remaining = a_res.1;

        let b_res = split_first_digit(b_remaining);
        let b_leading_nondigit = b_res.0;
        b_remaining = b_res.1;

        // These two parts (one of which may be empty) are compared lexically. If a difference is
        // found it is returned.
        match lexical_compare(a_leading_nondigit, b_leading_nondigit) {
            Ordering::Equal => {}
            res => {
                return res;
            }
        }

        // Then the initial part of the remainder of each string which consists entirely of digit
        // characters is determined.

        // The numerical values of these two parts are compared, and any difference found is
        // returned as the result of the comparison. For these purposes an empty string (which can
        // only occur at the end of one or both version strings being compared) counts as zero.

        let a_res = split_first_digit_number(a_remaining);
        let a_numeric = a_res.0;
        a_remaining = a_res.1;

        let b_res = split_first_digit_number(b_remaining);
        let b_numeric = b_res.0;
        b_remaining = b_res.1;

        match a_numeric.cmp(&b_numeric) {
            Ordering::Equal => {}
            res => {
                return res;
            }
        }

        if a_remaining.is_empty() && b_remaining.is_empty() {
            return Ordering::Equal;
        }
    }
}

impl PartialOrd<Self> for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Epoch is compared numerically. Then upstream and debian components are compared
        // using a custom algorithm. The absence of a debian revision is equivalent to `0`.

        match self.epoch_assumed().cmp(&other.epoch_assumed()) {
            Ordering::Less => Some(Ordering::Less),
            Ordering::Greater => Some(Ordering::Greater),
            Ordering::Equal => {
                match compare_component(&self.upstream_version, &other.upstream_version) {
                    Ordering::Less => Some(Ordering::Less),
                    Ordering::Greater => Some(Ordering::Greater),
                    Ordering::Equal => {
                        let a = self.debian_revision.as_deref().unwrap_or("0");
                        let b = other.debian_revision.as_deref().unwrap_or("0");

                        Some(compare_component(a, b))
                    }
                }
            }
        }
    }
}

impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() -> Result<()> {
        assert_eq!(
            PackageVersion::parse("1:4.7.0+dfsg1-2")?,
            PackageVersion {
                epoch: Some(1),
                upstream_version: "4.7.0+dfsg1".into(),
                debian_revision: Some("2".into()),
            }
        );
        assert_eq!(
            PackageVersion::parse("3.3.2.final~github")?,
            PackageVersion {
                epoch: None,
                upstream_version: "3.3.2.final~github".into(),
                debian_revision: None,
            }
        );
        assert_eq!(
            PackageVersion::parse("3.3.2.final~github-2")?,
            PackageVersion {
                epoch: None,
                upstream_version: "3.3.2.final~github".into(),
                debian_revision: Some("2".into()),
            }
        );
        assert_eq!(
            PackageVersion::parse("0.18.0+dfsg-2+b1")?,
            PackageVersion {
                epoch: None,
                upstream_version: "0.18.0+dfsg".into(),
                debian_revision: Some("2+b1".into())
            }
        );

        Ok(())
    }

    #[test]
    fn format() -> Result<()> {
        for s in ["1:4.7.0+dfsg1-2", "3.3.2.final~github", "0.18.0+dfsg-2+b1"] {
            let v = PackageVersion::parse(s)?;
            assert_eq!(format!("{}", v), s);
        }

        Ok(())
    }

    #[test]
    fn test_lexical_compare() {
        assert_eq!(lexical_compare("~~", "~~a"), Ordering::Less);
        assert_eq!(lexical_compare("~~a", "~~"), Ordering::Greater);
        assert_eq!(lexical_compare("~~a", "~"), Ordering::Less);
        assert_eq!(lexical_compare("~", "~~a"), Ordering::Greater);
        assert_eq!(lexical_compare("~", ""), Ordering::Less);
        assert_eq!(lexical_compare("", "~"), Ordering::Greater);
        assert_eq!(lexical_compare("", "a"), Ordering::Less);
        assert_eq!(lexical_compare("a", ""), Ordering::Greater);

        // 1.0~beta1~svn1245 sorts earlier than 1.0~beta1, which sorts earlier than 1.0.
    }

    #[test]
    fn test_compare_component() {
        assert_eq!(
            compare_component("1.0~beta1~svn1245", "1.0~beta1"),
            Ordering::Less
        );
        assert_eq!(compare_component("1.0~beta1", "1.0"), Ordering::Less);
    }

    #[test]
    fn compare_version() {
        assert_eq!(
            PackageVersion {
                epoch: Some(1),
                upstream_version: "ignored".into(),
                debian_revision: None,
            }
            .cmp(&PackageVersion {
                epoch: Some(0),
                upstream_version: "ignored".into(),
                debian_revision: None
            }),
            Ordering::Greater
        );
    }
}
