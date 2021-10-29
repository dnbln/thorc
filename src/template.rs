use std::{
    borrow::Borrow,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{CheckTemplateNameError, DownloadError},
    repo_def::RepoDef,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Template {
    Repo {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        #[serde(flatten)]
        repo: RepoDef,

        /// issue the template was added from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        issue: Option<usize>,

        /// setup kind
        #[serde(default, skip_serializing_if = "Option::is_none")]
        setup: Option<SetupKind>,
    },
    Local {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        path: PathBuf,
    },
}

impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.name().eq(other.name())
    }
}
impl Eq for Template {}

impl PartialOrd for Template {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Template {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name().cmp(other.name())
    }
}

impl Borrow<str> for Template {
    fn borrow(&self) -> &str {
        self.name()
    }
}

impl Template {
    pub fn download(&self, cache: &Path) -> Result<PathBuf, DownloadError> {
        match self {
            Template::Repo { repo, .. } => repo.download(cache),
            Template::Local { path, .. } => Ok(path.clone()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Template::Repo { name, .. } => name,
            Template::Local { name, .. } => name,
        }
    }

    pub fn description(&self) -> Option<&String> {
        match self {
            Template::Repo { description, .. } => description.as_ref(),
            Template::Local { description, .. } => description.as_ref(),
        }
    }

    pub fn one_line_summary(&self) -> String {
        match self {
            Template::Repo {
                name,
                description,
                repo,
                issue,
                ..
            } => {
                let issue_text = issue.map(|it| format!("for issue {}", it));
                let desc_text = description.as_ref();
                let extra_text = match (desc_text, issue_text) {
                    (Some(desc), Some(issue)) => format!(" {} [{}]", desc, issue),
                    (Some(desc), None) => format!(" {}", desc),
                    (None, Some(issue)) => format!("[for issue {}]", issue),
                    (None, None) => format!(""),
                };
                format!("{} => {}{}", name, repo.link(), extra_text)
            }
            Template::Local {
                name,
                description,
                path,
            } => {
                let desc_text = description.as_ref();
                let extra_text = match desc_text {
                    Some(desc) => format!(" {}", desc),
                    None => format!(""),
                };
                format!("{} => {}{}", name, path.display(), extra_text)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum SetupKind {
    Rust,
    Npm,
}

pub fn check_template_name(name: &str) -> Result<(), CheckTemplateNameError> {
    if let Some((index, c)) = name.chars().enumerate().find(|(_, it)| {
        !('a'..='z').contains(it)
            && !('A'..='Z').contains(it)
            && !('0'..='9').contains(it)
            && !"-_".contains(*it)
    }) {
        return Err(CheckTemplateNameError::InvalidCharacter { c, index });
    }

    Ok(())
}
