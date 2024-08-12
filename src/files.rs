use std::{
    collections::HashSet,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Dirs {
    target_name: String,
    follow: bool,
    ignore: HashSet<PathBuf>,
    frontier: Vec<PathBuf>,
}

impl Dirs {
    #[must_use]
    pub fn find(
        root: &Path,
        target_name: &str,
        follow: bool,
        ignore: &HashSet<PathBuf>,
    ) -> Self {
        let root = root.to_path_buf();
        Self {
            ignore: ignore.to_owned(),
            follow,
            target_name: target_name.to_string(),
            frontier: vec![root],
        }
    }
}

impl Iterator for Dirs {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.frontier.pop() {
            if self.ignore.contains(&path) {
                continue;
            }
            if !&path.try_exists().is_ok_and(|exists| exists) {
                continue;
            }
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_symlink() => {
                    if !self.follow {
                        continue;
                    }
                    match fs::read_link(&path) {
                        Ok(path1) => {
                            self.frontier.push(path1);
                        }
                        Err(error) => {
                            tracing::error!(
                                ?path,
                                ?error,
                                "Failed to read link."
                            );
                        }
                    }
                }
                Ok(meta) if meta.is_dir() => {
                    if path.file_name().is_some_and(|name| {
                        name.as_bytes() == self.target_name.as_bytes()
                    }) {
                        return Some(path);
                    }
                    match fs::read_dir(&path) {
                        Err(error) => {
                            tracing::error!(
                                ?path,
                                ?error,
                                "Failed to read directory",
                            );
                        }
                        Ok(entries) => {
                            for entry_result in entries {
                                match entry_result {
                                    Ok(entry) => {
                                        self.frontier.push(entry.path());
                                    }
                                    Err(error) => {
                                        tracing::error!(
                                            from = ?path, ?error,
                                            "Failed to read an entry",
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(
                        from = ?path, ?error,
                        "Failed to read metadata",
                    );
                }
            }
        }
        None
    }
}
