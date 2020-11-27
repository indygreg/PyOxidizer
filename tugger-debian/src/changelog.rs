// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines types representing debian/changelog files.

See https://www.debian.org/doc/debian-policy/ch-source.html#debian-changelog-debian-changelog
for the specification.
*/

use {
    chrono::{DateTime, Local},
    std::{borrow::Cow, io::Write},
};

#[derive(Clone, Debug)]
pub struct ChangelogEntry<'a> {
    pub package: Cow<'a, str>,
    pub version: Cow<'a, str>,
    pub distributions: Vec<Cow<'a, str>>,
    pub urgency: Cow<'a, str>,
    pub details: Cow<'a, str>,
    pub maintainer_name: Cow<'a, str>,
    pub maintainer_email: Cow<'a, str>,
    pub date: DateTime<Local>,
}

impl<'a> ChangelogEntry<'a> {
    /// Serialize the changelog entry to a writer.
    ///
    /// This incurs multiple `.write()` calls. So a buffered writer is
    /// recommended if performance matters.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        /*
        package (version) distribution(s); urgency=urgency
          [optional blank line(s), stripped]
          * change details
          more change details
          [blank line(s), included in output of dpkg-parsechangelog]
          * even more change details
          [optional blank line(s), stripped]
         -- maintainer name <email address>[two spaces]  date
        */
        writer.write_all(self.package.as_bytes())?;
        writer.write_all(b" (")?;
        writer.write_all(self.version.as_bytes())?;
        writer.write_all(b") ")?;
        writer.write_all(self.distributions.join(" ").as_bytes())?;
        writer.write_all(b"; urgency=")?;
        writer.write_all(self.urgency.as_bytes())?;
        writer.write_all(b"\n\n")?;
        writer.write_all(self.details.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.write_all(b"-- ")?;
        writer.write_all(self.maintainer_name.as_bytes())?;
        writer.write_all(b" <")?;
        writer.write_all(self.maintainer_email.as_bytes())?;
        writer.write_all(b">  ")?;
        writer.write_all(self.date.to_rfc2822().as_bytes())?;
        writer.write_all(b"\n\n")?;

        Ok(())
    }
}

/// Represents a complete `debian/changelog` file.
///
/// Changelogs are an ordered series of `ChangelogEntry` items.
#[derive(Default)]
pub struct Changelog<'a> {
    entries: Vec<ChangelogEntry<'a>>,
}

impl<'a> Changelog<'a> {
    /// Add an entry to this changelog.
    pub fn add_entry<'b: 'a>(&mut self, entry: ChangelogEntry<'b>) {
        self.entries.push(entry)
    }

    /// Serialize the changelog to a writer.
    ///
    /// Use of a buffered writer is encouraged if performance is a concern.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for entry in &self.entries {
            entry.write(writer)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {super::*, anyhow::Result};

    #[test]
    fn test_write() -> Result<()> {
        let mut changelog = Changelog::default();
        changelog.add_entry(ChangelogEntry {
            package: "mypackage".into(),
            version: "0.1".into(),
            distributions: vec!["mydist".into()],
            urgency: "low".into(),
            details: "details".into(),
            maintainer_name: "maintainer".into(),
            maintainer_email: "me@example.com".into(),
            date: DateTime::from_utc(
                chrono::NaiveDateTime::from_timestamp(1420000000, 0),
                chrono::TimeZone::from_offset(&chrono::FixedOffset::west(3600 * 7)),
            ),
        });

        let mut buf = vec![];
        changelog.write(&mut buf)?;

        let s = String::from_utf8(buf)?;
        assert_eq!(s, "mypackage (0.1) mydist; urgency=low\n\ndetails\n-- maintainer <me@example.com>  Tue, 30 Dec 2014 21:26:40 -0700\n\n");

        Ok(())
    }
}
