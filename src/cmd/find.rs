use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::Context;
use dashmap::DashSet;
use futures::{stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{info_span, Instrument};

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
        let locals: Arc<DashSet<git::Link>> = Arc::new(DashSet::new());
        let remotes_ok: Arc<DashSet<git::Link>> = Arc::new(DashSet::new());
        let remotes_err: Arc<DashSet<git::Link>> = Arc::new(DashSet::new());

        let host = os::hostname().await?;

        let (urls_tx, urls_rx) = mpsc::unbounded_channel();
        let (views_tx, _views_rx) = mpsc::unbounded_channel();

        let locals_worker = tokio::spawn(
            {
                let host = host.clone();
                let follow = self.follow;
                let locals = locals.clone();
                let views_tx = views_tx.clone();
                async move {
                    let git_dirs = search_paths.iter().flat_map(|path| {
                        crate::fs::find_dirs(
                            path,
                            ".git",
                            follow,
                            &ignore_paths,
                        )
                    });
                    let unique: DashSet<String> = DashSet::new();
                    // XXX This has been the fastest combination: sync producer + async consumer.
                    stream::iter(git_dirs)
                        .for_each_concurrent(None, |dir| async {
                            if git::is_repo(&dir).await {
                                let link = git::Link::Fs { dir };
                                let view = git::view(&host, &link).await;
                                locals.insert(link);
                                for url in view.repo.iter().flat_map(|repo| {
                                    repo.remotes.values().cloned()
                                }) {
                                    if unique.insert(url.clone()) {
                                        urls_tx.send(url).unwrap_or_else(
                                            |_| {
                                                unreachable!(
                                                    "urls_rx dropped while \
                                                    urls_tx is still in use"
                                                )
                                            },
                                        );
                                    }
                                }
                                views_tx.send(view).unwrap_or_else(|_| {
                                    unreachable!(
                                        "view_rx dropped while view_tx \
                                        is still in use"
                                    )
                                });
                            }
                        })
                        .await;
                }
            }
            .instrument(info_span!("locals_worker")),
        );

        let remotes_worker = tokio::spawn(
            {
                let views_tx = views_tx.clone();
                let remotes_ok = remotes_ok.clone();
                let remotes_err = remotes_err.clone();
                async move {
                    UnboundedReceiverStream::new(urls_rx)
                        .for_each_concurrent(None, {
                            move |url: String| {
                                let host = host.clone();
                                let remotes_ok = remotes_ok.clone();
                                let remotes_err = remotes_err.clone();
                                let views_tx = views_tx.clone();
                                async move {
                                    let link = git::Link::Net { url };
                                    let view = git::view(&host, &link).await;
                                    if view.repo.is_some() {
                                        remotes_ok.insert(link);
                                    } else {
                                        remotes_err.insert(link);
                                    }
                                    views_tx.send(view).unwrap_or_else(
                                        |_| {
                                            unreachable!(
                                                "view_rx dropped while view_tx \
                                                is still in use"
                                            )
                                        },
                                    );
                                }
                            }
                        })
                        .await;
                }
            }
            .instrument(info_span!("remotes_worker")),
        );

        let _ = locals_worker.await;
        let _ = remotes_worker.await;

        tracing::info!(
            locals = locals.len(),
            remotes_ok = remotes_ok.len(),
            remotes_err = remotes_err.len(),
            "Final counts."
        );
        Ok(())
    }
}
