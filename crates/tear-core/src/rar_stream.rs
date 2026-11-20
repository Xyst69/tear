//! Streams RAR members through UnRAR's data callback, so members of any size are
//! written chunk by chunk through tear's normal (safe) output path instead of
//! being buffered in memory or written by UnRAR itself.
//!
//! UnRAR pushes data from inside `RARProcessFileW`, while tear's visitor pulls
//! through `Read`. A worker thread owns the UnRAR handle and forwards headers
//! and data over a bounded channel, which also provides backpressure.
use crate::readers::{Entry, Visit, invalid, password_error};
use std::ffi::c_int;
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use unrar_sys as ffi;

struct Header {
    name: Vec<u16>,
    size: u64,
    directory: bool,
    encrypted: bool,
    dos_time: u32,
}

enum Message {
    Header(Header),
    Data(Vec<u8>),
    /// The current member finished with this UnRAR result code.
    MemberDone(c_int),
    /// The archive ended normally (0) or failed with an UnRAR result code.
    ArchiveDone(c_int),
}

struct Shared {
    tx: SyncSender<Message>,
    password: Option<Vec<u16>>,
}

extern "C" fn callback(
    msg: ffi::UINT,
    user: ffi::LPARAM,
    p1: ffi::LPARAM,
    p2: ffi::LPARAM,
) -> c_int {
    if user == 0 {
        return -1;
    }
    // SAFETY: `user` points to the worker's `Shared`, which outlives the handle.
    let shared = unsafe { &*(user as *const Shared) };
    match msg {
        ffi::UCM_PROCESSDATA => {
            // SAFETY: UnRAR passes a buffer of p2 bytes valid for this call.
            let chunk = unsafe { std::slice::from_raw_parts(p1 as *const u8, p2 as usize) };
            // A send error means the reader stopped; -1 makes UnRAR abort.
            if shared.tx.send(Message::Data(chunk.to_vec())).is_ok() {
                0
            } else {
                -1
            }
        }
        ffi::UCM_NEEDPASSWORDW => match &shared.password {
            Some(password) if p2 > 0 => {
                let room = p2 as usize - 1;
                let len = password.len().min(room);
                // SAFETY: UnRAR provides a writable buffer of p2 wide characters.
                unsafe {
                    let out = std::slice::from_raw_parts_mut(p1 as *mut u16, len + 1);
                    out[..len].copy_from_slice(&password[..len]);
                    out[len] = 0;
                }
                1
            }
            _ => -1,
        },
        // Continue when the next volume exists; stop if UnRAR would have to ask.
        ffi::UCM_CHANGEVOLUMEW if p2 == ffi::RAR_VOL_ASK => -1,
        _ => 0,
    }
}

fn worker(path: Vec<u16>, password: Option<Vec<u16>>, tx: SyncSender<Message>) {
    let shared = Shared {
        tx: tx.clone(),
        password,
    };
    let user = &shared as *const Shared as ffi::LPARAM;
    // SAFETY: plain FFI calls; every pointer handed to UnRAR (path, structs,
    // `shared`) stays alive until RARCloseArchive below.
    unsafe {
        let mut open: ffi::OpenArchiveDataEx = std::mem::zeroed();
        open.archive_name_w = path.as_ptr();
        open.open_mode = ffi::RAR_OM_EXTRACT;
        open.callback = Some(callback);
        open.user_data = user;
        // The bindings declare *const, but UnRAR writes results into these structs,
        // so pass pointers derived from mutable borrows.
        let handle = ffi::RAROpenArchiveEx(&raw mut open);
        if handle.is_null() || open.open_result != 0 {
            let code = if open.open_result != 0 {
                open.open_result as c_int
            } else {
                ffi::ERAR_EOPEN
            };
            if !handle.is_null() {
                ffi::RARCloseArchive(handle);
            }
            let _ = tx.send(Message::ArchiveDone(code));
            return;
        }
        ffi::RARSetCallback(handle, Some(callback), user);
        let mut header: Box<ffi::HeaderDataEx> = Box::new(std::mem::zeroed());
        let end = loop {
            let code = ffi::RARReadHeaderEx(handle, &raw mut *header);
            if code == ffi::ERAR_END_ARCHIVE {
                break 0;
            }
            if code != ffi::ERAR_SUCCESS {
                break code;
            }
            let name_len = header
                .filename_w
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(header.filename_w.len());
            let directory = header.flags & ffi::RHDF_DIRECTORY != 0;
            let info = Header {
                name: header.filename_w[..name_len].to_vec(),
                size: u64::from(header.unp_size) | (u64::from(header.unp_size_high) << 32),
                directory,
                encrypted: header.flags & ffi::RHDF_ENCRYPTED != 0,
                dos_time: header.file_time,
            };
            if tx.send(Message::Header(info)).is_err() {
                break -1;
            }
            let operation = if directory {
                ffi::RAR_SKIP
            } else {
                ffi::RAR_TEST
            };
            let code = ffi::RARProcessFileW(handle, operation, std::ptr::null(), std::ptr::null());
            if tx.send(Message::MemberDone(code)).is_err() || code != ffi::ERAR_SUCCESS {
                break -1;
            }
        };
        ffi::RARCloseArchive(handle);
        if end != -1 {
            let _ = tx.send(Message::ArchiveDone(end));
        }
    }
}

fn rar_error(code: c_int, password: bool) -> io::Error {
    match code {
        ffi::ERAR_MISSING_PASSWORD => password_error("Password required"),
        ffi::ERAR_BAD_PASSWORD => password_error("Wrong password"),
        // RAR4 has no password check value; a wrong password surfaces as a CRC error.
        ffi::ERAR_BAD_DATA if password => {
            password_error("Wrong password, or the archive is damaged")
        }
        ffi::ERAR_BAD_DATA => invalid("The archive is damaged (checksum mismatch)"),
        ffi::ERAR_BAD_ARCHIVE | ffi::ERAR_UNKNOWN_FORMAT => invalid("Not a valid RAR archive"),
        ffi::ERAR_EOPEN => io::Error::new(
            io::ErrorKind::NotFound,
            "Couldn't open the archive or one of its volumes",
        ),
        ffi::ERAR_EREAD => invalid("Couldn't read the archive; it may be truncated"),
        ffi::ERAR_NO_MEMORY => io::Error::new(io::ErrorKind::OutOfMemory, "Not enough memory"),
        other => invalid(format!("UnRAR error {other}")),
    }
}

/// Reads one member's data from the worker until its `MemberDone` arrives.
struct MemberReader<'a> {
    rx: &'a Receiver<Message>,
    chunk: Vec<u8>,
    offset: usize,
    finished: bool,
    password: bool,
}

impl Read for MemberReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.offset == self.chunk.len() {
            if self.finished {
                return Ok(0);
            }
            match self.rx.recv() {
                Ok(Message::Data(chunk)) => {
                    self.chunk = chunk;
                    self.offset = 0;
                }
                Ok(Message::MemberDone(code)) => {
                    self.finished = true;
                    if code != ffi::ERAR_SUCCESS {
                        return Err(rar_error(code, self.password));
                    }
                }
                Ok(Message::ArchiveDone(code)) => {
                    self.finished = true;
                    return Err(rar_error(code, self.password));
                }
                Ok(Message::Header(_)) | Err(_) => {
                    self.finished = true;
                    return Err(invalid("RAR reader stopped unexpectedly"));
                }
            }
        }
        let n = buf.len().min(self.chunk.len() - self.offset);
        buf[..n].copy_from_slice(&self.chunk[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}

/// Decodes every member of a RAR archive and passes it to `visit`.
pub(crate) fn decode(path: &Path, password: Option<&str>, visit: &mut Visit<'_>) -> io::Result<()> {
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let has_password = password.is_some();
    let wide_password = password.map(|p| p.encode_utf16().collect::<Vec<u16>>());
    let (tx, rx) = sync_channel(8);
    let thread = std::thread::Builder::new()
        .name("tear-unrar".into())
        .spawn(move || worker(wide_path, wide_password, tx))?;

    let result = (|| -> io::Result<()> {
        loop {
            match rx.recv() {
                Ok(Message::Header(h)) => {
                    let name = String::from_utf16(&h.name)
                        .map_err(|_| invalid("Invalid RAR member name"))?;
                    let mut entry = Entry::new(&name, Some(h.size), h.directory)?;
                    entry.encrypted = h.encrypted;
                    entry.modified =
                        crate::windows_io::dos_time((h.dos_time >> 16) as u16, h.dos_time as u16);
                    let mut reader = MemberReader {
                        rx: &rx,
                        chunk: Vec::new(),
                        offset: 0,
                        finished: false,
                        password: has_password,
                    };
                    if entry.directory {
                        visit(entry, None)?;
                    } else {
                        visit(entry, Some(&mut reader))?;
                    }
                    // Consume whatever the visitor didn't read, up to MemberDone.
                    io::copy(&mut reader, &mut io::sink())?;
                }
                Ok(Message::ArchiveDone(0)) => return Ok(()),
                Ok(Message::ArchiveDone(code)) => return Err(rar_error(code, has_password)),
                Ok(_) => return Err(invalid("RAR reader sent data out of order")),
                Err(_) => return Err(invalid("RAR reader stopped unexpectedly")),
            }
        }
    })();
    // Dropping the receiver makes any pending send fail, so the worker stops.
    drop(rx);
    if thread.join().is_err() {
        return Err(invalid("RAR reader crashed"));
    }
    result
}
