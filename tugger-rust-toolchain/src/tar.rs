// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    sha2::Digest,
    simple_file_manifest::{FileEntry, FileManifest},
    std::{
        io::{BufRead, Read, Write},
        path::{Path, PathBuf},
    },
};

#[derive(Clone, Copy, Debug)]
pub enum CompressionFormat {
    Gzip,
    Xz,
    Zstd,
}

fn get_decompression_stream(format: CompressionFormat, data: Vec<u8>) -> Result<Box<dyn Read>> {
    let reader = std::io::Cursor::new(data);

    match format {
        CompressionFormat::Zstd => Ok(Box::new(zstd::stream::read::Decoder::new(reader)?)),
        CompressionFormat::Xz => Ok(Box::new(xz2::read::XzDecoder::new(reader))),
        CompressionFormat::Gzip => Ok(Box::new(flate2::read::GzDecoder::new(reader))),
    }
}

/// Represents an extracted Rust package archive.
///
/// File contents exist in memory.
pub struct PackageArchive {
    manifest: FileManifest,
    components: Vec<String>,
}

impl PackageArchive {
    /// Construct a new instance with compressed tar data.
    pub fn new(format: CompressionFormat, data: Vec<u8>) -> Result<Self> {
        let mut archive = tar::Archive::new(
            get_decompression_stream(format, data).context("obtaining decompression stream")?,
        );

        let mut manifest = FileManifest::default();

        for entry in archive.entries().context("obtaining tar archive entries")? {
            let mut entry = entry.context("resolving tar archive entry")?;

            let path = entry.path().context("resolving entry path")?;

            let first_component = path
                .components()
                .next()
                .ok_or_else(|| anyhow!("unable to get first path component"))?;

            let path = path
                .strip_prefix(first_component)
                .context("stripping path prefix")?
                .to_path_buf();

            let mut entry_data = Vec::new();
            entry.read_to_end(&mut entry_data)?;

            manifest.add_file_entry(
                path,
                FileEntry::new_from_data(entry_data, entry.header().mode()? & 0o111 != 0),
            )?;
        }

        if manifest
            .get("rust-installer-version")
            .ok_or_else(|| anyhow!("archive does not contain rust-installer-version"))?
            .resolve_content()?
            != b"3\n"
        {
            return Err(anyhow!("rust-installer-version has unsupported version"));
        }

        let components = manifest
            .get("components")
            .ok_or_else(|| anyhow!("archive does not contain components file"))?
            .resolve_content()?;
        let components =
            String::from_utf8(components).context("converting components file to string")?;
        let components = components
            .lines()
            .map(|l| l.to_string())
            .collect::<Vec<_>>();

        Ok(Self {
            manifest,
            components,
        })
    }

    /// Resolve file installs that need to be performed to materialize this package.
    ///
    /// Returned Vec has relative destination path and the FileManifest's internal entry
    /// as members.
    pub fn resolve_installs(&self) -> Result<Vec<(PathBuf, &FileEntry)>> {
        let mut res = Vec::new();

        for component in &self.components {
            let component_path = PathBuf::from(component);
            let manifest_path = component_path.join("manifest.in");

            let manifest = self
                .manifest
                .get(&manifest_path)
                .ok_or_else(|| anyhow!("{} not found", manifest_path.display()))?;

            let (dirs, files) = Self::parse_manifest(manifest.resolve_content()?)?;

            if !dirs.is_empty() {
                return Err(anyhow!("support for copying directories not implemented"));
            }

            for file in files {
                let manifest_path = component_path.join(&file);
                let entry = self.manifest.get(&manifest_path).ok_or_else(|| {
                    anyhow!(
                        "could not locate file {} in manifest",
                        manifest_path.display()
                    )
                })?;

                res.push((PathBuf::from(file), entry));
            }
        }

        Ok(res)
    }

    /// Write a file containing SHA-256 hashes of file installs to the specified writer.
    pub fn write_installs_manifest(&self, fh: &mut impl Write) -> Result<()> {
        for (path, entry) in self.resolve_installs().context("resolving installs")? {
            let mut hasher = sha2::Sha256::new();
            hasher.update(entry.resolve_content()?);

            let line = format!(
                "{}\t{}\n",
                hex::encode(hasher.finalize().as_slice()),
                path.display()
            );

            fh.write_all(line.as_bytes())?;
        }

        Ok(())
    }

    /// Materialize files from this manifest into the specified destination directory.
    pub fn install(&self, dest_dir: &Path) -> Result<()> {
        for (dest_path, entry) in self.resolve_installs().context("resolving installs")? {
            let dest_path = dest_dir.join(dest_path);

            entry
                .write_to_path(&dest_path)
                .with_context(|| format!("writing {}", dest_path.display(),))?;
        }

        Ok(())
    }

    fn parse_manifest(data: Vec<u8>) -> Result<(Vec<String>, Vec<String>)> {
        let mut files = vec![];
        let mut dirs = vec![];

        let data = String::from_utf8(data)?;

        for line in data.lines() {
            if let Some(pos) = line.find(':') {
                let action = &line[0..pos];
                let path = &line[pos + 1..];

                match action {
                    "file" => {
                        files.push(path.to_string());
                    }
                    "dir" => {
                        dirs.push(path.to_string());
                    }
                    _ => return Err(anyhow!("unhandled action in manifest.in: {}", action)),
                }
            }
        }

        Ok((dirs, files))
    }
}

/// Read an installs manifest from a given reader.
///
/// Returns a mapping of filesystem path to expected SHA-256 digest.
pub fn read_installs_manifest(fh: &mut impl Read) -> Result<Vec<(PathBuf, String)>> {
    let mut res = vec![];

    let reader = std::io::BufReader::new(fh);

    for line in reader.lines() {
        let line = line?;

        if line.is_empty() {
            break;
        }

        let mut parts = line.splitn(2, '\t');

        let digest = parts
            .next()
            .ok_or_else(|| anyhow!("could not read digest"))?;
        let filename = parts
            .next()
            .ok_or_else(|| anyhow!("could not read filename"))?;

        res.push((PathBuf::from(filename), digest.to_string()));
    }

    Ok(res)
}
