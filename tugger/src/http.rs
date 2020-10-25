// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    sha2::Digest,
    slog::warn,
    std::io::Read,
    url::Url,
};

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
pub fn download_and_verify(logger: &slog::Logger, url: &str, hash: &str) -> Result<Vec<u8>> {
    warn!(logger, "downloading {}", url);
    let url = Url::parse(url)?;
    let client = get_http_client()?;
    let mut response = client.get(url).send()?;

    let mut data: Vec<u8> = Vec::new();
    response.read_to_end(&mut data)?;

    warn!(logger, "validating hash...");
    let mut hasher = sha2::Sha256::new();
    hasher.update(&data);

    let url_hash = hasher.finalize().to_vec();
    let expected_hash = hex::decode(hash)?;

    if expected_hash == url_hash {
        Ok(data)
    } else {
        Err(anyhow!("hash mismatch of downloaded file"))
    }
}
