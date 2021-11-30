// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! PGP functionality. */

use {
    digest::Digest,
    pgp::{
        crypto::{HashAlgorithm, Hasher},
        packet::Packet,
        types::PublicKeyTrait,
        Signature,
    },
    std::{
        cmp::Ordering,
        collections::HashMap,
        io::{self, BufRead, BufReader, Read},
    },
};

const HEADER: &str = "-----BEGIN PGP SIGNED MESSAGE-----";
const HEADER_LF: &str = "-----BEGIN PGP SIGNED MESSAGE-----\n";
const HEADER_CRLF: &str = "-----BEGIN PGP SIGNED MESSAGE-----\r\n";

const SIGNATURE_ARMOR_LF: &str = "-----BEGIN PGP SIGNATURE-----\n";
const SIGNATURE_ARMOR_CRLF: &str = "-----BEGIN PGP SIGNATURE-----\r\n";

/// Wrapper around content digesting to work around lack of clone() in pgp crate.
#[derive(Clone)]
pub enum MyHasher {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl MyHasher {
    pub fn md5() -> Self {
        Self::Md5(md5::Md5::new())
    }

    pub fn sha1() -> Self {
        Self::Sha1(sha1::Sha1::new())
    }

    pub fn sha256() -> Self {
        Self::Sha256(sha2::Sha256::new())
    }

    pub fn sha384() -> Self {
        Self::Sha384(sha2::Sha384::new())
    }

    pub fn sha512() -> Self {
        Self::Sha512(sha2::Sha512::new())
    }

    pub fn algorithm(&self) -> HashAlgorithm {
        match self {
            Self::Md5(_) => HashAlgorithm::MD5,
            Self::Sha1(_) => HashAlgorithm::SHA1,
            Self::Sha256(_) => HashAlgorithm::SHA2_256,
            Self::Sha384(_) => HashAlgorithm::SHA2_384,
            Self::Sha512(_) => HashAlgorithm::SHA2_512,
        }
    }
}

impl std::io::Write for MyHasher {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Hasher for MyHasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Md5(digest) => digest.update(data),
            Self::Sha1(digest) => digest.update(data),
            Self::Sha256(digest) => digest.update(data),
            Self::Sha384(digest) => digest.update(data),
            Self::Sha512(digest) => digest.update(data),
        }
    }

    fn finish(self: Box<Self>) -> Vec<u8> {
        match *self {
            Self::Md5(digest) => digest.finalize().to_vec(),
            MyHasher::Sha1(digest) => digest.finalize().to_vec(),
            MyHasher::Sha256(digest) => digest.finalize().to_vec(),
            MyHasher::Sha384(digest) => digest.finalize().to_vec(),
            MyHasher::Sha512(digest) => digest.finalize().to_vec(),
        }
    }
}

enum ReaderState {
    /// Instance construction.
    Initial,

    /// In `Hashes: ` headers section following cleartext armor header.
    Hashes,

    /// Reading the inline cleartext message.
    ///
    /// No buffered data available to send to client.
    ///
    /// The inner bool tracks whether we have consumed content yet.
    CleartextEmpty(bool),

    /// Reading the inline cleartext message.
    ///
    /// Buffered data available to send to client.
    CleartextBuffered(String),

    /// In the signatures section after the cleartext message.
    Signatures,

    /// End of file reached.
    Eof,
}

/// A reader capable of extracting PGP cleartext signatures as defined by RFC 4880 Section 7.
///
/// <https://datatracker.ietf.org/doc/html/rfc4880.html#section-7>.
///
/// The source reader is expected to initially emit a
/// `'-----BEGIN PGP SIGNED MESSAGE-----` line.
///
pub struct CleartextSignatureReader<R: Read> {
    reader: BufReader<R>,
    state: ReaderState,

    /// Hash types as advertised by the `Hash: ` header.
    hashers: HashMap<u8, MyHasher>,

    /// Parsed PGP signatures.
    signatures: Vec<Signature>,
}

impl<R: Read> CleartextSignatureReader<R> {
    /// Construct a new instance from a reader.
    pub fn new(reader: R) -> Self {
        Self {
            state: ReaderState::Initial,
            reader: BufReader::new(reader),
            hashers: HashMap::new(),
            signatures: vec![],
        }
    }

    /// Finalize this reader, returning an object with signature state.
    pub fn finalize(self) -> CleartextSignatures {
        CleartextSignatures {
            hashers: self.hashers,
            signatures: self.signatures,
        }
    }
}

impl<'a, R: Read> Read for CleartextSignatureReader<R> {
    fn read(&mut self, dest: &mut [u8]) -> std::io::Result<usize> {
        loop {
            match &mut self.state {
                ReaderState::Initial => {
                    let mut line = String::with_capacity(HEADER_CRLF.len());
                    self.reader.read_line(&mut line)?;

                    if !matches!(line.as_str(), HEADER_LF | HEADER_CRLF) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "bad PGP cleartext header; expected `{}`; got `{}`",
                                HEADER, line
                            ),
                        ));
                    }

                    self.state = ReaderState::Hashes;
                    // Fall through to next loop.
                }
                ReaderState::Hashes => {
                    // Following the cleartext header armor are 1 or more `Hash: ` armor headers.
                    // These are terminated by an empty line.

                    let mut line = String::with_capacity(16);
                    self.reader.read_line(&mut line)?;

                    if let Some(hash) = line.strip_prefix("Hash: ") {
                        // Comma delimited list.
                        for hash in hash.split(',') {
                            let hash = hash.trim();

                            if !hash.is_empty() {
                                let hasher = match hash {
                                    "MD5" => MyHasher::md5(),
                                    "SHA1" => MyHasher::sha1(),
                                    "SHA256" => MyHasher::sha256(),
                                    "SHA384" => MyHasher::sha384(),
                                    "SHA512" => MyHasher::sha512(),
                                    _ => {
                                        return Err(io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            format!("unsupported PGP hash type: {}", hash),
                                        ));
                                    }
                                };

                                self.hashers
                                    .entry(hasher.algorithm() as u8)
                                    .or_insert(hasher);
                            }
                        }
                    } else if line.trim().is_empty() {
                        if self.hashers.is_empty() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "bad PGP cleartext signature; no Hash headers",
                            ));
                        }

                        self.state = ReaderState::CleartextEmpty(false);
                        // Fall through to next read.
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "bad PGP cleartext signature; expected Hash: header; got {}",
                                line.trim_end()
                            ),
                        ));
                    }
                }

                // We want to actually return the cleartext data to the caller.
                // However, we can't just proxy things through because this section
                // uses dash escaping.
                //
                // From RFC 4880 Section 7.1:
                //
                //    When reversing dash-escaping, an implementation MUST strip the string
                //    "- " if it occurs at the beginning of a line, and SHOULD warn on "-"
                //    and any character other than a space at the beginning of a line.
                //
                // (We do not warn.)
                //
                // In addition, we need to feed the cleartext data into registered hashers
                // as we read so we can possibly verify the signatures later without
                // access to the original content. This is subtly complex. Again per
                // RFC 4880 Section 7.1:
                //
                //    As with binary signatures on text documents, a cleartext signature is
                //    calculated on the text using canonical <CR><LF> line endings. The
                //    line ending (i.e., the <CR><LF>) before the '-----BEGIN PGP
                //    SIGNATURE-----' line that terminates the signed text is not
                //    considered part of the signed text.
                //
                // That CRLF before the `-----BEGIN PGP SIGNATURE----` line not being part
                // of the digested content is a super annoying constraint because it forces
                // us to maintain more state.
                ReaderState::CleartextEmpty(previous_read) => {
                    let mut line = String::with_capacity(128);
                    self.reader.read_line(&mut line)?;

                    let emit = if let Some(stripped) = line.strip_prefix("- ") {
                        stripped
                    } else if matches!(line.as_str(), SIGNATURE_ARMOR_LF | SIGNATURE_ARMOR_CRLF) {
                        // Fall through to continue reading signature data.
                        self.state = ReaderState::Signatures;
                        continue;
                    } else {
                        line.as_str()
                    };

                    let no_eol = emit.trim_end_matches(|c| c == '\r' || c == '\n');

                    for hasher in self.hashers.values_mut() {
                        // On non-initial reads, feed in CRLF from last line, since we know this
                        // line isn't the end of the cleartext.
                        if *previous_read {
                            hasher.update(b"\r\n");
                        }

                        hasher.update(no_eol.as_bytes());
                    }

                    // We could continue reading to fill the destination buffer. But that is
                    // more complex.
                    return match dest.len().cmp(&emit.as_bytes().len()) {
                        Ordering::Equal | Ordering::Greater => {
                            // Destination buffer is large enough to hold the line/content we just
                            // read. Just copy it over and return how many bytes we copied.
                            let count = emit.as_bytes().len();
                            let dest = &mut dest[0..count];
                            dest.copy_from_slice(emit.as_bytes());
                            self.state = ReaderState::CleartextEmpty(true);

                            Ok(count)
                        }
                        Ordering::Less => {
                            // We read more data than we have an output buffer to write. Copy what
                            // we can then set up the next read to come from the buffer.
                            let (to_copy, remaining) = emit.split_at(dest.len());
                            dest.copy_from_slice(to_copy.as_bytes());
                            self.state = ReaderState::CleartextBuffered(remaining.to_string());

                            Ok(to_copy.as_bytes().len())
                        }
                    };
                }

                ReaderState::CleartextBuffered(ref mut remaining) => {
                    return match dest.len().cmp(&remaining.as_bytes().len()) {
                        Ordering::Equal | Ordering::Greater => {
                            // The destination buffer has enough capacity to hold what we have.
                            // Write it out and revert to clean read mode.
                            let count = remaining.as_bytes().len();
                            let dest = &mut dest[0..count];

                            dest.copy_from_slice(remaining.as_bytes());
                            self.state = ReaderState::CleartextEmpty(true);

                            Ok(count)
                        }
                        Ordering::Less => {
                            // Write what we can.
                            let count = dest.len();

                            let (to_copy, remaining) = remaining.split_at(count);

                            dest.copy_from_slice(to_copy.as_bytes());
                            self.state = ReaderState::CleartextBuffered(remaining.to_string());

                            Ok(count)
                        }
                    };
                }
                ReaderState::Signatures => {
                    // We should only get into this state immediately after reading the
                    // SIGNATURE_ARMOR line.

                    // We can conveniently use the pgp crate's armor reader to decode this
                    // data until EOF.

                    // Ownership of the reader is a bit wonky. We make life easy by building
                    // a new one. This is inefficient. But meh.
                    let mut buffer = SIGNATURE_ARMOR_LF.as_bytes().to_vec();
                    self.reader.read_to_end(&mut buffer)?;

                    let mut dearmor = pgp::armor::Dearmor::new(io::Cursor::new(buffer));
                    dearmor.read_header()?;

                    if !matches!(dearmor.typ, Some(pgp::armor::BlockType::Signature)) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "failed to parse PGP signature armor",
                        ));
                    }

                    for packet in pgp::packet::PacketParser::new(dearmor) {
                        match packet {
                            Ok(Packet::Signature(signature)) => {
                                self.signatures.push(signature);
                            }
                            Ok(packet) => {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!(
                                        "unexpected PGP packet seen; expected Signature; got {:?}",
                                        packet.tag()
                                    ),
                                ));
                            }
                            Err(e) => {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("PGP packet parsing error: {:?}", e),
                                ));
                            }
                        }
                    }

                    self.state = ReaderState::Eof;
                    return Ok(0);
                }
                ReaderState::Eof => {
                    return Ok(0);
                }
            }
        }
    }
}

pub struct CleartextSignatures {
    hashers: HashMap<u8, MyHasher>,
    signatures: Vec<Signature>,
}

impl CleartextSignatures {
    /// Iterate over signatures in this instance.
    pub fn iter_signatures(&self) -> impl Iterator<Item = &Signature> {
        self.signatures.iter()
    }

    /// Iterate over signatures made by a specific key.
    pub fn iter_signatures_from_key<'slf, 'key: 'slf>(
        &'slf self,
        key: &'key impl PublicKeyTrait,
    ) -> impl Iterator<Item = &'slf Signature> {
        self.signatures.iter().filter(|sig| {
            if let Some(issuer) = sig.issuer() {
                &key.key_id() == issuer
            } else {
                false
            }
        })
    }

    /// Verify a signature made from a known key.
    ///
    /// Returns the numbers of signatures verified against this key.
    pub fn verify(&self, key: &impl PublicKeyTrait) -> pgp::errors::Result<usize> {
        if self.signatures.is_empty() {
            return Err(pgp::errors::Error::Message(
                "no PGP signatures present".to_string(),
            ));
        }

        let mut valid_signatures = 0;

        for sig in self.iter_signatures_from_key(key) {
            // We need to feed signature-specific state into the hasher (which was previously
            // fed the cleartext) to verify the signature. Fortunately we can clone hashers.
            let mut hasher = Box::new(
                self.hashers
                    .get(&(sig.config.hash_alg as u8))
                    .ok_or_else(|| {
                        pgp::errors::Error::Message(format!(
                            "could not find hasher matching signature hash algorithm ({:?})",
                            sig.config.hash_alg
                        ))
                    })?
                    .clone(),
            );

            let len = sig.config.hash_signature_data(&mut *hasher)?;
            hasher.update(&sig.config.trailer(len));

            let digest = hasher.finish();

            if digest[0..2] != sig.signed_hash_value {
                return Err(pgp::errors::Error::Message(
                    "invalid signed hash value".into(),
                ));
            }

            key.verify_signature(sig.config.hash_alg, &digest, &sig.signature)?;
            valid_signatures += 1;
        }

        match valid_signatures {
            0 => Err(pgp::errors::Error::Message(
                "no signatured signed by provided key".into(),
            )),
            _ => Ok(valid_signatures),
        }
    }
}
