// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Code signing verification.
//!
//! This module implements functionality for verifying code signatures on
//! Mach-O binaries.
//!
//! # Verification Caveats
//!
//! **Verification performed by this code will vary from what Apple tools
//! do. Do not use successful verification from this code as validation that
//! Apple software will accept a signature.**
//!
//! We aim for our verification code to be as comprehensive as possible. But
//! there are things it doesn't yet or won't ever do. For example, we have
//! no clue of the full extent of verification that Apple performs because
//! that code is proprietary. We know some of the things that are done and
//! we have verification for a subset of them. Read the code or the set of
//! verification problem types enumerated by [VerificationProblemType] to get
//! a sense of what we do.

use {
    crate::{
        code_hash::compute_code_hashes,
        macho::{
            find_signature_data, parse_signature_data, CodeDirectoryBlob, CodeSigningSlot,
            DigestError, DigestType, EmbeddedSignature, MachOError,
        },
    },
    cryptographic_message_syntax::{CmsError, DigestAlgorithm, SignatureAlgorithm, SignedData},
    goblin::mach::{Mach, MachO},
    std::path::{Path, PathBuf},
};

/// Context for a verification issue.
#[derive(Clone, Debug)]
pub struct VerificationContext {
    /// Path of binary.
    pub path: Option<PathBuf>,

    /// Index of Mach-O binary within a fat binary that is problematic.
    pub fat_index: Option<usize>,
}

/// Describes a problem with verification.
#[derive(Debug)]
pub enum VerificationProblemType {
    IoError(std::io::Error),
    MachOParseError(goblin::error::Error),
    NoMachOSignatureData,
    MachOSignatureError(MachOError),
    LinkeditNotLastSegment,
    SignatureNotLastLinkeditData,
    NoCryptographicSignature,
    CmsError(CmsError),
    CmsOldSignatureAlgorithm(SignatureAlgorithm),
    NoCodeDirectory,
    CodeDirectoryOldDigestAlgorithm(DigestType),
    CodeDigestError(DigestError),
    CodeDigestMissingEntry(usize, Vec<u8>),
    CodeDigestExtraEntry(usize, Vec<u8>),
    CodeDigestMismatch(usize, Vec<u8>, Vec<u8>),
    SlotDigestMissing(CodeSigningSlot),
    ExtraSlotDigest(CodeSigningSlot, Vec<u8>),
    SlotDigestMismatch(CodeSigningSlot, Vec<u8>, Vec<u8>),
    SlotDigestError(DigestError),
}

#[derive(Debug)]
pub struct VerificationProblem {
    pub context: VerificationContext,
    pub problem: VerificationProblemType,
}

impl std::fmt::Display for VerificationProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let context = match (&self.context.path, &self.context.fat_index) {
            (None, None) => None,
            (Some(path), None) => Some(format!("{}", path.display())),
            (None, Some(index)) => Some(format!("@{}", index)),
            (Some(path), Some(index)) => Some(format!("{}@{}", path.display(), index)),
        };

        let message = match &self.problem {
            VerificationProblemType::IoError(e) => format!("I/O error: {}", e),
            VerificationProblemType::MachOParseError(e) => format!("Mach-O parse failure: {}", e),
            VerificationProblemType::NoMachOSignatureData => {
                "Mach-O signature data not found".to_string()
            }
            VerificationProblemType::MachOSignatureError(e) => {
                format!("error parsing Mach-O signature data: {}", e)
            }
            VerificationProblemType::LinkeditNotLastSegment => {
                "__LINKEDIT isn't last Mach-O segment".to_string()
            }
            VerificationProblemType::SignatureNotLastLinkeditData => {
                "signature isn't last data in __LINKEDIT segment".to_string()
            }
            VerificationProblemType::NoCryptographicSignature => {
                "no cryptographic signature present".to_string()
            }
            VerificationProblemType::CmsError(e) => format!("CMS error: {}", e),
            VerificationProblemType::CmsOldSignatureAlgorithm(alg) => {
                format!("insecure signature algorithm used: {:?}", alg)
            }
            VerificationProblemType::NoCodeDirectory => "no code directory".to_string(),
            VerificationProblemType::CodeDirectoryOldDigestAlgorithm(hash_type) => {
                format!(
                    "insecure digest algorithm used in code directory: {:?}",
                    hash_type
                )
            }
            VerificationProblemType::CodeDigestError(e) => {
                format!("error computing code digests: {}", e)
            }
            VerificationProblemType::CodeDigestMissingEntry(index, digest) => {
                format!(
                    "code digest missing entry at index {} for digest {}",
                    index,
                    hex::encode(&digest)
                )
            }
            VerificationProblemType::CodeDigestExtraEntry(index, digest) => {
                format!(
                    "code digest contains extra entry index {} with digest {}",
                    index,
                    hex::encode(&digest)
                )
            }
            VerificationProblemType::CodeDigestMismatch(index, cd_digest, actual_digest) => {
                format!(
                    "code digest mismatch for entry {}; recorded digest {}, actual {}",
                    index,
                    hex::encode(&cd_digest),
                    hex::encode(&actual_digest)
                )
            }
            VerificationProblemType::SlotDigestMissing(slot) => {
                format!("missing digest for slot {:?}", slot)
            }
            VerificationProblemType::ExtraSlotDigest(slot, digest) => {
                format!(
                    "slot digest contains digest for slot not in signature: {:?} with digest {}",
                    slot,
                    hex::encode(digest)
                )
            }
            VerificationProblemType::SlotDigestMismatch(slot, cd_digest, actual_digest) => {
                format!(
                    "slot digest mismatch for slot {:?}; recorded digest {}, actual {}",
                    slot,
                    hex::encode(cd_digest),
                    hex::encode(actual_digest)
                )
            }
            VerificationProblemType::SlotDigestError(e) => {
                format!("error computing slot digest: {}", e)
            }
        };

        match context {
            Some(context) => f.write_fmt(format_args!("{}: {}", context, message)),
            None => f.write_str(&message),
        }
    }
}

/// Verifies a binary in a given path.
///
/// Returns a vector of problems detected. An empty vector means no
/// problems were found.
pub fn verify_path(path: impl AsRef<Path>) -> Vec<VerificationProblem> {
    let path = path.as_ref();

    let context = VerificationContext {
        path: Some(path.to_path_buf()),
        fat_index: None,
    };

    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            return vec![VerificationProblem {
                context,
                problem: VerificationProblemType::IoError(e),
            }];
        }
    };

    verify_macho_data_internal(data, context)
}

/// Verifies unparsed Mach-O data.
///
/// Returns a vector of problems detected. An empty vector means no
/// problems were found.
pub fn verify_macho_data(data: impl AsRef<[u8]>) -> Vec<VerificationProblem> {
    let context = VerificationContext {
        path: None,
        fat_index: None,
    };

    verify_macho_data_internal(data, context)
}

fn verify_macho_data_internal(
    data: impl AsRef<[u8]>,
    context: VerificationContext,
) -> Vec<VerificationProblem> {
    match Mach::parse(data.as_ref()) {
        Ok(Mach::Binary(macho)) => verify_macho_internal(&macho, context),
        Ok(Mach::Fat(multiarch)) => {
            let mut problems = vec![];

            for index in 0..multiarch.narches {
                let mut context = context.clone();
                context.fat_index = Some(index);

                match multiarch.get(index) {
                    Ok(macho) => {
                        problems.extend(verify_macho_internal(&macho, context));
                    }
                    Err(e) => problems.push(VerificationProblem {
                        context,
                        problem: VerificationProblemType::MachOParseError(e),
                    }),
                }
            }

            problems
        }
        Err(e) => {
            vec![VerificationProblem {
                context,
                problem: VerificationProblemType::MachOParseError(e),
            }]
        }
    }
}

/// Verifies a parsed Mach-O binary.
///
/// Returns a vector of problems detected. An empty vector means no
/// problems were found.
pub fn verify_macho(macho: &MachO) -> Vec<VerificationProblem> {
    verify_macho_internal(
        macho,
        VerificationContext {
            path: None,
            fat_index: None,
        },
    )
}

fn verify_macho_internal(macho: &MachO, context: VerificationContext) -> Vec<VerificationProblem> {
    let signature_data = match find_signature_data(macho) {
        Ok(Some(data)) => data,
        Ok(None) => {
            return vec![VerificationProblem {
                context,
                problem: VerificationProblemType::NoMachOSignatureData,
            }];
        }
        Err(e) => {
            return vec![VerificationProblem {
                context,
                problem: VerificationProblemType::MachOSignatureError(e),
            }];
        }
    };

    let mut problems = vec![];

    // __LINKEDIT segment should be the last segment.
    if signature_data.linkedit_segment_index != macho.segments.len() - 1 {
        problems.push(VerificationProblem {
            context: context.clone(),
            problem: VerificationProblemType::LinkeditNotLastSegment,
        });
    }

    // Signature data should be the last data in the __LINKEDIT segment.
    if signature_data.signature_end_offset != signature_data.linkedit_segment_data.len() {
        problems.push(VerificationProblem {
            context: context.clone(),
            problem: VerificationProblemType::SignatureNotLastLinkeditData,
        });
    }

    let signature = match parse_signature_data(&signature_data.signature_data) {
        Ok(embedded) => embedded,
        Err(e) => {
            problems.push(VerificationProblem {
                context,
                problem: VerificationProblemType::MachOSignatureError(e),
            });

            // Can't do anything more if we couldn't parse the signature data.
            return problems;
        }
    };

    match signature.signature_data() {
        Ok(Some(cms_blob)) => {
            problems.extend(verify_cms_signature(cms_blob, context.clone()));
        }
        Ok(None) => problems.push(VerificationProblem {
            context: context.clone(),
            problem: VerificationProblemType::NoCryptographicSignature,
        }),
        Err(e) => {
            problems.push(VerificationProblem {
                context: context.clone(),
                problem: VerificationProblemType::MachOSignatureError(e),
            });
        }
    }

    match signature.code_directory() {
        Ok(Some(cd)) => {
            problems.extend(verify_code_directory(macho, &signature, &cd, context));
        }
        Ok(None) => {
            problems.push(VerificationProblem {
                context,
                problem: VerificationProblemType::NoCodeDirectory,
            });
        }
        Err(e) => {
            problems.push(VerificationProblem {
                context,
                problem: VerificationProblemType::MachOSignatureError(e),
            });
        }
    }

    problems
}

fn verify_cms_signature(data: &[u8], context: VerificationContext) -> Vec<VerificationProblem> {
    let signed_data = match SignedData::parse_ber(data) {
        Ok(signed_data) => signed_data,
        Err(e) => {
            return vec![VerificationProblem {
                context,
                problem: VerificationProblemType::CmsError(e),
            }];
        }
    };

    let mut problems = vec![];

    for signer in signed_data.signers() {
        match signer.digest_algorithm() {
            DigestAlgorithm::Sha256 => {}
        }

        match signer.signature_algorithm() {
            SignatureAlgorithm::Sha256Rsa
            | SignatureAlgorithm::EcdsaSha256
            | SignatureAlgorithm::Ed25519 => {}
            // RsaesPkcsV15 appears to be in widespread use. Should we still notify?
            SignatureAlgorithm::RsaesPkcsV15 | SignatureAlgorithm::Sha1Rsa => {
                problems.push(VerificationProblem {
                    context: context.clone(),
                    problem: VerificationProblemType::CmsOldSignatureAlgorithm(
                        signer.signature_algorithm(),
                    ),
                });
            }
        }

        match signer.verify_signature_with_signed_data(&signed_data) {
            Ok(()) => {}
            Err(e) => {
                problems.push(VerificationProblem {
                    context: context.clone(),
                    problem: VerificationProblemType::CmsError(e),
                });
            }
        }

        // TODO verify key length meets standards.
        // TODO verify CA chain is fully present.
        // TODO verify signing cert chains to Apple?
    }

    problems
}

fn verify_code_directory(
    macho: &MachO,
    signature: &EmbeddedSignature,
    cd: &CodeDirectoryBlob,
    context: VerificationContext,
) -> Vec<VerificationProblem> {
    let mut problems = vec![];

    match cd.hash_type {
        DigestType::Sha256 | DigestType::Sha384 => {}
        hash_type => problems.push(VerificationProblem {
            context: context.clone(),
            problem: VerificationProblemType::CodeDirectoryOldDigestAlgorithm(hash_type),
        }),
    }

    match compute_code_hashes(macho, cd.hash_type, Some(cd.page_size as usize)) {
        Ok(digests) => {
            let mut cd_iter = cd.code_hashes.iter().enumerate();
            let mut actual_iter = digests.iter().enumerate();

            loop {
                match (cd_iter.next(), actual_iter.next()) {
                    (None, None) => {
                        break;
                    }
                    (Some((cd_index, cd_digest)), Some((_, actual_digest))) => {
                        if &cd_digest.data != actual_digest {
                            problems.push(VerificationProblem {
                                context: context.clone(),
                                problem: VerificationProblemType::CodeDigestMismatch(
                                    cd_index,
                                    cd_digest.to_vec(),
                                    actual_digest.clone(),
                                ),
                            });
                        }
                    }
                    (None, Some((actual_index, actual_digest))) => {
                        problems.push(VerificationProblem {
                            context: context.clone(),
                            problem: VerificationProblemType::CodeDigestMissingEntry(
                                actual_index,
                                actual_digest.clone(),
                            ),
                        });
                    }
                    (Some((cd_index, cd_digest)), None) => {
                        problems.push(VerificationProblem {
                            context: context.clone(),
                            problem: VerificationProblemType::CodeDigestExtraEntry(
                                cd_index,
                                cd_digest.to_vec(),
                            ),
                        });
                    }
                }
            }
        }
        Err(e) => {
            problems.push(VerificationProblem {
                context: context.clone(),
                problem: VerificationProblemType::CodeDigestError(e),
            });
        }
    }

    // All slots beneath some threshold should have a special hash.
    // It isn't clear where this threshold is. But the alternate code directory and
    // CMS slots appear to start at 0x1000. We set our limit at 32, which seems
    // reasonable considering there are ~10 defined slots starting at value 0.
    //
    // The code directory doesn't have a digest because one cannot hash self.
    for blob in &signature.blobs {
        let slot = blob.slot;

        if u32::from(slot) < 32
            && !cd.special_hashes.contains_key(&slot)
            && slot != CodeSigningSlot::CodeDirectory
        {
            problems.push(VerificationProblem {
                context: context.clone(),
                problem: VerificationProblemType::SlotDigestMissing(slot),
            });
        }
    }

    let max_slot = cd
        .special_hashes
        .keys()
        .map(|slot| u32::from(*slot))
        .filter(|slot| *slot < 32)
        .max()
        .unwrap_or(0);

    let null_digest = b"\0".repeat(cd.hash_size as usize);

    // Verify the special/slot digests we do have match reality.
    for (slot, cd_digest) in cd.special_hashes.iter() {
        match signature.find_slot(*slot) {
            Some(entry) => match entry.digest_with(cd.hash_type) {
                Ok(actual_digest) => {
                    if actual_digest != cd_digest.to_vec() {
                        problems.push(VerificationProblem {
                            context: context.clone(),
                            problem: VerificationProblemType::SlotDigestMismatch(
                                *slot,
                                cd_digest.to_vec(),
                                actual_digest,
                            ),
                        });
                    }
                }
                Err(e) => {
                    problems.push(VerificationProblem {
                        context: context.clone(),
                        problem: VerificationProblemType::SlotDigestError(e),
                    });
                }
            },
            None => {
                // But slots with a null digest (all 0s) exist as placeholders when there
                // is a higher numbered slot present.
                if u32::from(*slot) >= max_slot || cd_digest.to_vec() != null_digest {
                    problems.push(VerificationProblem {
                        context: context.clone(),
                        problem: VerificationProblemType::ExtraSlotDigest(
                            *slot,
                            cd_digest.to_vec(),
                        ),
                    });
                }
            }
        }
    }

    // TODO verify code_limit[_64] is appropriate.
    // TODO verify exec_seg_base is appropriate.

    problems
}
