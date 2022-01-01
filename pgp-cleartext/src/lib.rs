// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! PGP cleartext framework

The PGP cleartext framework is a mechanism to store PGP signatures inline with
the cleartext data that is being signed.

The cleartext framework is defined by
[RFC 4880 Section 7](https://datatracker.ietf.org/doc/html/rfc4880.html#section-7)
and this implementation aims to be conformant with the specification.

PGP cleartext signatures are text documents beginning with
`-----BEGIN PGP SIGNED MESSAGE-----`. They have the form:

```text
-----BEGIN PGP SIGNED MESSAGE-----
Hash: <digest>

<normalized signed content>
-----BEGIN PGP SIGNATURE-----
<headers>

<signature data>
-----END PGP SIGNATURE-----
```
*/

use {
    chrono::SubsecRound,
    digest::Digest,
    pgp::{
        crypto::{HashAlgorithm, Hasher},
        packet::{Packet, SignatureConfig, SignatureType, Subpacket},
        types::{KeyVersion, PublicKeyTrait, SecretKeyTrait},
        Signature,
    },
    smallvec::SmallVec,
    std::{
        cmp::Ordering,
        collections::HashMap,
        io::{self, BufRead, BufReader, Cursor, Read},
    },
};

const HEADER: &str = "-----BEGIN PGP SIGNED MESSAGE-----";
const HEADER_LF: &str = "-----BEGIN PGP SIGNED MESSAGE-----\n";
const HEADER_CRLF: &str = "-----BEGIN PGP SIGNED MESSAGE-----\r\n";

const SIGNATURE_ARMOR_LF: &str = "-----BEGIN PGP SIGNATURE-----\n";
const SIGNATURE_ARMOR_CRLF: &str = "-----BEGIN PGP SIGNATURE-----\r\n";

/// Wrapper around content digesting to work around lack of clone() in pgp crate.
#[derive(Clone)]
pub enum CleartextHasher {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl CleartextHasher {
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

impl std::io::Write for CleartextHasher {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Hasher for CleartextHasher {
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
            CleartextHasher::Sha1(digest) => digest.finalize().to_vec(),
            CleartextHasher::Sha256(digest) => digest.finalize().to_vec(),
            CleartextHasher::Sha384(digest) => digest.finalize().to_vec(),
            CleartextHasher::Sha512(digest) => digest.finalize().to_vec(),
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
/// This type is effectively a filtering [Read] implementation. Given a source reader
/// that will emit bytes constituting cleartext signature data, this reader will parse
/// the special syntax defining the cleartext signature and store state in the instance.
/// Only the original / signed cleartext bytes will be returned by `read()` calls.
///
/// Once EOF is reached, call [Self::finalize()] to consume the reader and return a
/// [CleartextSignatures] holding parsed cleartext signature state.
///
/// Important: reading does not validate signatures. Use [CleartextSignatures] after
/// parsing/reading to validate signatures.
pub struct CleartextSignatureReader<R: Read> {
    reader: BufReader<R>,
    state: ReaderState,

    /// Hash types as advertised by the `Hash: ` header.
    hashers: HashMap<u8, CleartextHasher>,

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
                                    "MD5" => CleartextHasher::md5(),
                                    "SHA1" => CleartextHasher::sha1(),
                                    "SHA256" => CleartextHasher::sha256(),
                                    "SHA384" => CleartextHasher::sha384(),
                                    "SHA512" => CleartextHasher::sha512(),
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

/// Parsed cleartext signatures data.
///
/// This type represents the results of parsing cleartext signature data.
///
/// When a document containing PGP cleartext signatures is parsed, [CleartextSignatureReader]
/// derives hashers of the signed content as well as the parsed PGP signature packets. This
/// data is held by this type to facilitate signature verification.
pub struct CleartextSignatures {
    hashers: HashMap<u8, CleartextHasher>,
    signatures: Vec<Signature>,
}

impl CleartextSignatures {
    /// Iterate over signatures in this instance.
    ///
    /// This obtains the parsed signature packets as derived from
    /// `-----BEGIN PGP SIGNATURE-----` sections in the source document.
    pub fn iter_signatures(&self) -> impl Iterator<Item = &Signature> {
        self.signatures.iter()
    }

    /// Iterate over signatures made by a specific key.
    ///
    /// This is a convenience wrapper for [Self::iter_signatures()] that filters based on the
    /// signature's issuer matching the key ID of the specified key.
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
    ///
    /// If there are no signatures at all or no signatures from the specified key, an error is
    /// returned.
    ///
    /// Errors also occur if a signature could not be verified (possibly due to implementation
    /// bugs) or if the signature is invalid.
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
                "no signatures signed by provided key".into(),
            )),
            _ => Ok(valid_signatures),
        }
    }
}

/// Produce a cleartext signature over data.
///
/// The original cleartext data to be signed is provided by a reader.
///
/// The returned value is a multiline string with LF line endings containing the PGP
/// cleartext framework encoded cleartext and signature. The signature is produced by
/// the provided key using the specified hashing algorithm.
///
/// Normalizing the line endings to a different format (e.g. `\r\n` is allowed, as
/// cleartext signature framework readers should properly recognize alternate line
/// endings.
pub fn cleartext_sign<PW, R>(
    key: &impl SecretKeyTrait,
    key_pw: PW,
    hash_algorithm: HashAlgorithm,
    data: R,
) -> pgp::errors::Result<String>
where
    PW: FnOnce() -> String,
    R: BufRead,
{
    if !matches!(
        hash_algorithm,
        HashAlgorithm::MD5
            | HashAlgorithm::SHA1
            | HashAlgorithm::RIPEMD160
            | HashAlgorithm::SHA2_256
            | HashAlgorithm::SHA2_384
            | HashAlgorithm::SHA2_512
            | HashAlgorithm::SHA2_224,
    ) {
        return Err(pgp::errors::Error::Unsupported(
            "hash algorithm unsupported for cleartext signatures".to_string(),
        ));
    }

    // The message digest is computed using the source data. The emitted cleartext
    // signature contains the dash-escaped normalization of the source data. Furthermore,
    // line endings in the source data are normalized to CRLF for signature creation.

    let mut dashed_lines = vec![];
    let mut source_lines = vec![];

    for line in data.lines() {
        let line = line?;

        // From https://datatracker.ietf.org/doc/html/rfc4880.html#section-7.1:
        //
        // Dash-escaped cleartext is the ordinary cleartext where every line
        // starting with a dash '-' (0x2D) is prefixed by the sequence dash '-'
        // (0x2D) and space ' ' (0x20). ... An implementation MAY dash-escape any
        // line, SHOULD dash-escape lines commencing "From" followed by a space, and
        // MUST dash-escape any line commencing in a dash. ... Also, any trailing
        // whitespace -- spaces (0x20) and tabs (0x09) -- at the end of any line is
        // removed when the cleartext signature is generated.
        dashed_lines.push(if line.starts_with('-') || line.starts_with("From ") {
            format!("- {}", line.trim_end())
        } else {
            line.trim_end().to_string()
        });

        source_lines.push(line.trim_end().to_string());
    }

    let cleartext = source_lines.join("\r\n").into_bytes();

    // TODO these sets should be audited by someone who knows PGP.
    let hashed_subpackets = vec![
        Subpacket::IssuerFingerprint(KeyVersion::V4, SmallVec::from_slice(&key.fingerprint())),
        Subpacket::SignatureCreationTime(chrono::Utc::now().trunc_subsecs(0)),
    ];
    let unhashed_subpackets = vec![Subpacket::Issuer(key.key_id())];

    let config = SignatureConfig::new_v4(
        Default::default(),
        SignatureType::Text,
        key.algorithm(),
        hash_algorithm,
        hashed_subpackets,
        unhashed_subpackets,
    );

    let signature = config.sign(key, key_pw, Cursor::new(cleartext))?;

    // The armoring consists of a signature packet.
    let packet = Packet::Signature(signature);
    let mut writer = Cursor::new(Vec::<u8>::new());
    pgp::armor::write(&packet, pgp::armor::BlockType::Signature, &mut writer, None)?;

    // The armoring should always produce valid UTF-8. But we are careful.
    let signature_string = String::from_utf8(writer.into_inner())
        .map_err(|e| pgp::errors::Error::Utf8Error(e.utf8_error()))?;

    // The cleartext consists of the header, the hash identifier, an empty line, the
    // dash-escaped lines, and finally the signature armor.
    let lines = vec![
        HEADER.to_string(),
        format!(
            "Hash: {}",
            match hash_algorithm {
                HashAlgorithm::MD5 => "MD5",
                HashAlgorithm::SHA1 => "SHA1",
                HashAlgorithm::RIPEMD160 => "RIPEMD160",
                HashAlgorithm::SHA2_256 => "SHA256",
                HashAlgorithm::SHA2_384 => "SHA384",
                HashAlgorithm::SHA2_512 => "SHA512",
                HashAlgorithm::SHA2_224 => "SHA224",
                _ => panic!("hash algorithm should have been validated above"),
            }
        ),
        "".to_string(),
    ]
    .into_iter()
    .chain(dashed_lines.into_iter())
    .chain(std::iter::once(signature_string))
    .collect::<Vec<_>>();

    // We could potentially make the line ending configurable, as a cleartext reader
    // must normalize lines.
    Ok(lines.join("\n"))
}
