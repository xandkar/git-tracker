use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail};

use crate::os;

#[derive(Debug)]
pub struct View {
    pub host: String,
    pub link: Link,
    pub repo: Option<Repo>,
}

#[derive(Debug, Clone)]
pub enum Link {
    Fs { dir: PathBuf },
    Net { url: String },
}

#[derive(Debug)]
pub struct Branch {
    pub roots: HashSet<String>,
    pub leaf: String,
}

#[derive(Debug)]
pub struct Repo {
    pub description: Option<String>,
    pub remotes: HashMap<String, String>,
    pub branches: HashMap<String, Branch>,
}

impl Repo {
    #[tracing::instrument]
    pub async fn read_from_link(link: &Link) -> anyhow::Result<Self> {
        let result = match link {
            Link::Fs { dir } => Self::read_from_fs(dir).await,
            Link::Net { url } => Self::read_from_url(url).await,
        };
        if let Err(error) = &result {
            tracing::error!(?link, ?error, "Failed to read repo.");
        }
        result
    }

    #[tracing::instrument]
    pub async fn read_from_fs<P>(dir: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        let dir = dir.as_ref();
        let selph = Self {
            description: description(dir).await?,
            branches: branches(dir).await?,
            remotes: remote_refs(dir).await?,
        };
        Ok(selph)
    }

    #[tracing::instrument]
    pub async fn read_from_url(url: &str) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let dir = dir.path();
        tracing::debug!(?url, ?dir, "Cloning");
        clone_bare(url, dir).await?;
        Self::read_from_fs(dir).await
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
        let addr = fields
            .next()
            .map(|str| str.to_string())
            .ok_or_else(|| anyhow!("Remote line missing addr: {s:?}"))?;
        Ok(Self { name, addr })
    }
}

pub async fn view(host: &str, link: &Link) -> anyhow::Result<View> {
    let view = View {
        host: host.to_string(),
        link: link.clone(),
        repo: Repo::read_from_link(link).await.ok(),
    };
    Ok(view)
}

pub async fn is_repo<P: AsRef<Path>>(dir: P) -> bool {
    let git_dir = format!("--git-dir={}", dir.as_ref().to_string_lossy());
    os::cmd("git", &[&git_dir, "log", "--format=", "-1"])
        .await
        .is_ok()
}

#[tracing::instrument(skip_all)]
async fn branches(dir: &Path) -> anyhow::Result<HashMap<String, Branch>> {
    let mut branches = HashMap::new();
    // XXX Looking up roots for all refs, rather than just branches, takes a
    //     long time for repos with many tags and long history.
    for (name, leaf) in branch_leaves(dir).await? {
        let roots = branch_roots(dir, &leaf).await?;
        branches.insert(name, Branch { roots, leaf });
    }
    Ok(branches)
}

#[tracing::instrument(skip_all)]
async fn branch_leaves(
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
pub async fn clone_bare(
    from_addr: &str,
    to_dir: &Path,
) -> anyhow::Result<()> {
    let to_dir = to_dir.to_string_lossy().to_string();
    // Q: How to prevent git from prompting for credentials and fail instead?
    // A: https://serverfault.com/a/1054253/156830
    let env = HashMap::from([
        ("GIT_SSH_COMMAND", "ssh -oBatchMode=yes"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_ASKPASS", "echo"),
        ("SSH_ASKPASS", "echo"),
        ("GCM_INTERACTIVE", "never"),
    ]);
    let exe = "git";
    let args = &["clone", "--bare", from_addr, &to_dir];
    let out = tokio::process::Command::new(exe)
        .args(args)
        .envs(&env)
        .output()
        .await?;
    out.status.success().then_some(()).ok_or_else(|| {
        anyhow!(
            "Failed to execute command: exe={exe:?} args={args:?} env={env:?} err={:?}",
            String::from_utf8_lossy(&out.stderr[..])
        )
    })
}

#[tracing::instrument(skip_all)]
async fn remote_refs(dir: &Path) -> anyhow::Result<HashMap<String, String>> {
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
pub async fn branch_roots(
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
pub async fn is_bare(dir: &Path) -> anyhow::Result<bool> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let out =
        os::cmd("git", &[&git_dir, "rev-parse", "--is-bare-repository"])
            .await?;
    let out = String::from_utf8(out)?;
    let is_bare: bool = out.trim().parse()?;
    Ok(is_bare)
}

#[tracing::instrument(skip_all)]
async fn description(dir: &Path) -> io::Result<Option<String>> {
    tokio::fs::read_to_string(dir.join("description"))
        .await
        .map(|s| (!s.starts_with("Unnamed repository;")).then_some(s))
}
