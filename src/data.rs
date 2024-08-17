use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sqlx::Executor;
use tokio::fs;

const MIGRATIONS: [&str; 1] = [include_str!("../migrations/0_data.sql")];

#[derive(Debug)]
pub struct View {
    pub host: String,
    pub link: Link,
    pub repo: Option<Repo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub enum Link {
    Fs { dir: PathBuf },
    Net { url: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Branch {
    pub roots: HashSet<String>,
    pub leaf: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Repo {
    pub description: Option<String>,
    pub remotes: HashMap<String, String>,
    pub branches: HashMap<String, Branch>,
}

pub struct Storage {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Storage {
    pub async fn connect<P: AsRef<Path>>(file: P) -> anyhow::Result<Self> {
        let file = file.as_ref();
        if let Some(parent) = file.parent() {
            fs::create_dir_all(&parent).await?;
        }
        let url = format!("sqlite://{}?mode=rwc", file.to_string_lossy());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        let selph = Self { pool };
        for migration in MIGRATIONS {
            selph.pool.execute(migration).await?;
        }
        Ok(selph)
    }

    pub async fn store_views(&self, views: &[View]) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for view in views {
            let View { host, link, repo } = view;
            let link = serde_json::to_string(link)?;
            let repo = serde_json::to_string(repo)?;
            let _id = sqlx::query(
                "INSERT OR REPLACE INTO views (host, link, repo) VALUES (?, ?, ?)"
            )
                .bind(host)
                .bind(link)
                .bind(repo)
                .execute(&mut *tx).await?.last_insert_rowid();
        }
        tx.commit().await?;
        Ok(())
    }
}
