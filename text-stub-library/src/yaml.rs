// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! YAML structures in text stub files.

This module defines raw YAML primitives existing in text stub files / `.tbd`
files.

See https://github.com/llvm/llvm-project/blob/main/llvm/lib/TextAPI/MachO/TextStub.cpp
for specifications of the YAML files.
*/

use serde::{Deserialize, Serialize};

/*
The TBD v1 format only support two level address libraries and is per
definition application extension safe.
---                              # the tag !tapi-tbd-v1 is optional and
                                 # shouldn't be emitted to support older linker.
archs: [ armv7, armv7s, arm64 ]  # the list of architecture slices that are
                                 # supported by this file.
platform: ios                    # Specifies the platform (macosx, ios, etc)
install-name: /u/l/libfoo.dylib  #
current-version: 1.2.3           # Optional: defaults to 1.0
compatibility-version: 1.0       # Optional: defaults to 1.0
swift-version: 0                 # Optional: defaults to 0
objc-constraint: none            # Optional: defaults to none
exports:                         # List of export sections
...
Each export section is defined as following:
 - archs: [ arm64 ]                   # the list of architecture slices
   allowed-clients: [ client ]        # Optional: List of clients
   re-exports: [ ]                    # Optional: List of re-exports
   symbols: [ _sym ]                  # Optional: List of symbols
   objc-classes: []                   # Optional: List of Objective-C classes
   objc-ivars: []                     # Optional: List of Objective C Instance
                                      #           Variables
   weak-def-symbols: []               # Optional: List of weak defined symbols
   thread-local-symbols: []           # Optional: List of thread local symbols
*/

/// Version 1 of the TBD structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion1 {
    /// The list of architecture slices that are supported by this file.
    ///
    /// armv7, arm64, etc.
    pub archs: Vec<String>,

    /// Specifies the platform (macosx, ios, etc).
    pub platform: String,

    /// Path of installed library.
    pub install_name: String,

    /// Current version of library.
    ///
    /// Defaults to `1.0`.
    pub current_version: Option<String>,

    /// Compatibility version of library.
    ///
    /// Defaults to `1.0`.
    pub compatibility_version: Option<String>,

    /// Swift version of library.
    ///
    /// Defaults to `0`.
    pub swift_version: Option<String>,

    /// Objective-C constraint.
    ///
    /// Defaults to `none`.
    pub objc_constraint: Option<String>,

    /// Export sections.
    pub exports: Vec<TbdVersion12ExportSection>,
}

/// Export section in a TBD version 1 or 2 structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion12ExportSection {
    /// List of architecture slices.
    pub archs: Vec<String>,

    /// List of clients.
    #[serde(default)]
    pub allowed_clients: Vec<String>,

    /// List of re-exports.
    #[serde(default)]
    pub re_exports: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_def_symbols: Vec<String>,

    /// List of thread local symbols.
    #[serde(default)]
    pub thread_local_symbols: Vec<String>,
}

/*
--- !tapi-tbd-v2
archs: [ armv7, armv7s, arm64 ]  # the list of architecture slices that are
                                 # supported by this file.
uuids: [ armv7:... ]             # Optional: List of architecture and UUID pairs.
platform: ios                    # Specifies the platform (macosx, ios, etc)
flags: []                        # Optional:
install-name: /u/l/libfoo.dylib  #
current-version: 1.2.3           # Optional: defaults to 1.0
compatibility-version: 1.0       # Optional: defaults to 1.0
swift-version: 0                 # Optional: defaults to 0
objc-constraint: retain_release  # Optional: defaults to retain_release
parent-umbrella:                 # Optional:
exports:                         # List of export sections
...
undefineds:                      # List of undefineds sections
...
Each export section is defined as following:
- archs: [ arm64 ]                   # the list of architecture slices
  allowed-clients: [ client ]        # Optional: List of clients
  re-exports: [ ]                    # Optional: List of re-exports
  symbols: [ _sym ]                  # Optional: List of symbols
  objc-classes: []                   # Optional: List of Objective-C classes
  objc-ivars: []                     # Optional: List of Objective C Instance
                                     #           Variables
  weak-def-symbols: []               # Optional: List of weak defined symbols
  thread-local-symbols: []           # Optional: List of thread local symbols
Each undefineds section is defined as following:
- archs: [ arm64 ]     # the list of architecture slices
  symbols: [ _sym ]    # Optional: List of symbols
  objc-classes: []     # Optional: List of Objective-C classes
  objc-ivars: []       # Optional: List of Objective C Instance Variables
  weak-ref-symbols: [] # Optional: List of weak defined symbols
*/

/// Version 2 of the TBD data structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion2 {
    /// The list of architecture slices that are supported by this file.
    pub archs: Vec<String>,

    /// List of architecture and UUID pairs.
    #[serde(default)]
    pub uuids: Vec<String>,

    /// Specifies the paltform (macosx, ios, etc).
    pub platform: String,

    #[serde(default)]
    pub flags: Vec<String>,

    pub install_name: String,

    /// Current version of library.
    ///
    /// Defaults to `1.0`.
    pub current_version: Option<String>,

    /// Compatibility version of library.
    ///
    /// Defaults to `1.0`.
    pub compatibility_version: Option<String>,

    /// Swift version of library.
    pub swift_version: Option<String>,

    /// Objective-C constraint.
    pub objc_constraint: Option<String>,

    pub parent_umbrella: Option<String>,

    /// Export sections.
    #[serde(default)]
    pub exports: Vec<TbdVersion12ExportSection>,

    /// Undefineds sections.
    #[serde(default)]
    pub undefineds: Vec<TbdVersion2UndefinedsSection>,
}

/// Undefineds sections in a version 2 TBD structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion2UndefinedsSection {
    /// The list of architecture slices.
    pub archs: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_ref_symbols: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdUmbrellaSection {
    #[serde(default)]
    pub targets: Vec<String>,

    pub umbrella: String,
}

/*
--- !tapi-tbd-v3
archs: [ armv7, armv7s, arm64 ]  # the list of architecture slices that are
                                 # supported by this file.
uuids: [ armv7:... ]             # Optional: List of architecture and UUID pairs.
platform: ios                    # Specifies the platform (macosx, ios, etc)
flags: []                        # Optional:
install-name: /u/l/libfoo.dylib  #
current-version: 1.2.3           # Optional: defaults to 1.0
compatibility-version: 1.0       # Optional: defaults to 1.0
swift-abi-version: 0             # Optional: defaults to 0
objc-constraint: retain_release  # Optional: defaults to retain_release
parent-umbrella:                 # Optional:
exports:                         # List of export sections
...
undefineds:                      # List of undefineds sections
...
Each export section is defined as following:
- archs: [ arm64 ]                   # the list of architecture slices
  allowed-clients: [ client ]        # Optional: List of clients
  re-exports: [ ]                    # Optional: List of re-exports
  symbols: [ _sym ]                  # Optional: List of symbols
  objc-classes: []                   # Optional: List of Objective-C classes
  objc-eh-types: []                  # Optional: List of Objective-C classes
                                     #           with EH
  objc-ivars: []                     # Optional: List of Objective C Instance
                                     #           Variables
  weak-def-symbols: []               # Optional: List of weak defined symbols
  thread-local-symbols: []           # Optional: List of thread local symbols
Each undefineds section is defined as following:
- archs: [ arm64 ]     # the list of architecture slices
  symbols: [ _sym ]    # Optional: List of symbols
  objc-classes: []     # Optional: List of Objective-C classes
  objc-eh-types: []                  # Optional: List of Objective-C classes
                                     #           with EH
  objc-ivars: []       # Optional: List of Objective C Instance Variables
  weak-ref-symbols: [] # Optional: List of weak defined symbols
*/

/// Version 3 of the TBD data structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion3 {
    /// The list of architecture slices that are supported by this file.
    pub archs: Vec<String>,

    /// List of architecture and UUID pairs.
    #[serde(default)]
    pub uuids: Vec<String>,

    /// Specifies the paltform (macosx, ios, etc).
    pub platform: String,

    #[serde(default)]
    pub flags: Vec<String>,

    pub install_name: String,

    /// Current version of library.
    ///
    /// Defaults to `1.0`.
    pub current_version: Option<String>,

    /// Compatibility version of library.
    ///
    /// Defaults to `1.0`.
    pub compatibility_version: Option<String>,

    /// Swift version of library.
    pub swift_abi_version: Option<String>,

    /// Objective-C constraint.
    ///
    /// Defaults to `retain_release`.
    pub objc_constraint: Option<String>,

    pub parent_umbrella: Option<String>,

    /// Export sections.
    #[serde(default)]
    pub exports: Vec<TbdVersion3ExportSection>,

    /// Undefineds sections.
    #[serde(default)]
    pub undefineds: Vec<TbdVersion3UndefinedsSection>,
}

/// Export section in a TBD version 3 structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion3ExportSection {
    /// List of architecture slices.
    pub archs: Vec<String>,

    /// List of clients.
    #[serde(default)]
    pub allowed_clients: Vec<String>,

    /// List of re-exports.
    #[serde(default)]
    pub re_exports: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C classes with EH.
    #[serde(default)]
    pub objc_eh_types: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_def_symbols: Vec<String>,

    /// List of thread local symbols.
    #[serde(default)]
    pub thread_local_symbols: Vec<String>,
}

/// Undefineds section in a version 3 TBD structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion3UndefinedsSection {
    /// The list of architecture slices.
    pub archs: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C classes with EH.
    #[serde(default)]
    pub objc_eh_types: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_ref_symbols: Vec<String>,
}

/*
--- !tapi-tbd
tbd-version: 4                              # The tbd version for format
targets: [ armv7-ios, x86_64-maccatalyst ]  # The list of applicable tapi supported target triples
uuids:                                      # Optional: List of target and UUID pairs.
  - target: armv7-ios
    value: ...
  - target: x86_64-maccatalyst
    value: ...
flags: []                        # Optional:
install-name: /u/l/libfoo.dylib  #
current-version: 1.2.3           # Optional: defaults to 1.0
compatibility-version: 1.0       # Optional: defaults to 1.0
swift-abi-version: 0             # Optional: defaults to 0
parent-umbrella:                 # Optional:
allowable-clients:
  - targets: [ armv7-ios ]       # Optional:
    clients: [ clientA ]
exports:                         # List of export sections
...
re-exports:                      # List of reexport sections
...
undefineds:                      # List of undefineds sections
...
Each export and reexport  section is defined as following:
- targets: [ arm64-macos ]                        # The list of target triples associated with symbols
  symbols: [ _symA ]                              # Optional: List of symbols
  objc-classes: []                                # Optional: List of Objective-C classes
  objc-eh-types: []                               # Optional: List of Objective-C classes
                                                  #           with EH
  objc-ivars: []                                  # Optional: List of Objective C Instance
                                                  #           Variables
  weak-symbols: []                                # Optional: List of weak defined symbols
  thread-local-symbols: []                        # Optional: List of thread local symbols
- targets: [ arm64-macos, x86_64-maccatalyst ]    # Optional: Targets for applicable additional symbols
  symbols: [ _symB ]                              # Optional: List of symbols
Each undefineds section is defined as following:
- targets: [ arm64-macos ]    # The list of target triples associated with symbols
  symbols: [ _symC ]          # Optional: List of symbols
  objc-classes: []            # Optional: List of Objective-C classes
  objc-eh-types: []           # Optional: List of Objective-C classes
                              #           with EH
  objc-ivars: []              # Optional: List of Objective C Instance Variables
  weak-symbols: []            # Optional: List of weak defined symbols
 */

/// Version 4 of the TBD data structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion4 {
    /// The tbd version for format.
    pub tbd_version: usize,

    /// The list of applicable tapi supported target triples.
    pub targets: Vec<String>,

    /// List of architecture and UUID pairs.
    #[serde(default)]
    pub uuids: Vec<TbdVersion4Uuid>,

    #[serde(default)]
    pub flags: Vec<String>,

    pub install_name: String,

    /// Current version of library.
    ///
    /// Defaults to `1.0`.
    pub current_version: Option<String>,

    /// Compatibility version of library.
    ///
    /// Defaults to `1.0`.
    pub compatibility_version: Option<String>,

    /// Swift version of library.
    pub swift_abi_version: Option<String>,

    #[serde(default)]
    pub parent_umbrella: Vec<TbdUmbrellaSection>,

    #[serde(default)]
    pub allowable_clients: Vec<TbdVersion4AllowableClient>,

    /// Export sections.
    #[serde(default)]
    pub exports: Vec<TbdVersion4ExportSection>,

    /// Reexport sections.
    ///
    /// Version 11.0+ of the macOS SDKs renamed the field from `re-exports` to `reexports`.
    #[serde(default, alias = "reexports")]
    pub re_exports: Vec<TbdVersion4ExportSection>,

    /// Undefineds sections.
    #[serde(default)]
    pub undefineds: Vec<TbdVersion4UndefinedsSection>,
}

/// A UUID value in a TBD version 4 data structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TbdVersion4Uuid {
    pub target: String,

    pub value: String,
}

/// An allowable client in a TBD version 4 data structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TbdVersion4AllowableClient {
    #[serde(default)]
    targets: Vec<String>,
    clients: Vec<String>,
}

/// (Re)export section in a TBD version 4 structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion4ExportSection {
    /// Target triples associated with symbols.
    pub targets: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C classes with EH.
    #[serde(default)]
    pub objc_eh_types: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_symbols: Vec<String>,

    /// List of thread local symbols.
    #[serde(default)]
    pub thread_local_symbols: Vec<String>,
}

/// Undefineds sections in a version 4 TBD structure.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TbdVersion4UndefinedsSection {
    /// The list of target triples associated with symbols.
    pub targets: Vec<String>,

    /// List of symbols.
    #[serde(default)]
    pub symbols: Vec<String>,

    /// List of Objective-C classes.
    #[serde(default)]
    pub objc_classes: Vec<String>,

    /// List of Objective-C classes with EH.
    #[serde(default)]
    pub objc_eh_types: Vec<String>,

    /// List of Objective-C instance variables.
    #[serde(default)]
    pub objc_ivars: Vec<String>,

    /// List of weak defined symbols.
    #[serde(default)]
    pub weak_symbols: Vec<String>,
}
