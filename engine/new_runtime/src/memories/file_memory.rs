// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{
    locked_memory::LockedMemory,
    memories::buffer::Buffer,
    types::ContiguousBytes,
    utils::*,
    MemoryError::{self, *},
    DEBUG_MSG,
};
use core::fmt::{self, Debug, Formatter};
use dirs::{data_local_dir, home_dir};
use std::{
    fs::{self, File},
    io::{self, prelude::*},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

const FILENAME_SIZE: usize = 16;

/// Data is stored into files in clear or encrypted.
/// Basic security of this file includes files access control and
pub struct FileMemory {
    // Filename are random string of 16 characters
    fname: PathBuf,
    // Noise data to xor with data in file
    noise: Vec<u8>,
    // Size of the decrypted data
    size: usize,
}

impl FileMemory {
    pub fn alloc(payload: &[u8], size: usize) -> Result<Self, MemoryError> {
        if size == 0 {
            return Err(ZeroSizedNotAllowed);
        }

        // We actually don't want to have plain data in file
        // therefore we noise it
        let noise = random_vec(size);
        let data = xor(payload, &noise, size);

        // Write to file
        let fname = FileMemory::new_fname().or(Err(FileSystemError))?;
        let fm = FileMemory { fname, noise, size };
        fm.write_to_file(&data).or(Err(FileSystemError))?;
        Ok(fm)
    }

    fn new_fname() -> io::Result<PathBuf> {
        let fname = random_fname(FILENAME_SIZE);
        let mut dir = FileMemory::get_dir()?;
        dir.push(fname);
        Ok(dir)
    }

    fn clear_and_delete_file(&self) -> Result<(), std::io::Error> {
        self.set_write_only()?;
        let mut file = File::create(&self.fname)?;
        // Zeroes out the file
        file.write_all(&vec![0; self.size])?;
        // Remove file
        fs::remove_file(&self.fname)
    }

    // We create a directory in the home directory to store the data
    fn get_dir() -> io::Result<PathBuf> {
        // Select where the files will be stored
        let mut dir = if let Some(dir) = data_local_dir() {
            dir
        } else if let Some(dir) = home_dir() {
            dir
        } else {
            PathBuf::new()
        };
        dir.push(PathBuf::from(".locked_memories"));

        // Create the directory if it does not exists
        if !dir.is_dir() {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    // Set access control before and after reading the file
    fn read_file(&self) -> Result<Vec<u8>, std::io::Error> {
        self.set_read_only()?;
        let content = fs::read(&self.fname)?;
        self.lock_file()?;
        Ok(content)
    }

    // Set access control to minimum on the file
    fn lock_file(&self) -> Result<(), std::io::Error> {
        // Lock file permissions
        let mut perms = fs::metadata(&self.fname)?.permissions();
        if cfg!(unix) {
            // Prevent reading/writing
            perms.set_mode(0o000);
        } else {
            // Currently rust fs library can only be create
            // readonly file permissions
            perms.set_readonly(true);
        }
        fs::set_permissions(&self.fname, perms)
    }

    fn set_write_only(&self) -> Result<(), std::io::Error> {
        // Lock file permissions
        let mut perms = fs::metadata(&self.fname)?.permissions();
        if cfg!(unix) {
            // Set write only
            perms.set_mode(0o200);
        } else {
            // Currently rust fs library can only be create
            // readonly file permissions
            perms.set_readonly(true);
        }
        fs::set_permissions(&self.fname, perms)
    }

    fn set_read_only(&self) -> Result<(), std::io::Error> {
        // Lock file permissions
        let mut perms = fs::metadata(&self.fname)?.permissions();
        if cfg!(unix) {
            // Set write only
            perms.set_mode(0o400);
        } else {
            // Currently rust fs library can only be create
            // readonly file permissions
            perms.set_readonly(true);
        }
        fs::set_permissions(&self.fname, perms)
    }

    fn write_to_file(&self, payload: &[u8]) -> Result<(), std::io::Error> {
        match self.set_write_only() {
            Ok(()) => (),
            // File may not exist yet
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            err => {
                return err;
            }
        };
        let mut file = File::create(&self.fname)?;
        file.write_all(payload.as_bytes())?;
        self.lock_file()
    }
}

impl LockedMemory for FileMemory {
    /// Locks the memory and possibly reallocates
    fn update(self, payload: Buffer<u8>, size: usize) -> Result<Self, MemoryError> {
        // The current choice is to allocate a completely new file and
        // remove the previous one
        FileMemory::alloc(&payload.borrow(), size)
    }

    /// Unlocks the memory and returns an unlocked Buffer
    fn unlock(&self) -> Result<Buffer<u8>, MemoryError> {
        if self.size == 0 {
            return Err(ZeroSizedNotAllowed);
        }

        let data = self.read_file().or(Err(FileSystemError))?;
        let data = xor(&data, &self.noise, self.size);
        Ok(Buffer::alloc(&data, self.size))
    }
}

impl Zeroize for FileMemory {
    // Temporary measure, files get deleted multiple times in non contiguous memory,
    // needs to track usage to improve performance
    #[allow(unused_must_use)]
    fn zeroize(&mut self) {
        self.clear_and_delete_file();
        // May not be enough
        self.fname.clear();
        self.size.zeroize();
    }
}

impl ZeroizeOnDrop for FileMemory {}

impl Drop for FileMemory {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl Debug for FileMemory {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", DEBUG_MSG)
    }
}

/// To clone file memory we make a duplicate of the file containing the data
impl Clone for FileMemory {
    fn clone(&self) -> Self {
        let error_msg = "Issue while copying file";
        let fname = FileMemory::new_fname().expect(error_msg);
        self.set_read_only().expect(error_msg);
        fs::copy(&self.fname, fname.clone()).expect(error_msg);
        self.lock_file().expect(error_msg);
        let fm = FileMemory {
            fname,
            noise: self.noise.clone(),
            size: self.size,
        };
        fm.lock_file().expect(error_msg);
        fm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_zeroize() {
        let fm = FileMemory::alloc(&[1, 2, 3, 4, 5, 6][..], 6);
        assert!(fm.is_ok());
        let mut fm = fm.unwrap();
        let fname = fm.fname.clone();
        fm.zeroize();

        // Check that file has been removed
        assert!(!std::path::Path::new(&fname).exists());
        assert!(fm.fname.as_os_str().is_empty());
        assert!(fm.unlock().is_err());
        assert_eq!(fm.size, 0);
    }

    #[test]
    // Check that file content cannot be accessed directly
    fn file_security() {
        let data = [1, 2, 3, 4, 5, 6];
        let fm = FileMemory::alloc(&data, 6);
        assert!(fm.is_ok());
        let fm = fm.unwrap();

        // Try to read or write file
        let try_read = File::open(&fm.fname).expect_err("Test failed shall gives an Err");
        let try_write = File::create(&fm.fname).expect_err("Test failed shall gives an Err");
        assert_eq!(try_read.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(try_write.kind(), std::io::ErrorKind::PermissionDenied);

        // Check that content of the file has effectively been xored
        assert!(fm.set_read_only().is_ok());
        let content = fs::read(&fm.fname).expect("Fail to read file");
        assert_ne!(content, data);
        assert_eq!(xor(&content, &fm.noise, fm.size), data);
    }
}
