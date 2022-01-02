// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        binary::{analyze_binary_file_data, BinaryFileInfo},
        db::DatabaseConnection,
    },
    anyhow::{anyhow, Context, Result},
    debian_packaging::{
        binary_package_control::BinaryPackageControlFile,
        deb::reader::{BinaryPackageEntry, BinaryPackageReader, ControlTarFile},
        repository::{BinaryPackageFetch, RepositoryRootReader},
    },
    futures_util::{AsyncReadExt, StreamExt, TryFutureExt},
    std::{io::Read, path::PathBuf},
};

/// Import a collection of Debian packages given a root reader and iterable of fetches.
pub async fn import_debian_packages<'fetch>(
    repo: &(impl RepositoryRootReader + ?Sized),
    fetches: impl Iterator<Item = BinaryPackageFetch<'fetch>>,
    db: &mut DatabaseConnection,
    threads: usize,
) -> Result<()> {
    let mut total_size = 0;

    let known_urls = db.package_urls()?;
    let repo_url = repo.url()?;

    let mut fs = vec![];
    let mut fetch_count = 0;

    for fetch in fetches {
        let url = repo.url()?.join(&fetch.path)?;

        if known_urls.contains(url.as_str()) {
            continue;
        }

        fetch_count += 1;

        let size = fetch.size;
        total_size += size;

        let repo_url = repo_url.clone();
        fs.push(
            fetch_debian_package(repo, fetch).and_then(|(cf, reader, size)| async move {
                let url = repo_url.clone().join(cf.required_field_str("Filename")?)?;

                process_debian_package(reader, size, url.to_string()).await
            }),
        );
    }

    eprintln!("fetching {} packages", fetch_count);

    let mut pb = pbr::ProgressBar::new(total_size);
    pb.set_units(pbr::Units::Bytes);

    let mut buffered = futures::stream::iter(fs).buffer_unordered(threads);

    loop {
        match buffered.next().await {
            None => break,
            Some(Ok(package)) => {
                let size = package.package_size;
                let name = package.name.clone();
                store_indexed_package(db, package)
                    .with_context(|| format!("storing indexed package {}", name))?;
                pb.add(size);
            }
            Some(Err(e)) => {
                eprintln!("error processing package (ignoring): {:?}", e);
            }
        }
    }

    Ok(())
}

pub async fn import_rpm_packages(
    repo: &(impl rpm_repository::RepositoryRootReader + ?Sized),
    packages: impl Iterator<Item = rpm_repository::metadata::primary::Package>,
    db: &mut DatabaseConnection,
    max_concurrency: usize,
) -> Result<()> {
    let mut total_size = 0;

    let known_urls = db.package_urls()?;

    let mut fs = vec![];

    for package in packages {
        let package_url = repo.url()?.join(&package.location.href)?;

        if known_urls.contains(package_url.as_str()) {
            continue;
        }

        let package_size = package.size.package;
        total_size += package_size;

        fs.push(fetch_rpm_package(repo, package).and_then(move |package| {
            process_rpm_package(package_size, package_url.to_string(), package)
        }));
    }

    let mut pb = pbr::ProgressBar::new(total_size);
    pb.set_units(pbr::Units::Bytes);

    let mut buffered = futures::stream::iter(fs).buffer_unordered(max_concurrency);

    loop {
        match buffered.next().await {
            None => break,
            Some(Ok(package)) => {
                let size = package.package_size;
                store_indexed_package(db, package)?;
                pb.add(size);
            }
            Some(Err(e)) => {
                eprintln!("error processing package (ignoring): {:?}", e);
            }
        }
    }

    Ok(())
}

/// Import a Debian package given its `.deb` archive data.
pub async fn import_debian_package_from_data(
    url: &str,
    data: Vec<u8>,
    db: &mut DatabaseConnection,
) -> Result<()> {
    let size = data.len() as u64;
    let reader = BinaryPackageReader::new(std::io::Cursor::new(data))?;

    let package = process_debian_package(reader, size, url.to_string()).await?;

    store_indexed_package(db, package)?;

    Ok(())
}

/// Represents the result of indexing a package.
pub struct IndexedPackage {
    /// The package name.
    pub name: String,
    /// The package version.
    pub version: String,
    /// The URL this package came from.
    pub url: String,
    /// Size in bytes of the package.
    pub package_size: u64,
    /// Files within this package.
    pub files: Vec<PackageFile>,
}

/// A file in a package.
#[derive(Clone, Debug)]
pub struct PackageFile {
    pub path: PathBuf,
    pub size: u64,
    pub binary_info: Option<BinaryFileInfo>,
}

impl PackageFile {
    pub fn from_data(path: PathBuf, data: Vec<u8>) -> Result<Self> {
        let binary_info = match analyze_binary_file_data(&data) {
            Ok(bi) => bi,
            Err(e) => {
                eprintln!("error processing binary file {}: {:?}", path.display(), e);
                None
            }
        };

        Ok(Self {
            path,
            size: data.len() as u64,
            binary_info,
        })
    }
}

async fn fetch_debian_package<'repo, 'fetch>(
    repo: &'repo (impl RepositoryRootReader + ?Sized),
    fetch: BinaryPackageFetch<'fetch>,
) -> Result<(
    BinaryPackageControlFile<'fetch>,
    BinaryPackageReader<std::io::Cursor<Vec<u8>>>,
    u64,
)> {
    let cf = fetch.control_file.clone();
    let size = fetch.size;

    let reader = repo.fetch_binary_package_deb_reader(fetch).await?;

    Ok((cf, reader, size))
}

async fn process_debian_package<'cf>(
    mut deb_reader: BinaryPackageReader<std::io::Cursor<Vec<u8>>>,
    package_size: u64,
    url: String,
) -> Result<IndexedPackage> {
    let mut files = vec![];
    let mut cf = None;

    while let Some(entry) = deb_reader.next_entry() {
        let entry = entry?;

        match entry {
            BinaryPackageEntry::DebianBinary(_) => {}
            BinaryPackageEntry::Control(mut control_reader) => {
                for entry in control_reader.entries()? {
                    let mut entry = entry?;

                    if let ControlTarFile::Control(v) = entry.to_control_file()?.1 {
                        cf = Some(v);
                    };
                }
            }
            BinaryPackageEntry::Data(data_tar) => {
                let mut entries = data_tar.into_inner().entries()?;

                let mut fs = vec![];

                while let Some(entry) = entries.next().await {
                    let mut entry = entry?;

                    if !entry.header().entry_type().is_file() {
                        continue;
                    }

                    let entry_path: PathBuf = entry.path()?.as_ref().to_path_buf().into();

                    let entry_path = entry_path
                        .strip_prefix("./")
                        .unwrap_or(&entry_path)
                        .to_path_buf();

                    let mut file_data = vec![];
                    entry.read_to_end(&mut file_data).await?;

                    // Resolving file content often involves decompression, which can be the
                    // bottleneck. So push all additional processing to a different future.
                    fs.push(tokio::task::spawn_blocking(
                        move || -> Result<PackageFile> {
                            PackageFile::from_data(entry_path, file_data)
                        },
                    ));
                }

                let mut stream = futures::stream::iter(fs);

                while let Some(pf) = stream.next().await {
                    files.push(pf.await??);
                }
            }
        }
    }

    let cf = cf.ok_or_else(|| anyhow!("control file not found"))?;

    Ok(IndexedPackage {
        name: cf.package()?.to_string(),
        version: cf.version_str()?.to_string(),
        url,
        package_size,
        files,
    })
}

async fn fetch_rpm_package(
    repo: &(impl rpm_repository::RepositoryRootReader + ?Sized),
    package: rpm_repository::metadata::primary::Package,
) -> Result<rpm::RPMPackage> {
    let mut reader = repo
        .get_path_with_digest_verification(
            package.location.href,
            package.size.package,
            package.checksum.try_into()?,
        )
        .await?;

    let mut data = vec![];
    reader.read_to_end(&mut data).await?;

    let package = rpm::RPMPackage::parse(&mut std::io::Cursor::new(data))
        .map_err(|e| anyhow!("RPM parse error: {:?}", e))?;

    Ok(package)
}

async fn process_rpm_package(
    package_size: u64,
    url: String,
    package: rpm::RPMPackage,
) -> Result<IndexedPackage> {
    let name = package
        .metadata
        .header
        .get_name()
        .map_err(|e| anyhow!("could not resolve rpm name: {:?}", e))?;
    let version = package
        .metadata
        .header
        .get_version()
        .map_err(|e| anyhow!("could not resolve rpm version: {:?}", e))?;
    let archive_format = package
        .metadata
        .header
        .get_payload_format()
        .map_err(|e| anyhow!("could not resolve rpm archive format: {:?}", e))?;
    let compression = package
        .metadata
        .header
        .get_payload_compressor()
        .map_err(|e| anyhow!("could not resolve rpm compression: {:?}", e))?;

    if archive_format != "cpio" {
        return Err(anyhow!(
            "do not know how to handle archive format {}",
            archive_format
        ));
    }

    let compression = match compression {
        "xz" => rpm_repository::io::Compression::Xz,
        "zstd" => rpm_repository::io::Compression::Zstd,
        _ => {
            return Err(anyhow!(
                "do not know how to handle RPM compression {}",
                compression
            ));
        }
    };

    // We have to read all archive elements anyway. So we go ahead and buffer.
    let mut reader = rpm_repository::io::read_decompressed(
        futures::io::Cursor::new(package.content),
        compression,
    );
    let mut data = vec![];
    reader.read_to_end(&mut data).await?;

    let mut reader = cpio::NewcReader::new(std::io::Cursor::new(data))?;

    let mut files = vec![];
    let mut fs = vec![];

    loop {
        if reader.entry().name() == "TRAILER!!!" {
            break;
        }

        let mut data = Vec::with_capacity(reader.entry().file_size() as _);
        unsafe {
            data.set_len(data.capacity());
        }
        reader.read_exact(&mut data)?;

        let path = PathBuf::from(reader.entry().name());

        let path = path.strip_prefix("./").unwrap_or(&path).to_path_buf();

        fs.push(tokio::task::spawn_blocking(
            move || -> Result<PackageFile> { PackageFile::from_data(path, data) },
        ));

        reader = cpio::NewcReader::new(reader.finish()?)?;
    }

    let mut stream = futures::stream::iter(fs);

    while let Some(pf) = stream.next().await {
        files.push(pf.await??);
    }

    Ok(IndexedPackage {
        name: name.to_string(),
        version: version.to_string(),
        url,
        package_size,
        files,
    })
}

/// Perform SQLite operations to store metadata for an indexed package.
fn store_indexed_package(db: &mut DatabaseConnection, package: IndexedPackage) -> Result<()> {
    db.with_transaction(|txn| {
        txn.store_indexed_package(&package)?;
        txn.commit()?;

        Ok(())
    })
}
