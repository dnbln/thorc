use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{error::GetIndexError, index::TemplateIndex, remote_index::RemoteIndex};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    #[serde(default, rename = "remote_index")]
    pub remote_indexes: Vec<RemoteIndex>,
}

impl Config {
    pub fn get_all_remote_indexes_and_names<'a>(
        &'a self,
        cache: &Path,
    ) -> Result<Vec<(&'a str, TemplateIndex)>, GetIndexError> {
        self.remote_indexes
            .iter()
            .map(|it| Ok((it.name.as_str(), it.get_index(cache)?)))
            .collect()
    }

    pub fn get_all_remote_indexes<'a>(
        &'a self,
        cache: &Path,
    ) -> Result<Vec<TemplateIndex>, GetIndexError> {
        self.remote_indexes
            .iter()
            .map(|it| it.get_index(cache))
            .collect()
    }
}
