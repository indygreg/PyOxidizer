// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::error::{Result, RpmRepositoryError},
    async_compression::futures::bufread::{GzipDecoder, XzDecoder, ZstdDecoder},
    futures::{AsyncBufRead, AsyncRead},
    pin_project::pin_project,
    std::{
        fmt::Formatter,
        pin::Pin,
        task::{Context, Poll},
    },
};

/// Compression format.
pub enum Compression {
    /// No compression.
    None,
    /// Gzip compression.
    Gzip,
    /// Xz compression.
    Xz,
    /// Zstd compression.
    Zstd,
}

pub fn read_decompressed<'a>(
    stream: impl AsyncBufRead + Send + 'a,
    compression: Compression,
) -> Pin<Box<dyn AsyncRead + Send + 'a>> {
    match compression {
        Compression::None => Box::pin(stream),
        Compression::Gzip => Box::pin(GzipDecoder::new(stream)),
        Compression::Xz => Box::pin(XzDecoder::new(stream)),
        Compression::Zstd => Box::pin(ZstdDecoder::new(stream)),
    }
}

pub enum DigestFlavor {
    Sha1,
    Sha256,
}

/// Represents a content digest.
#[derive(Clone, Eq, PartialEq, PartialOrd)]
pub enum ContentDigest {
    /// A SHA-1 digest.
    Sha1(Vec<u8>),
    /// A SHA-256 digest.
    Sha256(Vec<u8>),
}

impl std::fmt::Debug for ContentDigest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sha1(data) => write!(f, "Sha1({})", hex::encode(data)),
            Self::Sha256(data) => write!(f, "Sha256({})", hex::encode(data)),
        }
    }
}

impl ContentDigest {
    /// Create a new SHA-1 instance by parsing a hex digest.
    pub fn sha1_hex(digest: &str) -> Result<Self> {
        Self::from_hex_digest(DigestFlavor::Sha1, digest)
    }

    /// Create a new SHA-256 instance by parsing a hex digest.
    pub fn sha256_hex(digest: &str) -> Result<Self> {
        Self::from_hex_digest(DigestFlavor::Sha256, digest)
    }

    /// Obtain an instance by parsing a hex string as a [ChecksumType].
    pub fn from_hex_digest(checksum: DigestFlavor, digest: &str) -> Result<Self> {
        let digest = hex::decode(digest)
            .map_err(|e| RpmRepositoryError::ContentDigestBadHex(digest.to_string(), e))?;

        Ok(match checksum {
            DigestFlavor::Sha1 => Self::Sha1(digest),
            DigestFlavor::Sha256 => Self::Sha256(digest),
        })
    }

    /// Create a new hasher matching for the type of this digest.
    pub fn new_hasher(&self) -> Box<dyn digest::DynDigest + Send> {
        match self {
            Self::Sha1(_) => Box::new(sha1::Sha1::default()),
            Self::Sha256(_) => Box::new(sha2::Sha256::default()),
        }
    }

    /// Obtain the digest bytes for this content digest.
    pub fn digest_bytes(&self) -> &[u8] {
        match self {
            Self::Sha1(x) => x,
            Self::Sha256(x) => x,
        }
    }

    /// Obtain the hex encoded content digest.
    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest_bytes())
    }

    /// Obtain the [ChecksumType] for this digest.
    pub fn digest_type(&self) -> DigestFlavor {
        match self {
            Self::Sha1(_) => DigestFlavor::Sha1,
            Self::Sha256(_) => DigestFlavor::Sha256,
        }
    }
}

#[pin_project]
pub struct ContentValidatingReader<R> {
    hasher: Option<Box<dyn digest::DynDigest + Send>>,
    expected_size: u64,
    expected_digest: ContentDigest,
    #[pin]
    source: R,
    bytes_read: u64,
}

impl<R> ContentValidatingReader<R> {
    pub fn new(source: R, expected_size: u64, expected_digest: ContentDigest) -> Self {
        Self {
            hasher: Some(expected_digest.new_hasher()),
            expected_size,
            expected_digest,
            source,
            bytes_read: 0,
        }
    }
}

impl<R> AsyncRead for ContentValidatingReader<R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut this = self.project();

        match this.source.as_mut().poll_read(cx, buf) {
            Poll::Ready(Ok(size)) => {
                if size > 0 {
                    if let Some(hasher) = this.hasher.as_mut() {
                        hasher.update(&buf[0..size]);
                    } else {
                        panic!("hasher destroyed prematurely");
                    }

                    *this.bytes_read += size as u64;
                }

                match this.bytes_read.cmp(&this.expected_size) {
                    std::cmp::Ordering::Equal => {
                        if let Some(hasher) = this.hasher.take() {
                            let got_digest = hasher.finalize();

                            if got_digest.as_ref() != this.expected_digest.digest_bytes() {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    format!(
                                        "digest mismatch of retrieved content: expected {}, got {}",
                                        this.expected_digest.digest_hex(),
                                        hex::encode(got_digest)
                                    ),
                                )));
                            }
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!(
                                "extra bytes read: expected {}; got {}",
                                this.expected_size, this.bytes_read
                            ),
                        )));
                    }
                    std::cmp::Ordering::Less => {}
                }

                Poll::Ready(Ok(size))
            }
            res => res,
        }
    }
}
