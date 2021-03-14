// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to hashing code.

One aspect of Apple code signing is binary integrity verification.

The Mach-O signature data contains cryptographic hashes of content
of the thing being signed. The way this works is different sections
of the binary are split into chunks or pages (e.g. of 4096 bytes).
The cryptographic hash of each chunk is computed and the hashes
are written to the signature data. When the binary is loaded, as a
hash is paged into the kernel, its cryptographic hash is verified against
what is inside the binary.

This module contains code related to reading and writing these so-called
*code hashes*.
*/

use {
    crate::macho::{
        find_signature_data, parse_signature_data, CodeSigningSlot, DigestError, HashType,
        MachOParseError,
    },
    goblin::mach::MachO,
};

/// Compute code hashes.
///
/// This function takes a reference to data (presumably code), chunks it into segments
/// of `page_size` up to offset `max_offset` and then hashes it with the specified
/// algorithm, producing a vector of binary hashes.
pub fn compute_code_hashes(
    data: &[u8],
    hash: HashType,
    page_size: usize,
    max_offset: usize,
) -> Result<Vec<Vec<u8>>, DigestError> {
    let data = &data[..max_offset];

    data.chunks(page_size)
        .map(|chunk| hash.digest(chunk))
        .collect::<Result<Vec<_>, DigestError>>()
}

#[derive(Debug)]
pub enum SignatureError {
    ParseError(MachOParseError),
    NoSignatureData,
    NoCodeDirectory,
    HashingError(DigestError),
    MissingHash(CodeSigningSlot),
    HashMismatch(CodeSigningSlot, Vec<u8>, Vec<u8>),
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(e) => e.fmt(f),
            Self::NoSignatureData => f.write_str("no signature data found"),
            Self::NoCodeDirectory => {
                f.write_str("signature data does not contain a code directory blob")
            }
            Self::HashingError(e) => {
                f.write_fmt(format_args!("error occurred while hashing: {}", e))
            }
            Self::MissingHash(slot) => {
                f.write_fmt(format_args!("missing hash for slot {:?}", slot))
            }
            Self::HashMismatch(slot, got, wanted) => f.write_fmt(format_args!(
                "hash mismatch on {:?}; got {:?}; wanted {:?}",
                slot, got, wanted
            )),
        }
    }
}

impl std::error::Error for SignatureError {}

impl From<MachOParseError> for SignatureError {
    fn from(e: MachOParseError) -> Self {
        Self::ParseError(e)
    }
}

impl From<DigestError> for SignatureError {
    fn from(e: DigestError) -> Self {
        Self::HashingError(e)
    }
}

/// Given a Mach-O binary, attempt to verify the integrity of code hashes within.
pub fn verify_macho_code_hashes(macho: &MachO) -> Result<(), SignatureError> {
    if let Some(signature_data) = find_signature_data(macho)? {
        let signature = parse_signature_data(signature_data.signature_data)?;

        let code_directory = signature
            .code_directory()?
            .ok_or(SignatureError::NoCodeDirectory)?;

        // "Special" hashes are hashes over the signature data itself. They
        // help ensure the signature data hasn't been tampered with. There
        // should be a signature for each blob in the signature payload.
        for blob_entry in &signature.blobs {
            let actual_hash = code_directory.hash_type.digest(blob_entry.data)?;

            if let Some(expected_hash) = code_directory.special_hashes.get(&blob_entry.slot) {
                let expected_hash = expected_hash.to_vec();

                // There's a timing side-channel here, but it shouldn't matter since it
                // isn't like someone is brute-forcing hashes.
                if actual_hash != expected_hash {
                    return Err(SignatureError::HashMismatch(
                        blob_entry.slot,
                        actual_hash,
                        expected_hash,
                    ));
                }
            } else {
                // TODO we presumably need to exclude CodeDirectory since it cannot sign self?
                return Err(SignatureError::MissingHash(blob_entry.slot));
            }
        }

        // "Code" hashes are hashes over the code.
        // TODO implement this.

        panic!("not yet implemented");
    } else {
        Err(SignatureError::NoSignatureData)
    }
}
