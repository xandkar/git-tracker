use std::{
    collections::HashSet,
    io::{self, BufRead},
    path::{Path, PathBuf},
};

use anyhow::anyhow;

use crate::os;

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct HeadRef {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct RemoteRef {
    pub name: String,
    pub addr: String,
    //
    // TODO The below fields indicate state, so should be in a different structure?
    // TODO Why are they not in a different structure in gg?
    // heads: Vec<HeadRef>,
    // is_reachable: bool,
}

#[derive(Debug)]
pub struct Local {
    pub hostname: String,
    pub path: PathBuf,
    pub is_bare: bool,
    pub description: Option<String>,
    pub roots: HashSet<String>,
    pub heads: HashSet<HeadRef>,
    pub remotes: HashSet<RemoteRef>,
}

impl Local {
    pub async fn read<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let selph = Self {
            hostname: os::hostname().await?,
            path: path.to_path_buf(),
            is_bare: is_bare(path).await?,
            description: description(path).await?,
            roots: roots(path).await?.into_iter().collect(),
            heads: head_refs(path).await?,
            remotes: remote_refs(path).await?,
        };
        Ok(selph)
    }
}

#[tracing::instrument]
pub async fn head_refs(dir: &Path) -> anyhow::Result<HashSet<HeadRef>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut heads = HashSet::new();
    for line_result in os::cmd("git", &[&git_dir, "show-ref", "--heads"])
        .await?
        .lines()
    {
        let line = line_result?;
        match line.split_whitespace().collect::<Vec<&str>>()[..] {
            [hash, name] => {
                let expected_prefix = "refs/heads/";
                let name =
                    name.strip_prefix(expected_prefix).ok_or_else(|| {
                        let msg =
                        "Reference name does not start with expected prefix.";
                        tracing::error!(?dir, ?expected_prefix, ?name, msg,);
                        anyhow!(msg)
                    })?;
                let head_ref = HeadRef {
                    name: name.to_string(),
                    hash: hash.to_string(),
                };
                heads.insert(head_ref);
            }
            _ => continue,
        }
    }
    Ok(heads)
}

#[tracing::instrument]
pub async fn remote_refs(dir: &Path) -> anyhow::Result<HashSet<RemoteRef>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut remotes = HashSet::new();
    for line_result in
        os::cmd("git", &[&git_dir, "remote", "-v"]).await?.lines()
    {
        let line = line_result?;
        match line.split_whitespace().collect::<Vec<&str>>()[..] {
            [name, addr] => {
                let remote_ref = RemoteRef {
                    name: name.to_string(),
                    addr: addr.to_string(),
                };
                remotes.insert(remote_ref);
            }
            _ => continue,
        }
    }
    Ok(remotes)
}

#[tracing::instrument]
pub async fn roots(dir: &Path) -> anyhow::Result<Vec<String>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut roots = Vec::new();
    for line_result in os::cmd(
        "git",
        &[&git_dir, "rev-list", "--max-parents=0", "HEAD", "--"],
    )
    .await?
    .lines()
    {
        let root = line_result?;
        roots.push(root);
    }
    Ok(roots)
}

#[tracing::instrument]
pub async fn is_bare(dir: &Path) -> anyhow::Result<bool> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let out =
        os::cmd("git", &[&git_dir, "rev-parse", "--is-bare-repository"])
            .await?;
    let out = String::from_utf8(out)?;
    let is_bare: bool = out.trim().parse()?;
    Ok(is_bare)
}

async fn description(dir: &Path) -> io::Result<Option<String>> {
    tokio::fs::read_to_string(dir.join("description"))
        .await
        .map(|s| (!s.starts_with("Unnamed repository;")).then_some(s))
}
