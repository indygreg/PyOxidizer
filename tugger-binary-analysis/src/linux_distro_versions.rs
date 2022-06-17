// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {once_cell::sync::Lazy, std::collections::BTreeMap};

type DistroVersion = Vec<(&'static str, &'static str)>;

pub static GLIBC_VERSIONS_BY_DISTRO: Lazy<BTreeMap<&'static str, DistroVersion>> =
    Lazy::new(|| {
        let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

        res.insert(
            "Fedora",
            vec![
                ("16", "2.14"),
                ("17", "2.15"),
                ("18", "2.16"),
                ("19", "2.17"),
                ("20", "2.18"),
                ("21", "2.20"),
                ("22", "2.21"),
                ("23", "2.22"),
                ("24", "2.23"),
                ("25", "2.24"),
                ("26", "2.25"),
                ("27", "2.26"),
                ("28", "2.27"),
                ("29", "2.28"),
                ("30", "2.29"),
                ("31", "2.30"),
                ("32", "2.31"),
                ("33", "2.32"),
                ("34", "2.33"),
                ("35", "2.34"),
                ("36", "2.35"),
            ],
        );

        res.insert(
            "RHEL",
            vec![("6", "2.12"), ("7", "2.17"), ("8", "2.28"), ("9", "2.34")],
        );

        res.insert(
            "OpenSUSE",
            vec![
                ("11.4", "2.11"),
                ("12.1", "2.14"),
                ("12.2", "2.15"),
                ("12.3", "2.17"),
                ("13.1", "2.18"),
                ("13.2", "2.19"),
                ("42.1", "2.19"),
                ("42.2", "2.22"),
                ("42.3", "2.22"),
                ("15.0", "2.26"),
                ("15.1", "2.26"),
                ("15.2", "2.26"),
                ("15.3", "2.31"),
                ("15.4", "2.31"),
            ],
        );

        res.insert(
            "Debian",
            vec![
                ("6", "2.11"),
                ("7", "2.13"),
                ("8", "2.19"),
                ("9", "2.24"),
                ("10", "2.28"),
                ("11", "2.31"),
            ],
        );

        res.insert(
            "Ubuntu",
            vec![
                ("12.04", "2.15"),
                ("14.04", "2.19"),
                ("16.04", "2.23"),
                ("18.04", "2.27"),
                ("18.10", "2.28"),
                ("19.04", "2.29"),
                ("19.10", "2.30"),
                ("20.04", "2.31"),
                ("20.10", "2.32"),
                ("22.04", "2.35"),
            ],
        );

        res
    });
pub static GCC_VERSIONS_BY_DISTRO: Lazy<BTreeMap<&'static str, DistroVersion>> = Lazy::new(|| {
    let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

    res.insert(
        "Fedora",
        vec![
            ("16", "4.6"),
            ("17", "4.7"),
            ("18", "4.7"),
            ("19", "4.8"),
            ("20", "4.8"),
            ("21", "4.9"),
            ("22", "4.9"),
            ("23", "5.1"),
            ("24", "6.1"),
            ("25", "6.2"),
            ("26", "7.1"),
            ("27", "7.2"),
            ("28", "8.0.1"),
            ("29", "8.2.1"),
            ("30", "9.0.1"),
            ("31", "9.2.1"),
            ("32", "10.0.1"),
            ("33", "10.3.1"),
            ("34", "11.2.1"),
            ("35", "11.2.1"),
            ("36", "12.0.1"),
        ],
    );

    res.insert(
        "RHEL",
        vec![("6", "4.4"), ("7", "4.8"), ("8", "8.3.1"), ("9", "11.2.1")],
    );

    res.insert(
        "OpenSUSE",
        vec![
            ("11.4", "4.5"),
            ("12.1", "4.6"),
            ("12.2", "4.7"),
            ("12.3", "4.7"),
            ("13.1", "4.8"),
            ("13.2", "4.8"),
            ("42.1", "4.8"),
            ("42.2", "4.8.5"),
            ("42.3", "4.8.5"),
            ("15.0", "7.3.1"),
            ("15.1", "10.2.1"),
            ("15.2", "10.2.1"),
            ("15.3", "11.3.0"),
            ("15.4", "11.3.0"),
        ],
    );

    res.insert(
        "Debian",
        vec![
            ("6", "4.1"),
            ("7", "4.4"),
            ("8", "4.8"),
            ("9", "6.3"),
            ("10", "8.3"),
            ("11", "10.2.1"),
        ],
    );

    res.insert(
        "Ubuntu",
        vec![
            ("12.04", "4.4"),
            ("14.04", "4.4"),
            ("16.04", "4.7"),
            ("18.04", "7.3"),
            ("20.04", "9.3"),
            ("20.10", "10.2"),
            ("22.04", "12"),
        ],
    );

    res
});

/// Find the minimum Linux distribution version supporting a given version of something.
pub fn find_minimum_distro_version(
    version: &version_compare::Version,
    distro_versions: &BTreeMap<&'static str, DistroVersion>,
) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    for (distro, dv) in distro_versions {
        let mut found = false;

        for (distro_version, version_version) in dv {
            let version_version = version_compare::Version::from(version_version)
                .expect("unable to parse distro version");

            if &version_version >= version {
                found = true;
                res.push(format!("{} {}", distro, distro_version));
                break;
            }
        }

        if !found {
            res.push(format!("No known {} versions supported", distro));
        }
    }

    res
}
