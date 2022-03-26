// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Provides primitives for constructing embeddable signature data structures.

use {
    crate::{
        code_directory::CodeDirectoryBlob,
        embedded_signature::{
            create_superblob, Blob, BlobData, BlobWrapperBlob, CodeSigningMagic, CodeSigningSlot,
        },
        error::AppleCodesignError,
    },
    std::collections::BTreeMap,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum BlobsState {
    Empty,
    SpecialAdded,
    CodeDirectoryAdded,
    SignatureAdded,
}

impl Default for BlobsState {
    fn default() -> Self {
        Self::Empty
    }
}

/// An entity for producing and writing [EmbeddedSignature].
///
/// This entity can be used to incrementally build up super blob data.
#[derive(Debug, Default)]
pub struct EmbeddedSignatureBuilder<'a> {
    state: BlobsState,
    blobs: BTreeMap<CodeSigningSlot, BlobData<'a>>,
}

impl<'a> EmbeddedSignatureBuilder<'a> {
    /// Obtain the code directory registered with this instance.
    pub fn code_directory(&self) -> Option<&Box<CodeDirectoryBlob>> {
        self.blobs.get(&CodeSigningSlot::CodeDirectory).map(|blob| {
            if let BlobData::CodeDirectory(cd) = blob {
                cd
            } else {
                panic!("a non code directory should never be stored in the code directory slot");
            }
        })
    }

    /// Register a blob into a slot.
    ///
    /// There can only be a single blob per slot. Last write wins.
    ///
    /// The code directory and embedded signature cannot be added using this method.
    ///
    /// Blobs cannot be registered after a code directory or signature are added, as this
    /// would invalidate the signature.
    pub fn add_blob(
        &mut self,
        slot: CodeSigningSlot,
        blob: BlobData<'a>,
    ) -> Result<(), AppleCodesignError> {
        match self.state {
            BlobsState::Empty | BlobsState::SpecialAdded => {}
            BlobsState::CodeDirectoryAdded | BlobsState::SignatureAdded => {
                return Err(AppleCodesignError::SignatureBuilder(
                    "cannot add blobs after code directory or signature is registered",
                ));
            }
        }

        if matches!(
            blob,
            BlobData::CodeDirectory(_)
                | BlobData::EmbeddedSignature(_)
                | BlobData::EmbeddedSignatureOld(_)
        ) {
            return Err(AppleCodesignError::SignatureBuilder(
                "cannot register code directory or signature blob via add_blob()",
            ));
        }

        self.blobs.insert(slot, blob);

        self.state = BlobsState::SpecialAdded;

        Ok(())
    }

    /// Register a [CodeDirectoryBlob] with this builder.
    ///
    /// This is the recommended mechanism to register a Code Directory with this instance.
    ///
    /// When a code directory is registered, this method will automatically ensure digests
    /// of previously registered blobs/slots are present in the code directory. This
    /// removes the burden from callers of having to keep the code directory in sync with
    /// other registered blobs.
    pub fn add_code_directory(
        &mut self,
        mut cd: CodeDirectoryBlob<'a>,
    ) -> Result<&Box<CodeDirectoryBlob>, AppleCodesignError> {
        if matches!(self.state, BlobsState::SignatureAdded) {
            return Err(AppleCodesignError::SignatureBuilder(
                "cannot add code directory after signature data added",
            ));
        }

        for (slot, blob) in &self.blobs {
            let digest = blob.digest_with(cd.hash_type)?.into();

            cd.special_hashes.insert(*slot, digest);
        }

        self.blobs.insert(CodeSigningSlot::CodeDirectory, cd.into());
        self.state = BlobsState::CodeDirectoryAdded;

        Ok(self.code_directory().expect("we just inserted this key"))
    }

    /// Add CMS signature data to this builder.
    pub fn add_cms_signature(&mut self, der_data: Vec<u8>) -> Result<(), AppleCodesignError> {
        self.blobs.insert(
            CodeSigningSlot::Signature,
            BlobData::BlobWrapper(Box::new(BlobWrapperBlob::from_data_owned(der_data))),
        );

        self.state = BlobsState::SignatureAdded;

        Ok(())
    }

    /// Create the embedded signature "superblob" data.
    pub fn create_superblob(&self) -> Result<Vec<u8>, AppleCodesignError> {
        if matches!(self.state, BlobsState::Empty | BlobsState::SpecialAdded) {
            return Err(AppleCodesignError::SignatureBuilder(
                "code directory required in order to materialize superblob",
            ));
        }

        let blobs = self
            .blobs
            .iter()
            .map(|(slot, blob)| {
                let data = blob.to_blob_bytes()?;

                Ok((*slot, data))
            })
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        create_superblob(CodeSigningMagic::EmbeddedSignature, blobs.iter())
    }
}
