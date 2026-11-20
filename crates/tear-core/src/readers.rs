use crate::format::Format;
use crate::path::relative_name;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::{Path, PathBuf};

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub size: Option<u64>,
    pub directory: bool,
    pub encrypted: bool,
    pub modified: Option<std::time::SystemTime>,
}

impl Entry {
    pub(crate) fn new(name: &str, size: Option<u64>, directory: bool) -> io::Result<Self> {
        Ok(Self {
            path: relative_name(name)?,
            size,
            directory,
            encrypted: false,
            modified: None,
        })
    }
}

pub(crate) type Visit<'a> = dyn FnMut(Entry, Option<&mut dyn Read>) -> io::Result<()> + 'a;

pub(crate) fn detect(file: &mut File) -> io::Result<Format> {
    file.rewind()?;
    let mut prefix = Vec::with_capacity(1024);
    file.take(1024).read_to_end(&mut prefix)?;
    file.rewind()?;
    Format::from_prefix(&prefix).ok_or_else(|| invalid("Unrecognized archive header"))
}

pub(crate) fn walk(
    file: &mut File,
    path: &Path,
    format: Format,
    password: Option<&str>,
    limit: u64,
    decode: bool,
    visit: &mut Visit<'_>,
) -> io::Result<()> {
    file.rewind()?;
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(file)?;
            for i in 0..zip.len() {
                let info = zip.by_index_data(i)?;
                let mut entry = Entry::new(&info.name()?, Some(info.size()), info.is_dir())?;
                entry.modified = info
                    .last_modified()
                    .and_then(|dt| crate::windows_io::dos_time(dt.datepart(), dt.timepart()));
                entry.encrypted = info.encrypted();
                if decode && !entry.directory {
                    if entry.encrypted && password.is_none() {
                        return Err(password_error("Password required"));
                    }
                    let mut member = match password {
                        Some(p) => zip.by_index_decrypt(i, p.as_bytes()),
                        None => zip.by_index(i),
                    }
                    .map_err(|e| match e {
                        zip::result::ZipError::InvalidPassword => password_error(e),
                        other => io::Error::from(other),
                    })?;
                    visit(entry, Some(&mut member))?;
                } else {
                    visit(entry, None)?;
                }
            }
            Ok(())
        }
        Format::SevenZip => {
            let mut seven = sevenz_rust2::ArchiveReader::new_with_memory_limit(
                file,
                password.unwrap_or_default().into(),
                256 * 1024,
            )
            .map_err(seven_error)?;
            let convert = |e: &sevenz_rust2::ArchiveEntry| {
                if e.is_anti_item {
                    return Err(invalid("7z deletion records are unsupported"));
                }
                let mut entry = Entry::new(&e.name, Some(e.size), e.is_directory)?;
                if e.has_last_modified_date {
                    entry.modified = crate::windows_io::file_time(e.last_modified_date.into());
                }
                Ok(entry)
            };
            if decode {
                seven
                    .for_each_entries(|e, data| {
                        visit(convert(e)?, Some(data))?;
                        Ok(true)
                    })
                    .map_err(seven_error)
            } else {
                for (i, entry) in seven.archive().files.iter().enumerate() {
                    let mut meta = convert(entry)?;
                    meta.encrypted = seven.archive().is_file_encrypted(i);
                    visit(meta, None)?;
                }
                Ok(())
            }
        }
        Format::Rar4 | Format::Rar5 => rar(path, password, decode, visit),
        Format::Tar => tar_entries(file, decode, visit),
        outer => {
            let stream: Box<dyn Read + '_> = match outer {
                Format::Gzip => Box::new(flate2::read::MultiGzDecoder::new(file)),
                Format::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(file)),
                Format::Xz => {
                    let stream = liblzma::stream::Stream::new_stream_decoder(
                        256 << 20,
                        liblzma::stream::CONCATENATED,
                    )
                    .map_err(io::Error::other)?;
                    Box::new(liblzma::read::XzDecoder::new_stream(file, stream))
                }
                Format::Zstd => {
                    let mut decoder = zstd::stream::read::Decoder::new(file)?;
                    decoder.window_log_max(28)?;
                    Box::new(decoder)
                }
                _ => return Err(invalid("Unsupported compression stream")),
            };
            let mut decoded = BufReader::new(Bounded {
                inner: stream,
                remaining: limit,
            });
            // Read enough bytes even when the decoder returns short chunks.
            let mut prefix = Vec::with_capacity(1024);
            (&mut decoded).take(1024).read_to_end(&mut prefix)?;
            let named_tar = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                let n = n.to_ascii_lowercase();
                [
                    ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tar.zst", ".tzst",
                ]
                .iter()
                .any(|ext| n.ends_with(ext))
            });
            let is_tar = Format::from_prefix(&prefix) == Some(Format::Tar)
                && (named_tar || prefix.iter().any(|&b| b != 0));
            let mut restored = io::Cursor::new(prefix).chain(decoded);
            if is_tar {
                tar_entries(&mut restored, decode, visit)
            } else {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| invalid("Invalid stream file name"))?;
                let entry = Entry::new(stem, None, false)?;
                if decode {
                    visit(entry, Some(&mut restored))
                } else {
                    visit(entry, None)
                }
            }
        }
    }
}

struct Bounded<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return if self.inner.read(&mut [0])? == 0 {
                Ok(0)
            } else {
                Err(invalid(
                    "Expansion limit exceeded. Review the archive before allowing large output.",
                ))
            };
        }
        let len = self.remaining.min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..len])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

fn tar_entries(source: &mut dyn Read, decode: bool, visit: &mut Visit<'_>) -> io::Result<()> {
    let mut archive = tar::Archive::new(source);
    for member in archive.entries()? {
        let mut member = member?;
        let name = member.path_bytes();
        let name = std::str::from_utf8(&name).map_err(invalid_encoding)?;
        let kind = member.header().entry_type();
        if kind.is_dir()
            && name.split('/').all(|p| p == "." || p.is_empty())
            && !name.starts_with('/')
        {
            continue;
        }
        let mut entry = Entry::new(name, Some(member.size()), kind.is_dir())?;
        entry.modified = member
            .header()
            .mtime()
            .ok()
            .and_then(|t| std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(t)));
        if kind.is_symlink() || kind.is_hard_link() {
            // Materialize link descriptions as ordinary files; never delegate links to the OS.
            let target = member
                .link_name_bytes()
                .ok_or_else(|| invalid("Archive link is missing its target"))?;
            entry.size = Some(target.len() as u64);
            if decode {
                visit(entry, Some(&mut target.as_ref()))?;
            } else {
                visit(entry, None)?;
            }
        } else if kind.is_dir() || kind.is_file() || kind.is_gnu_sparse() {
            if decode {
                visit(entry, Some(&mut member))?;
            } else {
                visit(entry, None)?;
            }
        } else {
            return Err(invalid(format!(
                "Unsupported special tar entry: {}",
                entry.path.display()
            )));
        }
    }
    // Complete the outer decoder so trailer checks also run for compressed tar.
    if decode {
        io::copy(&mut archive.into_inner(), &mut io::sink())?;
    }
    Ok(())
}

fn invalid_encoding(e: std::str::Utf8Error) -> io::Error {
    invalid(e.to_string())
}

fn seven_error(e: sevenz_rust2::Error) -> io::Error {
    match e {
        sevenz_rust2::Error::Io(inner, _) => inner,
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_) => {
            password_error(e)
        }
        e => invalid(e.to_string()),
    }
}

#[cfg(feature = "rar")]
fn rar(path: &Path, password: Option<&str>, decode: bool, visit: &mut Visit<'_>) -> io::Result<()> {
    if decode {
        // Streams members of any size; see rar_stream.rs.
        return crate::rar_stream::decode(path, password, visit);
    }
    let source = match password {
        Some(p) => unrar::Archive::with_password(path, p),
        None => unrar::Archive::new(path),
    };
    let convert = |e: unrar::error::UnrarError| {
        use unrar::error::Code;
        if matches!(e.code, Code::BadPassword | Code::MissingPassword) {
            password_error(e)
        } else if matches!(e.code, Code::BadData) && password.is_some() {
            // RAR4 has no password check value; a wrong password surfaces as a CRC error.
            password_error("Wrong password, or the archive is damaged")
        } else {
            invalid(e.to_string())
        }
    };
    let mut archive = source.open_for_processing().map_err(convert)?;
    while let Some(header) = archive.read_header().map_err(convert)? {
        let info = header.entry();
        let name = info
            .filename
            .to_str()
            .ok_or_else(|| invalid("Invalid RAR member name"))?;
        let mut entry = Entry::new(name, Some(info.unpacked_size), info.is_directory())?;
        entry.encrypted = info.is_encrypted();
        entry.modified =
            crate::windows_io::dos_time((info.file_time >> 16) as u16, info.file_time as u16);
        visit(entry, None)?;
        archive = header.skip().map_err(convert)?;
    }
    Ok(())
}

#[cfg(not(feature = "rar"))]
fn rar(_: &Path, _: Option<&str>, _: bool, _: &mut Visit<'_>) -> io::Result<()> {
    Err(invalid("RAR support was disabled at build time"))
}

#[derive(Debug)]
struct PasswordError(String);
impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PasswordError {}
pub(crate) fn password_error(e: impl ToString) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        PasswordError(e.to_string()),
    )
}
pub fn is_password_error(e: &io::Error) -> bool {
    e.get_ref().is_some_and(|inner| inner.is::<PasswordError>())
}
