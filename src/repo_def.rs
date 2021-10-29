use std::{fs, io::{self, Write}, path::{Path, PathBuf}, str::FromStr, time::{Duration, SystemTime}};

use flate2::read::GzDecoder;
use reqwest::{StatusCode, header};
use serde::{Deserialize, Serialize};
use tar::Archive;

use crate::{error::{DownloadError, NoSuchGitProviderError}, utils::hash};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GitProvider {
    GitHub,
    GitLab,
}

impl GitProvider {
    fn simple_name(&self) -> &'static str {
        match self {
            GitProvider::GitHub => "github",
            GitProvider::GitLab => "gitlab",
        }
    }
}

impl FromStr for GitProvider {
    type Err = NoSuchGitProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let gp = match s {
            "github" | "GitHub" => GitProvider::GitHub,
            "gitlab" | "GitLab" => GitProvider::GitLab,
            _ => return Err(NoSuchGitProviderError),
        };

        Ok(gp)
    }
}

impl Default for GitProvider {
    fn default() -> Self {
        Self::GitHub
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RepoDef {
    #[serde(default)]
    pub git_provider: GitProvider,

    pub user: String,
    pub repo: String,

    #[serde(default = "default_branch")]
    pub git_ref: String,
}

impl RepoDef {
    pub fn link(&self) -> String {
        match self.git_provider {
            GitProvider::GitHub => format!(
                "https://github.com/{}/{}/tree/{}",
                self.user, self.repo, self.git_ref
            ),
            GitProvider::GitLab => format!(
                "https://gitlab.com/{}/{}/-/tree/{}",
                self.user, self.repo, self.git_ref
            ),
        }
    }

    fn cache_file(&self) -> String {
        format!(
            "{}_{}_{}_{}",
            self.git_provider.simple_name(),
            self.user,
            self.repo,
            self.git_ref
        )
    }

    fn archive_link(&self) -> String {
        match self.git_provider {
            GitProvider::GitHub => format!(
                "https://github.com/{}/{}/archive/{}.tar.gz",
                self.user, self.repo, self.git_ref
            ),
            GitProvider::GitLab => format!(
                "https://gitlab.com/api/v4/projects/{}%2F{}/repository/archive.tar.gz?sha={}",
                self.user, self.repo, self.git_ref
            ),
        }
    }

    pub(crate) fn download(&self, cache: &Path) -> Result<PathBuf, DownloadError> {
        if !cache.exists() {
            fs::create_dir_all(cache)?;
        }

        let file = self.cache_file();
        let tar_file = format!("{}.tar.gz", file);
        let link = self.archive_link();

        let path = cache.join(tar_file);

        let etag_f = path.with_extension("etag");
        if path.exists() {
            let md = path.metadata()?;
            let created = md.modified()?;

            if SystemTime::now() > created + Duration::from_secs(60) {
                download_file(&link, &path, Some(&etag_f))?;
            }
        } else {
            download_file(&link, &path, Some(&etag_f))?;
        }

        let hash = hash(&path);

        let out_dir = cache.join(format!("{}-{}", file, hash));

        if out_dir.exists() {
            return Ok(out_dir);
        }

        fs::create_dir_all(&out_dir)?;

        let tar_gz = fs::File::open(&path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut a = Archive::new(tar);
        a.unpack(&out_dir)?;

        flatten(&out_dir)?;

        Ok(out_dir)
    }
}

fn default_branch() -> String {
    "main".to_string()
}

fn flatten(out_dir: &Path) -> io::Result<()> {
    // has only one child
    let entry = out_dir.read_dir()?.next().unwrap()?;

    let children = entry
        .path()
        .read_dir()?
        .map(|child| {
            let child = child?;
            let c = child.path();

            Ok::<_, io::Error>((c, out_dir.join(child.file_name())))
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (src, dest) in children {
        fs::rename(src, dest)?;
    }

    fs::remove_dir(entry.path())?;

    Ok(())
}

fn download_file(url: &str, path: &Path, etag_f: Option<&Path>) -> Result<(), DownloadError> {
    let prev_etag = etag_f.and_then(|it| {
        if it.exists() {
            fs::read_to_string(it).ok()
        } else {
            None
        }
    });

    let cl = reqwest::blocking::Client::new();
    let req = cl.get(url);
    let req = prev_etag
        .iter()
        .fold(req, |req, etag| req.header(header::IF_NONE_MATCH, etag));
    let resp = req.send()?.error_for_status()?;

    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(());
    }

    let etag = {
        let headers = resp.headers();
        headers
            .get(header::ETAG)
            .and_then(|it| Some(it.to_str().unwrap()))
    };

    if let Some(etag) = etag {
        if let Some(etag_f) = etag_f {
            fs::write(etag_f, etag)?;
        }
    }

    let mut f = fs::File::create(path)?;

    let bytes = resp.bytes()?;
    f.write_all(&bytes)?;

    Ok(())
}
