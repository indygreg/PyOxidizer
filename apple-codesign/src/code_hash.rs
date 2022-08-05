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

use crate::{embedded_signature::DigestType, error::AppleCodesignError};

/// Compute paged hashes.
///
/// This function takes a reference to data, chunks it into segments of `page_size`
/// and then hashes it with the specified algorithm, producing a vector of binary hashes.
///
/// This is likely used as part of computing code hashes.
pub fn paged_digests(
    data: &[u8],
    hash: DigestType,
    page_size: usize,
) -> Result<Vec<Vec<u8>>, AppleCodesignError> {
    data.chunks(page_size)
        .map(|chunk| hash.digest_data(chunk))
        .collect::<Result<Vec<_>, AppleCodesignError>>()
}

/// Compute digests over raw data segments.
///
/// For each segment within `segments`, we compute paged digests of size `page_size`.
pub fn segment_digests<'a>(
    segments: impl Iterator<Item = &'a [u8]>,
    hash_type: DigestType,
    page_size: usize,
) -> Result<Vec<Vec<u8>>, AppleCodesignError> {
    Ok(segments
        .map(|data| paged_digests(data, hash_type, page_size))
        .collect::<Result<Vec<_>, AppleCodesignError>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>())
}
