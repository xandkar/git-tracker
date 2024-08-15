use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail};

use crate::os;

#[derive(Debug)]
pub struct Branch {
    pub roots: HashSet<String>,
    pub leaf: String,
}

#[derive(Debug)]
pub struct Local {
    pub hostname: String,
    pub path: PathBuf,
    pub is_bare: bool, // TODO Does it really matter for us if a repo is bare?
    pub description: Option<String>,
    pub branches: HashMap<String, Branch>,
    pub remotes: HashMap<String, String>, // TODO Parse/validate URL/addr.
}

impl Local {
    #[tracing::instrument]
    pub async fn read<P>(dir: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        let dir = dir.as_ref();
        let selph = Self {
            is_bare: local_is_bare(dir).await?,
            description: local_description(dir).await?,
            branches: local_branches(dir).await?,
            remotes: local_remote_refs(dir).await?,
            path: dir.to_path_buf(),
            hostname: os::hostname().await?,
        };
        Ok(selph)
    }
}

#[derive(Debug)]
struct TreeRef {
    pub name: String,
    pub hash: String,
}

impl FromStr for TreeRef {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut fields = s.split_whitespace();
        let hash = fields
            .next()
            .map(|str| str.to_string())
            .ok_or_else(|| anyhow!("Ref line is empty: {s:?}"))?;
        let name = fields
            .next()
            .map(|str| str.to_string())
            .ok_or_else(|| anyhow!("Ref line missing path: {s:?}"))?;
        if fields.next().is_some() {
            bail!("Ref line has too many fields: {s:?}");
        }
        Ok(Self { name, hash })
    }
}

#[derive(Debug)]
struct RemoteRef {
    pub name: String,
    pub addr: String,
}

impl FromStr for RemoteRef {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut fields = s.split_whitespace();
        let name = fields
            .next()
            .map(|str| str.to_string())
            .ok_or_else(|| anyhow!("Remote line is empty: {s:?}"))?;
        // TODO Parse/validate addr/URL?
        let addr = fields
            .next()
            .map(|str| str.to_string())
            .ok_or_else(|| anyhow!("Remote line missing addr: {s:?}"))?;
        Ok(Self { name, addr })
    }
}

pub async fn local_is_repo<P: AsRef<Path>>(dir: P) -> bool {
    let git_dir = format!("--git-dir={}", dir.as_ref().to_string_lossy());
    os::cmd("git", &[&git_dir, "log", "--format=", "-1"])
        .await
        .is_ok()
}

#[tracing::instrument(skip_all)]
async fn local_branches(
    dir: &Path,
) -> anyhow::Result<HashMap<String, Branch>> {
    let mut branches = HashMap::new();
    // XXX Looking up roots for all refs, rather than just branches, takes a
    //     long time for repos with many tags and long history.
    for (name, leaf) in local_branch_leaves(dir).await? {
        let roots = local_branch_roots(dir, &leaf).await?;
        branches.insert(name, Branch { roots, leaf });
    }
    Ok(branches)
}

#[tracing::instrument(skip_all)]
async fn local_branch_leaves(
    dir: &Path,
) -> anyhow::Result<HashMap<String, String>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut refs = HashMap::new();
    for line_result in os::cmd("git", &[&git_dir, "show-ref", "--branches"])
        .await?
        .lines()
    {
        let line: String = line_result?;
        let TreeRef { name, hash } = line.parse()?;
        if let Some(name) = name.strip_prefix("refs/heads/") {
            refs.insert(name.to_string(), hash);
        }
    }
    Ok(refs)
}

#[tracing::instrument(skip_all)]
async fn local_remote_refs(
    dir: &Path,
) -> anyhow::Result<HashMap<String, String>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut remotes = HashMap::new();
    for line_result in
        os::cmd("git", &[&git_dir, "remote", "-v"]).await?.lines()
    {
        let line = line_result?;
        let RemoteRef { name, addr } = line.parse()?;
        remotes.insert(name, addr);
    }
    Ok(remotes)
}

#[tracing::instrument(skip(dir))]
pub async fn local_branch_roots(
    dir: &Path,
    leaf_hash: &str,
) -> anyhow::Result<HashSet<String>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let output = os::cmd(
        "git",
        &[&git_dir, "rev-list", "--max-parents=0", leaf_hash, "--"],
    )
    .await?;
    let roots: HashSet<String> =
        output.lines().map_while(Result::ok).collect();
    if roots.is_empty() {
        bail!("Found 0 roots for leaf hash {leaf_hash} in repo={dir:?}");
    }
    Ok(roots)
}

#[tracing::instrument(skip_all)]
pub async fn local_is_bare(dir: &Path) -> anyhow::Result<bool> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let out =
        os::cmd("git", &[&git_dir, "rev-parse", "--is-bare-repository"])
            .await?;
    let out = String::from_utf8(out)?;
    let is_bare: bool = out.trim().parse()?;
    Ok(is_bare)
}

#[tracing::instrument(skip_all)]
async fn local_description(dir: &Path) -> io::Result<Option<String>> {
    tokio::fs::read_to_string(dir.join("description"))
        .await
        .map(|s| (!s.starts_with("Unnamed repository;")).then_some(s))
}
