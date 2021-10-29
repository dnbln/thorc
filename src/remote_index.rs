use std::{fs, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};

use crate::{error::GetIndexError, index::TemplateIndex, repo_def::RepoDef};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RemoteIndex {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,

    #[serde(flatten)]
    pub repo: RepoDef,

    // path in repo to index file
    #[serde(default = "default_remote_index_path")]
    pub path: PathBuf,
}

fn default_remote_index_path() -> PathBuf {
    PathBuf::from("index.toml")
}


impl RemoteIndex {
    pub fn get_index(&self, cache: &Path) -> Result<TemplateIndex, GetIndexError> {
        let p = self.repo.download(cache)?;

        let index_p = p.join(&self.path);

        let index_contents = fs::read_to_string(index_p)?;

        let index = toml::from_str(&index_contents)?;

        Ok(index)
    }
}
