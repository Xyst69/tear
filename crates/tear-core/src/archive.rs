use crate::format::Format;
use crate::path::relative_name;
pub use crate::readers::Entry;
use crate::readers::{detect, invalid, password_error, walk};
use crate::windows_io::{self, Directory, Output};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Where extracted entries go, relative to the chosen destination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExtractMode {
    /// A single top-level folder is kept as-is; loose entries go into a new
    /// folder named after the archive. Existing folders are never merged.
    #[default]
    Smart,
    /// Always create a new folder named after the archive.
    Folder,
    /// Write directly into the destination. Existing files are never replaced.
    Here,
}

#[derive(Default)]
pub struct ArchiveOptions {
    pub password: Option<String>,
    pub allow_large: bool,
}

pub struct Archive {
    file: File,
    path: PathBuf,
    kind: Format,
    options: ArchiveOptions,
    entries: Vec<Entry>,
    declared: u64,
    limit: u64,
}

#[derive(Default, Debug)]
pub struct Report {
    pub damaged: Vec<String>,
    pub files: u64,
    pub bytes: u64,
    pub destination: Option<PathBuf>,
}

impl Archive {
    pub fn open(path: &Path, options: ArchiveOptions) -> io::Result<Self> {
        let path = path.canonicalize()?;
        let mut file = OpenOptions::new().read(true).share_mode(1).open(&path)?;
        let kind = detect(&mut file)?;
        let packed = file.metadata()?.len();
        let limit = if options.allow_large {
            u64::MAX
        } else {
            packed.saturating_mul(1000).max(10 << 30)
        };
        let mut entries = Vec::new();
        let mut names = HashSet::new();
        let mut declared = 0u64;
        walk(
            &mut file,
            &path,
            kind,
            options.password.as_deref(),
            limit,
            false,
            &mut |e, _| {
                if entries.len() >= 1_000_000 {
                    return Err(invalid("Archive index exceeds one million entries"));
                }
                if !names.insert(e.path.to_string_lossy().to_uppercase()) {
                    return Err(invalid(format!(
                        "Conflicting archive names at {}",
                        e.path.display()
                    )));
                }
                declared = declared
                    .checked_add(e.size.unwrap_or(0))
                    .ok_or_else(|| invalid("Archive output size overflow"))?;
                if declared > limit {
                    return Err(invalid(
                        "Archive exceeds 10 GiB and a 1000:1 ratio. Inspect it before using --allow-large.",
                    ));
                }
                entries.push(e);
                Ok(())
            },
        )?;
        let files: HashSet<_> = entries
            .iter()
            .filter(|e| !e.directory)
            .map(|e| e.path.to_string_lossy().to_uppercase())
            .collect();
        for e in &entries {
            if e.path
                .ancestors()
                .skip(1)
                .any(|p| files.contains(&p.to_string_lossy().to_uppercase()))
            {
                return Err(invalid(
                    "Archive contains a file used as a parent directory",
                ));
            }
        }
        Ok(Self {
            file,
            path,
            kind,
            options,
            entries,
            declared,
            limit,
        })
    }

    pub fn format(&self) -> Format {
        self.kind
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn common_dir(&self) -> Option<PathBuf> {
        let top = PathBuf::from(self.entries.first()?.path.iter().next()?);
        self.entries
            .iter()
            .all(|e| e.path.starts_with(&top) && (e.directory || e.path != top))
            .then_some(top)
    }

    pub fn test(self) -> io::Result<Report> {
        self.run(None, None)
    }
    pub fn extract(self, destination: &Path, mode: ExtractMode) -> io::Result<Report> {
        // Fail before creating anything when a password is obviously missing.
        if self.options.password.is_none() && self.entries.iter().any(|e| e.encrypted) {
            return Err(password_error("Password required"));
        }
        let base = Directory::open(destination)?;
        if self.declared > base.available_space()? {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                format!("Extraction requires {} bytes", self.declared),
            ));
        }
        let common = match mode {
            ExtractMode::Here => return self.run(Some(base), None),
            ExtractMode::Folder => None,
            ExtractMode::Smart => self.common_dir(),
        };
        let stem = self
            .path
            .file_stem()
            .and_then(|p| p.to_str())
            .unwrap_or("archive");
        let stem = if stem.to_ascii_lowercase().ends_with(".tar") {
            &stem[..stem.len() - 4]
        } else {
            stem
        };
        let fallback = relative_name(stem).unwrap_or_else(|_| PathBuf::from("archive"));
        let base = base.unique_child(common.as_deref().unwrap_or(&fallback))?;
        let created = base.path.clone();
        let result = self.run(Some(base), common);
        if result.is_err() && holds_no_files(&created) {
            // This run created the folder. When nothing but empty folders was
            // written (e.g. a wrong password), remove it so a retry reuses the name.
            let _ = std::fs::remove_dir_all(&created);
        }
        result
    }
    fn run(mut self, root: Option<Directory>, strip: Option<PathBuf>) -> io::Result<Report> {
        let zone = if root.is_some() {
            windows_io::zone(&self.path)?
        } else {
            None
        };
        let mut expected: HashMap<_, _> = self
            .entries
            .iter()
            .map(|e| (e.path.clone(), (e.directory, e.size)))
            .collect();
        let mut report = Report {
            destination: root.as_ref().map(|r| r.path.clone()),
            ..Report::default()
        };
        walk(
            &mut self.file,
            &self.path,
            self.kind,
            self.options.password.as_deref(),
            self.limit,
            true,
            &mut |e, source| {
                if expected.remove(&e.path) != Some((e.directory, e.size)) {
                    return Err(invalid("Archive members changed after indexing"));
                }
                let name = match strip.as_ref() {
                    Some(p) => e
                        .path
                        .strip_prefix(p)
                        .map_err(|_| invalid("Archive root changed"))?,
                    None => &e.path,
                };
                let mut pins = Vec::new();
                let mut output = None;
                if let Some(root) = &root {
                    let dir = if e.directory {
                        name
                    } else {
                        name.parent()
                            .ok_or_else(|| invalid("Missing parent directory"))?
                    };
                    pins = root.descendants(dir)?;
                    if !e.directory {
                        output = Some(
                            Output::new(&root.path.join(name), zone.as_deref()).map_err(|err| {
                                io::Error::new(err.kind(), format!("{}: {err}", name.display()))
                            })?,
                        );
                    }
                }
                if !e.directory {
                    let source = source.ok_or_else(|| invalid("Missing member data"))?;
                    let remaining = self.limit - report.bytes;
                    let cap = e.size.unwrap_or(remaining).min(remaining);
                    let mut sink = io::sink();
                    let to: &mut dyn Write = match &mut output {
                        Some(out) => &mut out.file,
                        None => &mut sink,
                    };
                    let count = match copy_member(source, to, cap, e.size) {
                        Ok(n) => n,
                        Err(CopyError::Input(err)) if self.kind == Format::Zip => {
                            report.damaged.push(format!("{}: {err}", name.display()));
                            return Ok(());
                        }
                        Err(CopyError::Input(err) | CopyError::Output(err)) => return Err(err),
                    };
                    if let Some(out) = output {
                        if let Some(time) = e.modified {
                            out.file.set_modified(time)?;
                        }
                        out.finish()?;
                    }
                    report.files += 1;
                    report.bytes += count;
                }
                drop(pins);
                Ok(())
            },
        )?;
        if !expected.is_empty() {
            return Err(invalid(
                "Archive ended before all indexed entries were decoded",
            ));
        }
        Ok(report)
    }
}

/// True when `dir` contains only (nested) empty folders. Anything else, including
/// links or unreadable entries, counts as content and keeps the folder.
fn holds_no_files(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.into_iter().all(|entry| {
        entry.is_ok_and(|e| e.file_type().is_ok_and(|t| t.is_dir()) && holds_no_files(&e.path()))
    })
}

enum CopyError {
    Input(io::Error),
    Output(io::Error),
}

fn copy_member(
    source: &mut dyn Read,
    dest: &mut dyn Write,
    cap: u64,
    expected: Option<u64>,
) -> Result<u64, CopyError> {
    let mut buffer = [0u8; 64 * 1024];
    let mut count = 0u64;
    loop {
        let room = (cap - count).saturating_add(1).min(buffer.len() as u64) as usize;
        let n = source.read(&mut buffer[..room]).map_err(CopyError::Input)?;
        if n == 0 {
            break;
        }
        if n as u64 > cap - count {
            return Err(CopyError::Input(invalid(
                "Member exceeds expected output size",
            )));
        }
        dest.write_all(&buffer[..n]).map_err(CopyError::Output)?;
        count += n as u64;
    }
    if expected.is_some_and(|size| size != count) {
        return Err(CopyError::Input(invalid("Truncated member")));
    }
    Ok(count)
}
