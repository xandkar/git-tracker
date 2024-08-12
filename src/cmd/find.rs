use std::{collections::HashSet, path::PathBuf};

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
    pub fn run(&self) -> anyhow::Result<()> {
        let ignore: HashSet<PathBuf> = self.ignore.iter().cloned().collect();
        let mut roots = Vec::new();
        for path in self.dirs.iter() {
            let root = path.canonicalize()?;
            roots.push(root);
        }
        let locals = roots
            .into_iter()
            .flat_map(|d| {
                crate::files::Dirs::find(&d, ".git", self.follow, &ignore)
            })
            .filter_map(|path| crate::git::Local::read(&path).ok());
        for local in locals {
            dbg!(local);
        }
        Ok(())
    }
}
