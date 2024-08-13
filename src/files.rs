use std::{
    collections::HashSet,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use futures::{stream, Stream};

pub fn find_dirs(
    root: &Path,
    target_name: &str,
    follow: bool,
    ignore: &HashSet<PathBuf>,
) -> impl Stream<Item = PathBuf> {
    stream::unfold(
        Dirs::init(root, target_name, follow, ignore),
        |mut dirs| async { dirs.next().await.map(|dir| (dir, dirs)) },
    )
}

#[derive(Debug)]
struct Dirs {
    target_name: String,
    follow: bool,
    ignore: HashSet<PathBuf>,
    frontier: Vec<PathBuf>,
}

impl Dirs {
    fn init(
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

    async fn next(&mut self) -> Option<PathBuf> {
        // XXX Walking the fs tree with tokio is about 5x slower!
        // use tokio::fs;
        use std::fs;

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
                            // --- std ---
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

                            // --- tokio ---
                            // loop {
                            //     match entries.next_entry().await {
                            //         Ok(Some(entry)) => {
                            //             self.frontier.push(entry.path());
                            //         }
                            //         Ok(None) => break,
                            //         Err(error) => {
                            //             tracing::error!(
                            //                 from = ?path, ?error,
                            //                 "Failed to read an entry",
                            //             );
                            //             break;
                            //         }
                            //     }
                            // }
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
