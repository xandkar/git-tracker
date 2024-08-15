use std::{collections::HashSet, path::PathBuf};

use anyhow::Context;
use futures::{stream, StreamExt};

use crate::git;

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    /// Follow symbollic links.
    #[clap(short, long, default_value_t = false)]
    follow: bool,

    // TODO These should actualy be regexp patterns to filter candidate paths.
    /// Ignore this path when searching for repos.
    #[clap(short, long)]
    ignore: Vec<PathBuf>,

    /// Local paths to explore for potential git repos.
    paths: Vec<PathBuf>,
}

impl Cmd {
    pub async fn run(&self) -> anyhow::Result<()> {
        let ignore: HashSet<PathBuf> = self.ignore.iter().cloned().collect();
        let mut paths = Vec::new();
        for path in &self.paths {
            let path = path
                .canonicalize()
                .context(format!("Invalid local path={path:?}"))?;
            paths.push(path);
        }

        stream::iter(paths)
            .flat_map(|path| {
                crate::fs::find_dirs(&path, ".git", self.follow, &ignore)
            })
            .filter(|path| git::local_is_repo(path.clone()))
            .filter_map(|path| async move {
                let res = git::Local::read(&path).await;
                if let Err(error) = &res {
                    tracing::error!(?path, ?error, "Failed to read repo.");
                }
                res.ok()
            })
            .for_each_concurrent(None, |repo| async move {
                println!("{repo:#?}");
            })
            .await;
        Ok(())
    }
}
