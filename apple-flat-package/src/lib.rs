// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple flat packages.
//!
//! Apple flat packages - often existing as `.pkg` files - are an installer
//! file format used by macOS.
//!
//! # File Format
//!
//! Flat packages are Apple-flavored XAR archives. XAR is a tar-like
//! file format consisting of file records/metadata and raw file data.
//! See the `apple-xar` crate for more on this file format.
//!
//! Flat packages come in 2 flavors: *component* packages and *product*
//! packages. *Component* packages contain a single *component*. *Product*
//! installers can contain multiple *components* as well as additional
//! metadata describing the installer. End-user `.pkg` files are typically
//! *product* packages. Using Apple tooling, *component* packages are built
//! `pkgbuild` and *product* packages using `productbuild`.
//!
//! ## Components
//!
//! A *component* defines an installable unit. *Components* are comprised of
//! a set of well-known files:
//!
//! `Bom`
//!    A *bill of materials* describing the contents of the component.
//! `PackageInfo`
//!    An XML file describing the component. See [PackageInfo] for the Rust
//!    struct defining this file format.
//! `Payload`
//!    A cpio archive containing files comprising the component. See the
//!    `cpio-archive` for more on this file format.
//! `Scripts`
//!    A cpio archive containing *scripts* files that run as part of component
//!    processing.
//!
//! ## Products
//!
//! A *product* flat package consists of 1 or more *components* and additional
//! metadata.
//!
//! A *product* flat package is identified by the presence of a `Distribution`
//! XML file in the root of the archive. See [Distribution] for the Rust type
//! defining this file format. See also
//! [Apple's XML documentation](https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/DistributionDefinitionRef/Chapters/Distribution_XML_Ref.html).
//!
//! Components within a *product* flat package exist in sub-directories which often
//! have the name `*.pkg/`.
//!
//! In addition, a *product* flat package may also have additional *resource* files
//! in the `Resources/` directory.
//!
//! # Cryptographic Signing
//!
//! Cryptographic message syntax (CMS) / RFC 5652 signatures can be embedded in
//! the XAR archive's *table of contents*, which is a data structure at the beginning
//! of the XAR defining the content within.
//!
//! The cryptographic signature is over the *checksum* content digest, which is also
//! captured in the XAR table of contents. This *checksum* effectively captures the
//! content of all files within the XAR.
//!
//! # Nested Archive Formats
//!
//! Flat packages contain multiple data structures that effectively enumerate
//! lists of files. There are many layers to the onion and there is duplication
//! of functionality to express file manifests.
//!
//! * XAR archives contain a *table of contents* enumerating files within the XAR.
//! * Each component has `Payload` and/or `Scripts` files, which are cpio archives.
//!   These cpio archives are file manifests containing file metadata and content.
//! * Each component may have a `Bom`, which is a binary data structure defining
//!   file metadata as well as other attributes.
//!
//! There are also multiple layers that involve compression:
//!
//! * The XAR table of contents is likely compressed with zlib.
//! * Individual files within XAR archives can be individually compressed
//!   with a compression format denoted by a MIME type.
//! * cpio archive files may also be compressed.
//! * Installed files in components may also be compressed (but this file
//!   content is treated as opaque by the flat package format).

pub mod component_package;
pub use component_package::ComponentPackageReader;
pub mod distribution;
pub use distribution::Distribution;
pub mod package_info;
pub use package_info::PackageInfo;
pub mod reader;
pub use reader::{PkgFlavor, PkgReader};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("scroll error: {0}")]
    Scroll(#[from] scroll::Error),

    #[error("XML error: {0}")]
    SerdeXml(#[from] serde_xml_rs::Error),

    #[error("xar error: {0}")]
    Xar(#[from] apple_xar::Error),

    #[error("cpio archive error: {0}")]
    Cpio(#[from] cpio_archive::Error),

    #[error("failed to resolve known component (this should not happen)")]
    ComponentResolution,
}

/// Result type for this crate.
pub type PkgResult<T> = std::result::Result<T, Error>;
