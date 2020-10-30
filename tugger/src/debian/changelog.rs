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
    package: Cow<'a, str>,
    version: Cow<'a, str>,
    distributions: Vec<Cow<'a, str>>,
    urgency: Cow<'a, str>,
    details: Cow<'a, str>,
    maintainer_name: Cow<'a, str>,
    maintainer_email: Cow<'a, str>,
    date: DateTime<Local>,
    /*


    */
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
pub struct Changelog<'a> {
    entries: Vec<ChangelogEntry<'a>>,
}

impl<'a> Changelog<'a> {
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
