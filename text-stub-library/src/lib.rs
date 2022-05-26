// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod yaml;

use yaml::*;
use yaml_rust::ScanError;

/// Version of a TBD document.
#[derive(Copy, Clone, Debug)]
pub enum TbdVersion {
    V1,
    V2,
    V3,
    V4,
}

/// A parsed TBD record from a YAML document.
///
/// This is an enum over the raw, versioned YAML data structures.
pub enum TbdVersionedRecord {
    V1(TbdVersion1),
    V2(TbdVersion2),
    V3(TbdVersion3),
    V4(TbdVersion4),
}

/// Represents an error when parsing TBD YAML.
#[derive(Debug)]
pub enum ParseError {
    YamlError(yaml_rust::ScanError),
    DocumentCountMismatch,
    Serde(serde_yaml::Error),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::YamlError(e) => e.fmt(f),
            Self::DocumentCountMismatch => {
                f.write_str("mismatch in expected document count when parsing YAML")
            }
            Self::Serde(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<yaml_rust::ScanError> for ParseError {
    fn from(e: ScanError) -> Self {
        Self::YamlError(e)
    }
}

impl From<serde_yaml::Error> for ParseError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Serde(e)
    }
}

const TBD_V2_DOCUMENT_START: &str = "--- !tapi-tbd-v2";
const TBD_V3_DOCUMENT_START: &str = "--- !tapi-tbd-v3";
const TBD_V4_DOCUMENT_START: &str = "--- !tapi-tbd";

/// Parse TBD records from a YAML stream.
///
/// Returns a series of parsed records contained in the stream.
pub fn parse_str(data: &str) -> Result<Vec<TbdVersionedRecord>, ParseError> {
    // serde_yaml doesn't support tags on documents with YAML streams
    // (https://github.com/dtolnay/serde-yaml/issues/147) because yaml-rust
    // doesn't do so (https://github.com/chyh1990/yaml-rust/issues/147). Our
    // extremely hacky and inefficient solution is to parse the stream once
    // using yaml_rust to ensure it is valid YAML. Then we do a manual pass
    // scanning for document markers (`---` and `...`) and corresponding TBD
    // tags. We then pair things up and feed each document into the serde_yaml
    // deserializer for the given type.

    let yamls = yaml_rust::YamlLoader::load_from_str(data)?;

    // We got valid YAML. That's a good sign. Proceed with document/tag scanning.

    let mut document_versions = vec![];

    for line in data.lines() {
        // Start of new YAML document.
        if line.starts_with("---") {
            let version = if line.starts_with(TBD_V2_DOCUMENT_START) {
                TbdVersion::V2
            } else if line.starts_with(TBD_V3_DOCUMENT_START) {
                TbdVersion::V3
            } else if line.starts_with(TBD_V4_DOCUMENT_START) {
                TbdVersion::V4
            } else {
                // Version 1 has no document tag.
                TbdVersion::V1
            };

            document_versions.push(version);
        }
    }

    // The initial document marker in a YAML file is optional. And the
    // `---` marker is a version 1 TBD. So if there is a count mismatch,
    // insert a version 1 at the beginning of the versions list.
    if document_versions.len() == yamls.len() - 1 {
        document_versions.insert(0, TbdVersion::V1);
    } else if document_versions.len() != yamls.len() {
        return Err(ParseError::DocumentCountMismatch);
    }

    let mut res = vec![];

    for (index, value) in yamls.iter().enumerate() {
        // TODO We could almost certainly avoid the YAML parsing round trip
        let mut s = String::new();
        yaml_rust::YamlEmitter::new(&mut s).dump(value).unwrap();

        res.push(match document_versions[index] {
            TbdVersion::V1 => TbdVersionedRecord::V1(serde_yaml::from_str(&s)?),
            TbdVersion::V2 => TbdVersionedRecord::V2(serde_yaml::from_str(&s)?),
            TbdVersion::V3 => TbdVersionedRecord::V3(serde_yaml::from_str(&s)?),
            TbdVersion::V4 => TbdVersionedRecord::V4(serde_yaml::from_str(&s)?),
        })
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        apple_sdk::{AppleSdk, SdkSearch, SdkSearchLocation, SimpleSdk},
        rand::seq::SliceRandom,
        rayon::prelude::*,
    };

    #[test]
    fn test_parse_apple_sdk_tbds() {
        // This will find older Xcode versions and their SDKs when run in GitHub
        // Actions. That gives us extreme test coverage of real world .tbd files.
        let sdks = SdkSearch::empty()
            .location(SdkSearchLocation::SystemXcodes)
            .location(SdkSearchLocation::CommandLineTools)
            .search::<SimpleSdk>()
            .unwrap();

        sdks.into_par_iter().for_each(|sdk| {
            let mut tbd_paths = walkdir::WalkDir::new(sdk.path())
                .into_iter()
                .filter_map(|entry| {
                    let entry = entry.unwrap();

                    let file_name = entry.file_name().to_string_lossy();
                    if file_name.ends_with(".tbd") {
                        Some(entry.path().to_path_buf())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // We only select a percentage of tbd paths because there are too many
            // in CI and the test takes too long.
            let percentage = if let Ok(percentage) = std::env::var("TBD_SAMPLE_PERCENTAGE") {
                percentage.parse::<usize>().unwrap()
            } else {
                10
            };

            let mut rng = rand::thread_rng();
            tbd_paths.shuffle(&mut rng);

            for path in tbd_paths.iter().take(tbd_paths.len() * percentage / 100) {
                eprintln!("parsing {}", path.display());
                let data = std::fs::read(path).unwrap();
                let data = String::from_utf8(data).unwrap();

                parse_str(&data).unwrap_or_else(|e| {
                    eprintln!("path: {}", path.display());
                    eprint!("{}", data);
                    eprint!("{:?}", e);
                    panic!("parse error");
                });
            }
        });
    }
}
