use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::Context;
use dashmap::DashSet;
use futures::{stream, StreamExt};

use crate::{git, os};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    /// Follow symbollic links.
    #[clap(short, long, default_value_t = false)]
    follow: bool,

    // TODO These should actualy be regexp patterns to filter candidate paths.
    /// Ignore this path when searching for repos.
    #[clap(short, long)]
    ignore_paths: Vec<PathBuf>,

    /// Local paths to explore for potential git repos.
    search_paths: Vec<PathBuf>,
}

impl Cmd {
    pub async fn run(&self) -> anyhow::Result<()> {
        let ignore_paths: HashSet<PathBuf> =
            self.ignore_paths.iter().cloned().collect();
        let mut search_paths = Vec::new();
        for path in &self.search_paths {
            let path = path
                .canonicalize()
                .context(format!("Invalid local path={path:?}"))?;
            search_paths.push(path);
        }
        let urls: Arc<DashSet<String>> = Arc::new(DashSet::new());
        let host = os::hostname().await?;
        let git_dirs = search_paths.iter().flat_map(|path| {
            crate::fs::find_dirs(path, ".git", self.follow, &ignore_paths)
        });
        // XXX This has been the fastest combination: sync producer + async consumer.
        stream::iter(git_dirs)
            .for_each_concurrent(None, |dir| async {
                if git::is_repo(&dir).await {
                    if let Ok(view) =
                        git::view(&host, &git::Link::Fs { dir }).await
                    {
                        println!("{:#?}", &view);
                        for url in view
                            .repo
                            .iter()
                            .flat_map(|repo| repo.remotes.values())
                        {
                            urls.insert(url.to_string());
                        }
                    }
                }
            })
            .await;
        dbg!(&urls);
        stream::iter(urls.iter())
            .map(|url| git::Link::Net {
                url: url.to_string(),
            })
            .filter_map(|link| {
                let host = host.clone();
                async move { git::view(&host, &link).await.ok() }
            })
            .for_each_concurrent(None, |view| async move {
                println!("{:#?}", &view);
            })
            .await;
        Ok(())
    }
}
