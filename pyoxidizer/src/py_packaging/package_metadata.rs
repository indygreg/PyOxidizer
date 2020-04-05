// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Working with Python package metadata (i.e. .pkg-info directories) */

use {
    anyhow::{Context, Result},
    mailparse::parse_mail,
};

/// Represents a Python METADATA file.
pub struct PythonPackageMetadata {
    headers: Vec<(String, String)>,
}

impl PythonPackageMetadata {
    /// Create an instance from data in a METADATA file.
    pub fn from_metadata(data: &[u8]) -> Result<PythonPackageMetadata> {
        let message = parse_mail(&data).context("parsing metadata file")?;

        let headers = message
            .headers
            .iter()
            .map(|header| (header.get_key(), header.get_value()))
            .collect::<Vec<_>>();

        Ok(PythonPackageMetadata { headers })
    }

    /// Find the first value of a specified header.
    pub fn find_first_header(&self, key: &str) -> Option<&str> {
        for (k, v) in &self.headers {
            if k == key {
                return Some(v);
            }
        }

        None
    }

    /// Find all values of a specified header.
    #[allow(unused)]
    pub fn find_all_headers(&self, key: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter_map(|(k, v)| if k == key { Some(v.as_ref()) } else { None })
            .collect::<Vec<_>>()
    }

    pub fn name(&self) -> Option<&str> {
        self.find_first_header("Name")
    }

    pub fn version(&self) -> Option<&str> {
        self.find_first_header("Version")
    }

    #[allow(unused)]
    pub fn license(&self) -> Option<&str> {
        self.find_first_header("License")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metadata() -> Result<()> {
        let data = concat!(
            "Metadata-Version: 2.1\n",
            "Name: black\n",
            "Version: 19.10b0\n",
            "Summary: The uncompromising code formatter.\n",
            "Home-page: https://github.com/psf/black\n",
            "Author: Łukasz Langa\n",
            "Author-email: lukasz@langa.pl\n",
            "License: MIT\n",
            "Requires-Dist: click (>=6.5)\n",
            "Requires-Dist: attrs (>=18.1.0)\n",
            "Requires-Dist: appdirs\n",
            "\n",
            "![Black Logo](https://raw.githubusercontent.com/psf/black/master/docs/_static/logo2-readme.png)\n",
            "\n",
            "<h2 align=\"center\">The Uncompromising Code Formatter</h2>\n",
        ).as_bytes();

        let m = PythonPackageMetadata::from_metadata(data)?;

        assert_eq!(m.name(), Some("black"));
        assert_eq!(m.version(), Some("19.10b0"));
        assert_eq!(m.license(), Some("MIT"));
        assert_eq!(
            m.find_all_headers("Requires-Dist"),
            vec!["click (>=6.5)", "attrs (>=18.1.0)", "appdirs"]
        );
        assert_eq!(m.find_first_header("Missing"), None);

        Ok(())
    }
}
