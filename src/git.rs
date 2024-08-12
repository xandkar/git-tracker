use std::{
    collections::HashSet,
    fs,
    io::{self, BufRead},
    path::{Path, PathBuf},
};

use anyhow::anyhow;

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
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let selph = Self {
            hostname: hostname()?,
            path: path.to_path_buf(),
            is_bare: is_bare(path)?,
            description: description(path)?,
            roots: roots(path)?.into_iter().collect(),
            heads: heads(path)?,
            remotes: remotes(path)?,
        };
        Ok(selph)
    }
}

#[tracing::instrument]
pub fn heads(dir: &Path) -> anyhow::Result<HashSet<HeadRef>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut heads = HashSet::new();
    for line_result in cmd("git", &[&git_dir, "show-ref", "--heads"])?.lines()
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
pub fn remotes(dir: &Path) -> anyhow::Result<HashSet<RemoteRef>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut remotes = HashSet::new();
    for line_result in cmd("git", &[&git_dir, "remote", "-v"])?.lines() {
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
pub fn roots(dir: &Path) -> anyhow::Result<Vec<String>> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let mut roots = Vec::new();
    for line_result in cmd(
        "git",
        &[&git_dir, "rev-list", "--max-parents=0", "HEAD", "--"],
    )?
    .lines()
    {
        let root = line_result?;
        roots.push(root);
    }
    Ok(roots)
}

#[tracing::instrument]
pub fn is_bare(dir: &Path) -> anyhow::Result<bool> {
    let git_dir = format!("--git-dir={}", dir.to_string_lossy());
    let out = cmd("git", &[&git_dir, "rev-parse", "--is-bare-repository"])?;
    let out = String::from_utf8(out)?;
    let is_bare: bool = out.trim().parse()?;
    Ok(is_bare)
}

fn description(dir: &Path) -> io::Result<Option<String>> {
    fs::read_to_string(dir.join("description"))
        .map(|s| (!s.starts_with("Unnamed repository;")).then_some(s))
}

fn hostname() -> anyhow::Result<String> {
    // TODO Consider a cross-platofrm way to lookup hostname.
    let bytes = cmd("hostname", &[])?;
    let str = String::from_utf8(bytes)?;
    let str = str.trim();
    Ok(str.to_string())
}

fn cmd(exe: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = std::process::Command::new(exe).args(args).output()?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        tracing::error!(
            ?exe,
            ?args,
            ?out,
            stderr = ?String::from_utf8_lossy(&out.stderr[..]),
            "Failed to execute command."
        );
        Err(anyhow!("Failed to execute command: {exe:?} {args:?}"))
    }
}
