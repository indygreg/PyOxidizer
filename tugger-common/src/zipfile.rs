// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    std::{
        io::{Read, Seek, Write},
        path::Path,
    },
};

pub fn extract_zip<R: Read + Seek, P: AsRef<Path>>(reader: R, path: P) -> Result<()> {
    let mut za = zip::ZipArchive::new(reader)?;

    for i in 0..za.len() {
        let mut file = za.by_index(i)?;

        let dest_path = path.as_ref().join(file.name());
        let parent = dest_path.parent().unwrap();

        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let mut b: Vec<u8> = Vec::new();
        file.read_to_end(&mut b)?;
        let mut fh = std::fs::File::create(dest_path)?;
        fh.write_all(&b)?;
    }

    Ok(())
}
