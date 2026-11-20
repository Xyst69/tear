//! Pin each parent directory while opening a child. An open Windows handle
//! without write/delete sharing prevents a directory being redirected or renamed.
use crate::readers::invalid;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::*;

pub(crate) struct Directory {
    pub path: PathBuf,
    _chain: Vec<File>,
}

fn pin(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid(format!(
            "Expected a regular directory: {}",
            path.display()
        )));
    }
    Ok(file)
}

impl Directory {
    pub fn open(path: &Path) -> io::Result<Self> {
        // The caller chooses the destination. Resolve it once, then hold its ancestors.
        fs::create_dir_all(path)?;
        let path = path.canonicalize()?;
        let mut chain = Vec::new();
        let mut parents: Vec<_> = path.ancestors().collect();
        parents.reverse();
        for ancestor in parents {
            chain.push(pin(ancestor)?);
        }
        Ok(Self {
            path,
            _chain: chain,
        })
    }

    pub fn descendants(&self, relative: &Path) -> io::Result<Vec<File>> {
        let mut path = self.path.clone();
        let mut chain = Vec::new();
        for component in relative.components() {
            let std::path::Component::Normal(part) = component else {
                return Err(invalid("Expected relative directory"));
            };
            path.push(part);
            match fs::create_dir(&path) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e),
            }
            chain.push(pin(&path)?);
        }
        Ok(chain)
    }

    pub fn unique_child(mut self, name: &Path) -> io::Result<Self> {
        for i in 1u32.. {
            let child = if i == 1 {
                self.path.join(name)
            } else {
                self.path.join(format!("{} ({i})", name.display()))
            };
            match fs::create_dir(&child) {
                Ok(()) => {
                    self._chain.push(pin(&child)?);
                    self.path = child;
                    return Ok(self);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(invalid("No unused output folder name"))
    }

    pub fn available_space(&self) -> io::Result<u64> {
        let path: Vec<u16> = self.path.as_os_str().encode_wide().chain([0]).collect();
        let mut space = 0;
        // SAFETY: the path is NUL-terminated and the only output points to a live u64.
        if unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut space,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(space)
    }
}

fn zone_path(path: &Path) -> OsString {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    stream
}

pub(crate) fn zone(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match File::open(zone_path(path)) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return Err(invalid("Archive has oversized Zone.Identifier"));
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub(crate) struct Output {
    pub file: File,
    keep: bool,
}
impl Output {
    pub fn new(path: &Path, zone: Option<&[u8]>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .access_mode(0x40000000 | DELETE)
            .share_mode(0)
            .open(path)?;
        let output = Self { file, keep: false };
        if let Some(bytes) = zone {
            File::create(zone_path(path))?.write_all(bytes)?;
        }
        Ok(output)
    }
    pub fn finish(mut self) -> io::Result<()> {
        self.file.flush()?;
        self.keep = true;
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if !self.keep {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: the file was opened with DELETE access. Target the handle
            // so cleanup can never unlink a different file at the same path.
            unsafe {
                SetFileInformationByHandle(
                    self.file.as_raw_handle(),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            };
        }
    }
}

pub(crate) fn file_time(ticks: u64) -> Option<std::time::SystemTime> {
    const EPOCH: u64 = 116444736000000000;
    let delta = ticks.abs_diff(EPOCH);
    let duration =
        std::time::Duration::new(delta / 10_000_000, ((delta % 10_000_000) * 100) as u32);
    if ticks >= EPOCH {
        std::time::UNIX_EPOCH.checked_add(duration)
    } else {
        std::time::UNIX_EPOCH.checked_sub(duration)
    }
}
pub(crate) fn dos_time(date: u16, time: u16) -> Option<std::time::SystemTime> {
    use windows_sys::Win32::{
        Foundation::FILETIME, System::WindowsProgramming::DosDateTimeToFileTime,
    };
    let mut local = FILETIME::default();
    let mut utc = FILETIME::default();
    // SAFETY: all input and output pointers refer to initialized FILETIME values.
    if unsafe { DosDateTimeToFileTime(date, time, &mut local) } == 0
        || unsafe { LocalFileTimeToFileTime(&local, &mut utc) } == 0
    {
        return None;
    }
    file_time((u64::from(utc.dwHighDateTime) << 32) | u64::from(utc.dwLowDateTime))
}
