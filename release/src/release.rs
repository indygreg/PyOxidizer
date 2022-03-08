// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    clap::ArgMatches,
    futures::StreamExt,
    octocrab::{
        models::{repos::Release, workflows::WorkflowListArtifact},
        Octocrab, OctocrabBuilder,
    },
    sha2::{Digest, Sha256},
    std::{
        collections::BTreeMap,
        io::{Read, Write},
    },
    zip::ZipArchive,
};

async fn fetch_artifact(
    client: &Octocrab,
    artifact: WorkflowListArtifact,
) -> Result<(String, bytes::Bytes)> {
    println!("downloading {}", artifact.name);
    let res = client
        .execute(client.request_builder(artifact.archive_download_url, reqwest::Method::GET))
        .await?;

    let data = res.bytes().await?;

    Ok((artifact.name.clone(), data))
}

fn create_single_file_zip(file_name: &str, data: &[u8]) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(vec![]));

    let options = zip::write::FileOptions::default().unix_permissions(0o777);
    zip.start_file(file_name, options)?;
    zip.write_all(data)?;

    Ok(zip.finish()?.into_inner())
}

async fn upload_release_artifact(
    client: &Octocrab,
    release: &Release,
    filename: &str,
    data: Vec<u8>,
    dry_run: bool,
) -> Result<()> {
    if release.assets.iter().any(|asset| asset.name == filename) {
        println!("release asset {} already present; skipping", filename);
        return Ok(());
    }

    let mut url = release.upload_url.clone();
    let path = url.path().to_string();

    if let Some(path) = path.strip_suffix("%7B") {
        url.set_path(path);
    }

    url.query_pairs_mut().clear().append_pair("name", filename);

    println!("uploading to {}", url);

    let request = client
        .request_builder(url, reqwest::Method::POST)
        .header("Content-Length", data.len())
        .header("Content-Type", "application/x-tar")
        .body(data);

    if dry_run {
        return Ok(());
    }

    let response = client.execute(request).await?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP {}", response.status()));
    }

    Ok(())
}

pub async fn command_upload_release_artifacts(args: &ArgMatches) -> Result<()> {
    let org = args
        .value_of("organization")
        .expect("organization argument is required");
    let repo = args.value_of("repo").expect("repo argument is required");
    let version = args
        .value_of("version")
        .expect("version argument is required");
    let commit = args
        .value_of("commit")
        .expect("commit argument is required");
    let dry_run = args.is_present("dry_run");
    let pypi_registry = args
        .value_of("pypi_registry")
        .expect("pypi_registry should have default value");
    let pypi_username = args.value_of("pypi_username");
    let pypi_password = args.value_of("pypi_password");

    let home = dirs::home_dir().expect("unable to resolve home directory");
    let token_path = home.join(".github-token");

    let github_token = std::fs::read_to_string(&token_path)
        .context("reading ~/.github-token")?
        .trim()
        .to_string();

    let client = OctocrabBuilder::new()
        .personal_token(github_token)
        .build()?;

    let gh_repo = client.repos(org, repo);
    let release = gh_repo
        .releases()
        .get_by_tag(&format!("pyoxidizer/{}", version))
        .await?;

    let workflows = client.workflows(org, repo);

    let page = workflows
        .list_all_runs()
        .event("push")
        .status("success")
        .branch("stable")
        .send()
        .await?;

    let runs = client
        .all_pages::<octocrab::models::workflows::Run>(page)
        .await?
        .into_iter()
        .filter(|run| run.head_sha == commit)
        .collect::<Vec<_>>();

    let mut fs = vec![];

    for run in runs.into_iter().filter(|run| run.head_sha == commit) {
        if !matches!(
            run.name.as_str(),
            ".github/workflows/release.yml" | ".github/workflows/oxidized_importer.yml"
        ) {
            continue;
        }

        let page = client
            .actions()
            .list_workflow_run_artifacts(org, repo, run.id)
            .send()
            .await?;

        let artifacts = client
            .all_pages::<octocrab::models::workflows::WorkflowListArtifact>(
                page.value.expect("untagged request should have page"),
            )
            .await?;

        for artifact in artifacts {
            fs.push(fetch_artifact(&client, artifact));
        }
    }

    let mut buffered = futures::stream::iter(fs).buffer_unordered(4);

    let mut upload_artifacts = vec![];

    // Collect artifacts to release.
    while let Some(res) = buffered.next().await {
        let (artifact_name, data) = res?;

        let mut za = ZipArchive::new(std::io::Cursor::new(data))?;

        for zi in 0..za.len() {
            let mut zf = za.by_index(zi)?;

            let name = zf.name().to_string();

            let mut buf = vec![];
            zf.read_to_end(&mut buf)?;

            if !zf.is_file() {
                continue;
            }

            match artifact_name.as_str() {
                "pyoxidizer-wheels" | "wheels" => {
                    if name.ends_with(".whl") {
                        upload_artifacts.push((name, buf));
                    }
                }
                "pyoxidizer-windows_installers" => {
                    let name = name.replace(".exe", "-installer.exe");

                    upload_artifacts.push((name, buf));
                }
                "pyoxidizer-macos_dmg" => {
                    upload_artifacts.push((format!("PyOxidizer-{}.dmg", version), buf));
                }
                "pyoxidizer-linux-x86_64-bin" => {
                    let data = create_single_file_zip("pyoxidizer", &buf)?;

                    upload_artifacts
                        .push((format!("PyOxidizer-{}-exe-x86-64-linux.zip", version), data));
                }
                "pyoxidizer-macos_exes" | "pyoxidizer-macos_universal_exe" => match name.as_str() {
                    "aarch64-apple-darwin/pyoxidizer" => {
                        let data = create_single_file_zip("pyoxidizer", &buf)?;

                        upload_artifacts.push((
                            format!("PyOxidizer-{}-exe-aarch64-apple-darwin.zip", version),
                            data,
                        ));
                    }
                    "x86_64-apple-darwin/pyoxidizer" => {
                        let data = create_single_file_zip("pyoxidizer", &buf)?;

                        upload_artifacts.push((
                            format!("PyOxidizer-{}-exe-x86_64-apple-darwin.zip", version),
                            data,
                        ));
                    }
                    "macos-universal/pyoxidizer" => {
                        let data = create_single_file_zip("pyoxidizer", &buf)?;

                        upload_artifacts.push((
                            format!("PyOxidizer-{}-exe-macos-universal.zip", version),
                            data,
                        ));
                    }
                    _ => {
                        return Err(anyhow!("unexpected file in {}: {}", artifact_name, name));
                    }
                },
                "pyoxidizer-windows_exes" => match name.as_str() {
                    "i686-pc-windows-msvc/pyoxidizer.exe" => {
                        let data = create_single_file_zip("pyoxidizer.exe", &buf)?;

                        upload_artifacts.push((
                            format!("PyOxidizer-{}-exe-i686-pc-windows.zip", version),
                            data,
                        ));
                    }
                    "x86_64-pc-windows-msvc/pyoxidizer.exe" => {
                        let data = create_single_file_zip("pyoxidizer.exe", &buf)?;

                        upload_artifacts.push((
                            format!("PyOxidizer-{}-exe-x86_64-pc-windows.zip", version),
                            data,
                        ));
                    }
                    _ => {
                        return Err(anyhow!("unexpected file in {}: {}", artifact_name, name));
                    }
                },
                _ => {
                    println!("ignoring {} from {}", name, artifact_name);
                }
            }
        }
    }

    let mut digests = BTreeMap::new();

    // Ensure artifacts are part of release.
    for (name, data) in upload_artifacts {
        let mut digest = Sha256::new();
        digest.update(&data);

        let digest = hex::encode(digest.finalize());

        digests.insert(name.clone(), digest.clone());

        upload_release_artifact(&client, &release, &name, data.clone(), dry_run).await?;
        upload_release_artifact(
            &client,
            &release,
            &format!("{}.sha256", name),
            format!("{}\n", digest).into_bytes(),
            dry_run,
        )
        .await?;

        if name.ends_with(".whl") {
            if let (Some(username), Some(password)) = (pypi_username, pypi_password) {
                println!("uploading {} to PyPI", name);

                let td = tempfile::tempdir()?;

                let path = td.path().join(&name);
                std::fs::write(&path, &data)?;

                let registry = maturin::Registry {
                    url: pypi_registry.to_string(),
                    username: username.to_string(),
                    password: password.to_string(),
                };

                match maturin::upload(&registry, &path) {
                    Ok(_) => {
                        println!("uploaded {} to PyPI", name);
                    }
                    Err(maturin::UploadError::FileExistsError(_)) => {
                        println!("{} already exists in PyPI", name);
                    }
                    Err(err) => return Err(anyhow!("PyPI upload error: {:?}", err)),
                }
            }
        }
    }

    let shasums = digests
        .iter()
        .map(|(filename, digest)| format!("{}  {}\n", digest, filename))
        .collect::<Vec<_>>()
        .join("");

    upload_release_artifact(
        &client,
        &release,
        "SHA256SUMS",
        shasums.into_bytes(),
        dry_run,
    )
    .await?;

    Ok(())
}
