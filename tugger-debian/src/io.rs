// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! I/O helpers. */

use {
    crate::pgp::MyHasher,
    async_compression::futures::bufread::{BzDecoder, GzipDecoder, LzmaDecoder, XzDecoder},
    futures::{AsyncBufRead, AsyncRead},
    pgp::crypto::Hasher,
    pin_project::pin_project,
    std::{
        pin::Pin,
        task::{Context, Poll},
    },
};

/// Represents a content digest.
pub enum ContentDigest {
    /// An MD5 digest.
    Md5(Vec<u8>),
    /// A SHA-1 digest.
    Sha1(Vec<u8>),
    /// A SHA-256 digest.
    Sha256(Vec<u8>),
}

impl ContentDigest {
    /// Create a new hasher matching for the type of this digest.
    pub fn new_hasher(&self) -> Box<dyn Hasher + Send> {
        Box::new(match self {
            Self::Md5(_) => MyHasher::md5(),
            Self::Sha1(_) => MyHasher::sha1(),
            Self::Sha256(_) => MyHasher::sha256(),
        })
    }

    /// Obtain the digest bytes for this content digest.
    pub fn digest_bytes(&self) -> &[u8] {
        match self {
            Self::Md5(x) => x,
            Self::Sha1(x) => x,
            Self::Sha256(x) => x,
        }
    }

    /// Obtain the hex encoded content digest.
    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest_bytes())
    }
}

/// Compression format used by Debian primitives.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Compression {
    /// No compression (no extension).
    None,

    /// XZ compression (.xz extension).
    Xz,

    /// Gzip compression (.gz extension).
    Gzip,

    /// Bzip2 compression (.bz2 extension).
    Bzip2,

    /// LZMA compression (.lzma extension).
    Lzma,
}

impl Compression {
    /// Filename extension for files compressed in this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Xz => ".xz",
            Self::Gzip => ".gz",
            Self::Bzip2 => ".bz2",
            Self::Lzma => ".lzma",
        }
    }

    /// The default retrieval preference order for client.
    pub fn default_preferred_order() -> impl Iterator<Item = Compression> {
        [Self::Xz, Self::Lzma, Self::Gzip, Self::Bzip2, Self::None].into_iter()
    }
}

/// Wrap a reader with transparent decompression.
pub async fn read_decompressed(
    stream: Pin<Box<dyn AsyncBufRead + Send>>,
    compression: Compression,
) -> Result<Pin<Box<dyn AsyncRead + Send>>, std::io::Error> {
    Ok(match compression {
        Compression::None => Box::pin(stream),
        Compression::Gzip => Box::pin(GzipDecoder::new(stream)),
        Compression::Xz => Box::pin(XzDecoder::new(stream)),
        Compression::Bzip2 => Box::pin(BzDecoder::new(stream)),
        Compression::Lzma => Box::pin(LzmaDecoder::new(stream)),
    })
}

/// Drain content from a reader to a black hole.
pub async fn drain_reader(reader: impl AsyncRead) -> std::io::Result<u64> {
    let mut sink = futures::io::sink();
    futures::io::copy(reader, &mut sink).await
}

/// An adapter for [AsyncRead] streams that validates source size and digest.
///
/// Validation only occurs once the expected source size bytes have been read.
///
/// If the reader consumes less than the expected number of bytes, no validation
/// occurs and incorrect data could have been read. Therefore it is **strongly recommended**
/// for readers to drain this reader. e.g. by using [drain_reader()].
#[pin_project]
pub struct ContentValidatingReader<R> {
    hasher: Option<Box<dyn pgp::crypto::Hasher + Send>>,
    expected_size: usize,
    expected_digest: ContentDigest,
    #[pin]
    source: R,
    bytes_read: usize,
}

impl<R> ContentValidatingReader<R> {
    /// Create a new instance bound to a source and having expected size and content digest.
    pub fn new(source: R, expected_size: usize, expected_digest: ContentDigest) -> Self {
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

                    *this.bytes_read += size;
                }

                match this.bytes_read.cmp(&this.expected_size) {
                    std::cmp::Ordering::Equal => {
                        if let Some(hasher) = this.hasher.take() {
                            let got_digest = hasher.finish();

                            if got_digest != this.expected_digest.digest_bytes() {
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

/// Holds multiple flavors of content digests.
pub struct MultiContentDigest {
    pub md5: ContentDigest,
    pub sha1: ContentDigest,
    pub sha256: ContentDigest,
}

/// A content digester that simultaneously computes multiple digest types.
pub struct MultiDigester {
    md5: Box<dyn Hasher + Send>,
    sha1: Box<dyn Hasher + Send>,
    sha256: Box<dyn Hasher + Send>,
}

impl Default for MultiDigester {
    fn default() -> Self {
        Self {
            md5: Box::new(MyHasher::md5()),
            sha1: Box::new(MyHasher::sha1()),
            sha256: Box::new(MyHasher::sha256()),
        }
    }
}

impl MultiDigester {
    /// Write content into the digesters.
    pub fn update(&mut self, data: &[u8]) {
        self.md5.update(data);
        self.sha1.update(data);
        self.sha256.update(data);
    }

    /// Finish digesting content.
    ///
    /// Consumes the instance and returns a [MultiContentDigest] holding all the digests.
    pub fn finish(self) -> MultiContentDigest {
        MultiContentDigest {
            md5: ContentDigest::Md5(self.md5.finish()),
            sha1: ContentDigest::Sha1(self.sha1.finish()),
            sha256: ContentDigest::Sha256(self.sha256.finish()),
        }
    }
}

/// An [AsyncRead] stream adapter that computes multiple [ContentDigest] as data is read.
#[pin_project]
pub struct DigestingReader<R> {
    digester: MultiDigester,
    #[pin]
    source: R,
}

impl<R> DigestingReader<R> {
    /// Construct a new instance from a source reader.
    pub fn new(source: R) -> Self {
        Self {
            digester: MultiDigester::default(),
            source,
        }
    }

    /// Finish the stream.
    ///
    /// Returns the source reader and a resolved [MultiContentDigest].
    pub fn finish(self) -> (R, MultiContentDigest) {
        (self.source, self.digester.finish())
    }
}

impl<R> AsyncRead for DigestingReader<R>
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
                    this.digester.update(&buf[0..size]);
                }

                Poll::Ready(Ok(size))
            }
            res => res,
        }
    }
}
