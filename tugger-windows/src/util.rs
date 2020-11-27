// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(windows)]
use std::{os::windows::ffi::OsStringExt, path::PathBuf};

#[cfg(windows)]
pub fn get_known_folder_path(
    id: winapi::um::shtypes::REFKNOWNFOLDERID,
) -> std::io::Result<PathBuf> {
    struct KnownFolderPath(winapi::shared::ntdef::PWSTR);

    impl Drop for KnownFolderPath {
        fn drop(&mut self) {
            unsafe {
                winapi::um::combaseapi::CoTaskMemFree(self.0 as *mut std::ffi::c_void);
            }
        }
    }

    unsafe {
        let mut path = KnownFolderPath(std::ptr::null_mut());
        let hres =
            winapi::um::shlobj::SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path.0);
        if hres == winapi::shared::winerror::S_OK {
            let mut wide_string = path.0;
            let mut len = 0;
            while wide_string.read() != 0 {
                wide_string = wide_string.offset(1);
                len += 1;
            }
            let ws_slice = std::slice::from_raw_parts(path.0, len);
            let os_string = std::ffi::OsString::from_wide(ws_slice);
            Ok(std::path::Path::new(&os_string).to_owned())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}
