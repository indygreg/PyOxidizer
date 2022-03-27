// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        format::XarHeader,
        table_of_contents::{ChecksumType, File, FileType, SignatureStyle, TableOfContents},
        Error, XarResult,
    },
    cryptographic_message_syntax::SignedData,
    scroll::IOread,
    std::{
        cmp::min,
        fmt::Debug,
        io::{Read, Seek, SeekFrom, Write},
        path::Path,
    },
    x509_certificate::CapturedX509Certificate,
};

/// Read-only interface to a single XAR archive.
#[derive(Debug)]
pub struct XarReader<R: Read + Seek + Sized + Debug> {
    /// Reader of raw XAR archive content.
    reader: R,

    /// Parsed file header.
    header: XarHeader,

    /// Parsed table of contents.
    toc: TableOfContents,

    /// Absolute offset within the reader where the heap begins.
    heap_start_offset: u64,
}

impl<R: Read + Seek + Sized + Debug> XarReader<R> {
    /// Construct a new XAR reader from a stream reader.
    pub fn new(mut reader: R) -> XarResult<Self> {
        let header = reader.ioread_with::<XarHeader>(scroll::BE)?;

        let mut header_extra = Vec::with_capacity(header.size as usize - 28);
        header_extra.resize(header_extra.capacity(), 0);
        reader.read_exact(&mut header_extra)?;

        // Following the header is a zlib compressed table of contents.
        // Unfortunately, serde_xml_rs takes ownership of the reader and doesn't
        // allow returning it. So we have to buffer decompressed data before feeding
        // it to the XML parser.
        let toc_reader = reader.take(header.toc_length_compressed);
        let mut toc_reader = flate2::read::ZlibDecoder::new(toc_reader);

        let mut toc_data = Vec::with_capacity(header.toc_length_uncompressed as _);
        toc_reader.read_to_end(&mut toc_data)?;

        let mut reader = toc_reader.into_inner().into_inner();
        let heap_start_offset = reader.stream_position()?;

        let toc = TableOfContents::from_reader(std::io::Cursor::new(toc_data))?;

        Ok(Self {
            reader,
            header,
            toc,
            heap_start_offset,
        })
    }

    /// Obtain the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Obtain the parsed [XarHeader] file header.
    pub fn header(&self) -> &XarHeader {
        &self.header
    }

    /// The start offset of the heap.
    pub fn heap_start_offset(&self) -> u64 {
        self.heap_start_offset
    }

    /// Obtain the table of contents for this archive.
    pub fn table_of_contents(&self) -> &TableOfContents {
        &self.toc
    }

    /// Obtain the raw bytes holding the checksum.
    pub fn checksum_data(&mut self) -> XarResult<Vec<u8>> {
        let mut buf = Vec::with_capacity(self.toc.checksum.size as _);

        self.write_heap_slice(
            self.toc.checksum.offset,
            self.toc.checksum.size as _,
            &mut buf,
        )?;

        Ok(buf)
    }

    /// Obtain the file entries in this archive.
    pub fn files(&self) -> XarResult<Vec<(String, File)>> {
        self.toc.files()
    }

    /// Attempt to find the [File] entry for a given path in the archive.
    pub fn find_file(&self, filename: &str) -> XarResult<Option<File>> {
        Ok(self
            .toc
            .files()?
            .into_iter()
            .find_map(|(path, file)| if path == filename { Some(file) } else { None }))
    }

    /// Write a slice of the heap to a writer.
    fn write_heap_slice(
        &mut self,
        offset: u64,
        size: usize,
        writer: &mut impl Write,
    ) -> XarResult<()> {
        self.reader
            .seek(SeekFrom::Start(self.heap_start_offset + offset))?;

        let mut remaining = size;
        let mut buffer = Vec::with_capacity(32768);
        buffer.resize(min(remaining, buffer.capacity()), 0);

        while remaining > 0 {
            self.reader.read_exact(&mut buffer)?;
            remaining -= buffer.len();
            writer.write_all(&buffer)?;

            unsafe {
                buffer.set_len(min(remaining, buffer.capacity()));
            }
        }

        Ok(())
    }

    /// Write heap file data for a given file record to a writer.
    ///
    /// This will write the raw data backing a file as stored in the heap.
    /// There's a good chance the raw data is encoded/compressed.
    ///
    /// Returns the number of bytes written.
    pub fn write_file_data_heap_from_file(
        &mut self,
        file: &File,
        writer: &mut impl Write,
    ) -> XarResult<usize> {
        let data = file.data.as_ref().ok_or(Error::FileNoData)?;

        self.write_heap_slice(data.offset, data.length as _, writer)?;

        Ok(data.length as _)
    }

    /// Write heap file data for a given file ID to a writer.
    ///
    /// This is a wrapper around [Self::write_file_data_heap_from_file] that
    /// resolves the [File] given a file ID.
    pub fn write_file_data_heap_from_id(
        &mut self,
        id: u64,
        writer: &mut impl Write,
    ) -> XarResult<usize> {
        let file = self
            .toc
            .files()?
            .into_iter()
            .find(|(_, f)| f.id == id)
            .ok_or(Error::InvalidFileId)?
            .1;

        self.write_file_data_heap_from_file(&file, writer)
    }

    /// Write decoded file data for a given file record to a writer.
    ///
    /// This will call [Self::write_file_data_heap_from_file] and will decode
    /// that data stream, if the file data is encoded.
    pub fn write_file_data_decoded_from_file(
        &mut self,
        file: &File,
        writer: &mut impl Write,
    ) -> XarResult<usize> {
        let data = file.data.as_ref().ok_or(Error::FileNoData)?;

        let mut writer = match data.encoding.style.as_str() {
            "application/octet-stream" => Box::new(writer) as Box<dyn Write>,
            "application/x-bzip2" => {
                Box::new(bzip2::write::BzDecoder::new(writer)) as Box<dyn Write>
            }
            // The media type is arguably wrong, as there is no gzip header.
            "application/x-gzip" => {
                Box::new(flate2::write::ZlibDecoder::new(writer)) as Box<dyn Write>
            }
            "application/x-lzma" => Box::new(xz2::write::XzDecoder::new(writer)) as Box<dyn Write>,
            encoding => {
                return Err(Error::UnimplementedFileEncoding(encoding.to_string()));
            }
        };

        self.write_file_data_heap_from_file(file, &mut writer)
    }

    /// Write decoded file data for a given file ID to a writer.
    ///
    /// This is a wrapper for [Self::write_file_data_decoded_from_file] that locates
    /// the [File] entry given a file ID.
    pub fn write_file_data_decoded_from_id(
        &mut self,
        id: u64,
        writer: &mut impl Write,
    ) -> XarResult<usize> {
        let file = self
            .toc
            .files()?
            .into_iter()
            .find(|(_, f)| f.id == id)
            .ok_or(Error::InvalidFileId)?
            .1;

        self.write_file_data_decoded_from_file(&file, writer)
    }

    /// Resolve data for a given path.
    pub fn get_file_data_from_path(&mut self, path: &str) -> XarResult<Option<Vec<u8>>> {
        if let Some(file) = self.find_file(path)? {
            let mut buffer = Vec::<u8>::with_capacity(file.size.unwrap_or(0) as _);
            self.write_file_data_decoded_from_file(&file, &mut buffer)?;

            Ok(Some(buffer))
        } else {
            Ok(None)
        }
    }

    /// Unpack the contents of the XAR archive to a given directory.
    pub fn unpack(&mut self, dest_dir: impl AsRef<Path>) -> XarResult<()> {
        let dest_dir = dest_dir.as_ref();

        for (path, file) in self.toc.files()? {
            let dest_path = dest_dir.join(path);

            match file.file_type {
                FileType::Directory => {
                    std::fs::create_dir(&dest_path)?;
                }
                FileType::File => {
                    let mut fh = std::fs::File::create(&dest_path)?;
                    self.write_file_data_decoded_from_file(&file, &mut fh)?;
                }
                FileType::HardLink => return Err(Error::Unsupported("writing hard links")),
                FileType::Link => return Err(Error::Unsupported("writing symlinks")),
            }
        }

        Ok(())
    }

    /// Obtain the archive checksum.
    ///
    /// The checksum consists of a digest format and a raw digest.
    pub fn checksum(&mut self) -> XarResult<(ChecksumType, Vec<u8>)> {
        let mut data = Vec::<u8>::with_capacity(self.toc.checksum.size as _);
        self.write_heap_slice(
            self.toc.checksum.offset,
            self.toc.checksum.size as _,
            &mut data,
        )?;

        Ok((self.toc.checksum.style, data))
    }

    /// Obtain RSA signature data from this archive.
    ///
    /// The returned tuple contains the raw signature data and the embedded X.509 certificates.
    pub fn rsa_signature(&mut self) -> XarResult<Option<(Vec<u8>, Vec<CapturedX509Certificate>)>> {
        if let Some(sig) = self.toc.find_signature(SignatureStyle::Rsa).cloned() {
            let mut data = Vec::<u8>::with_capacity(sig.size as _);
            self.write_heap_slice(sig.offset, sig.size as _, &mut data)?;

            let certs = sig.x509_certificates()?;

            Ok(Some((data, certs)))
        } else {
            Ok(None)
        }
    }

    /// Verifies the RSA signature in the archive.
    ///
    /// This verifies that the RSA signature in the archive, if present, is a valid signature
    /// for the archive's checksum data.
    ///
    /// The boolean return value indicates if signature validation was performed.
    pub fn verify_rsa_checksum_signature(&mut self) -> XarResult<bool> {
        let signed_data = self.checksum()?.1;

        if let Some((signature, certificates)) = self.rsa_signature()? {
            // The first certificate is the signing certificate.
            if let Some(cert) = certificates.get(0) {
                cert.verify_signed_data(signed_data, signature)?;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Attempt to resolve a cryptographic message syntax (CMS) signature.
    ///
    /// The data signed by the CMS signature is the raw data returned by [Self::checksum].
    pub fn cms_signature(&mut self) -> XarResult<Option<SignedData>> {
        if let Some(sig) = self.toc.find_signature(SignatureStyle::Cms).cloned() {
            let mut data = Vec::<u8>::with_capacity(sig.size as _);
            self.write_heap_slice(sig.offset, sig.size as _, &mut data)?;

            Ok(Some(SignedData::parse_ber(&data)?))
        } else {
            Ok(None)
        }
    }

    /// Verifies the cryptographic message syntax (CMS) signature, if present.
    pub fn verify_cms_signature(&mut self) -> XarResult<bool> {
        let checksum = self.checksum()?.1;
        let mut checked = false;

        if let Some(signed_data) = self.cms_signature()? {
            for signer in signed_data.signers() {
                signer.verify_signature_with_signed_data(&signed_data)?;
                signer.verify_message_digest_with_content(&checksum)?;
                checked = true;
            }
        }

        Ok(checked)
    }
}
