// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    sha2::Digest,
    slog::warn,
    std::{io::Read, path::Path},
    url::Url,
};

/// Defines remote content that can be downloaded securely.
pub struct RemoteContent {
    pub url: String,
    pub sha256: String,
}

fn sha256_path<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut hasher = sha2::Sha256::new();
    let fh = std::fs::File::open(&path)?;
    let mut reader = std::io::BufReader::new(fh);

    let mut buffer = [0; 32768];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hasher.finalize().to_vec())
}

/// Obtain an HTTP client, taking proxy environment variables into account.
pub fn get_http_client() -> reqwest::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::ClientBuilder::new();

    for (key, value) in std::env::vars() {
        let key = key.to_lowercase();
        if key.ends_with("_proxy") {
            let end = key.len() - "_proxy".len();
            let schema = &key[..end];

            if let Ok(url) = Url::parse(&value) {
                if let Some(proxy) = match schema {
                    "http" => Some(reqwest::Proxy::http(url.as_str())),
                    "https" => Some(reqwest::Proxy::https(url.as_str())),
                    _ => None,
                } {
                    if let Ok(proxy) = proxy {
                        builder = builder.proxy(proxy);
                    }
                }
            }
        }
    }

    builder.build()
}

/// Fetch a URL and verify its SHA-256 matches expectations.
pub fn download_and_verify(logger: &slog::Logger, entry: &RemoteContent) -> Result<Vec<u8>> {
    warn!(logger, "downloading {}", entry.url);
    let url = Url::parse(&entry.url)?;
    let client = get_http_client()?;
    let mut response = client.get(url).send()?;

    let mut data: Vec<u8> = Vec::new();
    response.read_to_end(&mut data)?;

    warn!(logger, "validating hash...");
    let mut hasher = sha2::Sha256::new();
    hasher.update(&data);

    let url_hash = hasher.finalize().to_vec();
    let expected_hash = hex::decode(&entry.sha256)?;

    if expected_hash == url_hash {
        Ok(data)
    } else {
        Err(anyhow!("hash mismatch of downloaded file"))
    }
}

/// Ensure a URL with specified hash exists in a local filesystem path.
pub fn download_to_path<P: AsRef<Path>>(
    logger: &slog::Logger,
    entry: &RemoteContent,
    dest_path: P,
) -> Result<()> {
    let dest_path = dest_path.as_ref();

    let expected_hash = hex::decode(&entry.sha256)?;

    if dest_path.exists() {
        let file_hash = sha256_path(dest_path)?;

        if file_hash == expected_hash {
            return Ok(());
        }

        // Hash mismatch. Remove the current file.
        std::fs::remove_file(dest_path)?;
    }

    let data = download_and_verify(logger, entry)?;
    let temp_path = dest_path.with_file_name(format!(
        "{}.tmp",
        dest_path
            .file_name()
            .ok_or_else(|| anyhow!("unable to obtain file name"))?
            .to_string_lossy()
    ));

    std::fs::write(&temp_path, data)?;
    std::fs::rename(&temp_path, dest_path)?;

    Ok(())
}
