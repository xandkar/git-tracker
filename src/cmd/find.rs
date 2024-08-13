use std::{collections::HashSet, path::PathBuf};

use futures::{stream, FutureExt, StreamExt};

#[derive(clap::Args, Debug, Clone)]
pub struct Cmd {
    /// Follow symbollic links.
    #[clap(short, long, default_value_t = false)]
    follow: bool,

    /// Ignore this path when searching for repos.
    #[clap(short, long)]
    ignore: Vec<PathBuf>,

    dirs: Vec<PathBuf>,
}

impl Cmd {
    pub async fn run(&self) -> anyhow::Result<()> {
        let ignore: HashSet<PathBuf> = self.ignore.iter().cloned().collect();
        let mut roots = Vec::new();
        for path in self.dirs.iter() {
            let root = path.canonicalize()?;
            roots.push(root);
        }
        stream::iter(roots)
            .flat_map(|root| {
                crate::files::find_dirs(&root, ".git", self.follow, &ignore)
            })
            .filter_map(|path| {
                crate::git::Local::read(path).map(|res| res.ok())
            })
            .for_each_concurrent(None, |repo| async {
                dbg!(repo);
            })
            .await;
        Ok(())
    }
}
