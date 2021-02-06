// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {lazy_static::lazy_static, std::collections::BTreeMap};

type DistroVersion = Vec<(&'static str, &'static str)>;

lazy_static! {
    pub static ref GLIBC_VERSIONS_BY_DISTRO: BTreeMap<&'static str, DistroVersion> = {
        let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

        let mut fedora = DistroVersion::new();
        fedora.push(("16", "2.14"));
        fedora.push(("17", "2.15"));
        fedora.push(("18", "2.16"));
        fedora.push(("19", "2.17"));
        fedora.push(("20", "2.18"));
        fedora.push(("21", "2.20"));
        fedora.push(("22", "2.21"));
        fedora.push(("23", "2.22"));
        fedora.push(("24", "2.23"));
        fedora.push(("25", "2.24"));
        fedora.push(("26", "2.25"));
        fedora.push(("27", "2.26"));
        fedora.push(("28", "2.27"));
        fedora.push(("29", "2.28"));
        fedora.push(("30", "2.29"));
        fedora.push(("31", "2.30"));
        fedora.push(("32", "2.31"));
        res.insert("Fedora", fedora);

        let mut rhel = DistroVersion::new();
        rhel.push(("6", "2.12"));
        rhel.push(("7", "2.17"));
        rhel.push(("8", "2.28"));
        res.insert("RHEL", rhel);

        let mut opensuse = DistroVersion::new();
        opensuse.push(("11.4", "2.11"));
        opensuse.push(("12.1", "2.14"));
        opensuse.push(("12.2", "2.15"));
        opensuse.push(("12.3", "2.17"));
        opensuse.push(("13.1", "2.18"));
        opensuse.push(("13.2", "2.19"));
        opensuse.push(("42.1", "2.19"));
        opensuse.push(("42.2", "2.22"));
        opensuse.push(("42.3", "2.22"));
        opensuse.push(("15.0", "2.26"));
        opensuse.push(("15.1", "2.26"));
        res.insert("OpenSUSE", opensuse);

        let mut debian = DistroVersion::new();
        debian.push(("6", "2.11"));
        debian.push(("7", "2.13"));
        debian.push(("8", "2.19"));
        debian.push(("9", "2.24"));
        debian.push(("10", "2.28"));
        res.insert("Debian", debian);

        let mut ubuntu = DistroVersion::new();
        ubuntu.push(("12.04", "2.15"));
        ubuntu.push(("14.04", "2.19"));
        ubuntu.push(("16.04", "2.23"));
        ubuntu.push(("18.04", "2.27"));
        ubuntu.push(("18.10", "2.28"));
        ubuntu.push(("19.04", "2.29"));
        ubuntu.push(("19.10", "2.30"));
        ubuntu.push(("20.04", "2.31"));
        ubuntu.push(("20.10", "2.32"));
        res.insert("Ubuntu", ubuntu);

        res
    };
    pub static ref GCC_VERSIONS_BY_DISTRO: BTreeMap<&'static str, DistroVersion> = {
        let mut res: BTreeMap<&'static str, DistroVersion> = BTreeMap::new();

        let mut fedora = DistroVersion::new();
        fedora.push(("16", "4.6"));
        fedora.push(("17", "4.7"));
        fedora.push(("18", "4.7"));
        fedora.push(("19", "4.8"));
        fedora.push(("20", "4.8"));
        fedora.push(("21", "4.9"));
        fedora.push(("22", "4.9"));
        fedora.push(("23", "5.1"));
        fedora.push(("24", "6.1"));
        fedora.push(("25", "6.2"));
        fedora.push(("26", "7.1"));
        fedora.push(("27", "7.2"));
        fedora.push(("28", "8.0.1"));
        fedora.push(("29", "8.2.1"));
        fedora.push(("30", "9.0.1"));
        fedora.push(("31", "9.2.1"));
        fedora.push(("32", "10.0.1"));
        res.insert("Fedora", fedora);

        let mut rhel = DistroVersion::new();
        rhel.push(("6", "4.4"));
        rhel.push(("7", "4.8"));
        rhel.push(("8", "8.3.1"));
        res.insert("RHEL", rhel);

        let mut opensuse = DistroVersion::new();
        opensuse.push(("11.4", "4.5"));
        opensuse.push(("12.1", "4.6"));
        opensuse.push(("12.2", "4.7"));
        opensuse.push(("12.3", "4.7"));
        opensuse.push(("13.1", "4.8"));
        opensuse.push(("13.2", "4.8"));
        opensuse.push(("42.1", "4.8"));
        opensuse.push(("42.2", "4.8.5"));
        opensuse.push(("42.3", "4.8.5"));
        opensuse.push(("15.0", "7.3.1"));
        res.insert("OpenSUSE", opensuse);

        let mut debian = DistroVersion::new();
        debian.push(("6", "4.1"));
        debian.push(("7", "4.4"));
        debian.push(("8", "4.8"));
        debian.push(("9", "6.3"));
        debian.push(("10", "8.3"));
        res.insert("Debian", debian);

        let mut ubuntu = DistroVersion::new();
        ubuntu.push(("12.04", "4.4"));
        ubuntu.push(("14.04", "4.4"));
        ubuntu.push(("16.04", "4.7"));
        ubuntu.push(("18.04", "7.3"));
        ubuntu.push(("20.04", "9.3"));
        ubuntu.push(("20.10", "10.2"));
        res.insert("Ubuntu", ubuntu);

        res
    };
}

/// Find the minimum Linux distribution version supporting a given version of something.
pub fn find_minimum_distro_version(
    version: &version_compare::Version<'_>,
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
