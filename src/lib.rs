use std::{borrow::Borrow, collections::BTreeSet, fs, io::{self, Write}, ops::Deref, path::{Path, PathBuf}, str::FromStr, time::{Duration, SystemTime}};

use flate2::read::GzDecoder;
use reqwest::{header, StatusCode};
use serde::{Deserialize, Serialize};
use sha::{
    sha512::Sha512,
    utils::{Digest, DigestExt},
};
use tar::Archive;

pub enum RO<'a, T> {
    Ref(&'a T),
    Owned(T),
}

impl<'a, T> Deref for RO<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            RO::Ref(r) => *r,
            RO::Owned(r) => r,
        }
    }
}

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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RemoteIndex {
    pub name: String,

    #[serde(flatten)]
    pub repo: RepoDef,

    // path in repo to index file
    #[serde(default = "default_remote_index_path")]
    pub path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GetIndexError {
    #[error("download error: {0}")]
    Download(#[from] DownloadError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("deserialization error: {0}")]
    DeserializeError(#[from] toml::de::Error),
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

fn default_remote_index_path() -> PathBuf {
    PathBuf::from("index.toml")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TemplateIndex {
    #[serde(default)]
    pub for_remote: bool,
    #[serde(default, rename = "template")]
    pub templates: BTreeSet<Template>,
}

#[derive(Debug, Clone)]
pub struct FindResult<'a> {
    pub name_and_description: Vec<&'a Template>,
    pub name_only: Vec<&'a Template>,
    pub description_only: Vec<&'a Template>,
}

impl<'a> FindResult<'a> {
    pub fn compose(&self, name: &'a str) -> FindResultComposite<'a> {
        FindResultComposite {
            name_and_description: self
                .name_and_description
                .iter()
                .map(|&it| (name, it))
                .collect(),
            name_only: self.name_only.iter().map(|&it| (name, it)).collect(),
            description_only: self.description_only.iter().map(|&it| (name, it)).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FindResultComposite<'a> {
    pub name_and_description: Vec<(&'a str, &'a Template)>,
    pub name_only: Vec<(&'a str, &'a Template)>,
    pub description_only: Vec<(&'a str, &'a Template)>,
}

impl<'a> FindResultComposite<'a> {
    pub fn merge_ref<'b>(&mut self, other: FindResultComposite<'b>)
    where
        'a: 'b,
        'b: 'a,
    {
        self.name_and_description.extend(other.name_and_description);
        self.name_only.extend(other.name_only);
        self.description_only.extend(other.description_only);
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.merge_ref(other);
        self
    }
}

impl TemplateIndex {
    pub fn find<'a>(&'a self, term: &str) -> FindResult<'a> {
        let (name_and_description, (name_only, description_only)): (Vec<_>, (Vec<_>, Vec<_>)) =
            self.templates
                .iter()
                .map(|t| {
                    let n = t.name().contains(term);
                    let desc = t.description().map_or(false, |d| d.contains(term));
                    if n && desc {
                        (Some(t), (None, None))
                    } else if n {
                        (None, (Some(t), None))
                    } else if desc {
                        (None, (None, Some(t)))
                    } else {
                        (None, (None, None))
                    }
                })
                .unzip();

        fn idnt<T>(v: T) -> T {
            v
        }

        FindResult {
            name_and_description: name_and_description.into_iter().filter_map(idnt).collect(),
            name_only: name_only.into_iter().filter_map(idnt).collect(),
            description_only: description_only.into_iter().filter_map(idnt).collect(),
        }
    }

    pub fn find_exact<'a>(&'a self, name: &str) -> Option<&'a Template> {
        self.templates.iter().find(|it| it.name() == name)
    }
}

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

#[derive(thiserror::Error, Debug)]
#[error("no such git provider")]
pub struct NoSuchGitProviderError;

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
}

impl RepoDef {
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

    fn download(&self, cache: &Path) -> Result<PathBuf, DownloadError> {
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

fn default_branch() -> String {
    "main".to_string()
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

fn hash_buffer(buf: &[u8]) -> String {
    Sha512::default().digest(buf).to_hex()
}

fn hash(path: &Path) -> String {
    let buf = fs::read(path).unwrap();

    hash_buffer(&buf)
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

#[derive(Debug, thiserror::Error)]
pub enum CheckTemplateNameError {
    #[error("invalid character {c:?} at {index}")]
    InvalidCharacter { c: char, index: usize },
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

// https://stackoverflow.com/a/60406693
pub fn copy<U: AsRef<Path>, V: AsRef<Path>>(from: U, to: V) -> Result<(), std::io::Error> {
    let mut stack = Vec::new();
    stack.push(PathBuf::from(from.as_ref()));

    let output_root = PathBuf::from(to.as_ref());
    let input_root = PathBuf::from(from.as_ref()).components().count();

    while let Some(working_path) = stack.pop() {
        let src: PathBuf = working_path.components().skip(input_root).collect();

        let dest = if src.components().count() == 0 {
            output_root.clone()
        } else {
            output_root.join(&src)
        };
        if fs::metadata(&dest).is_err() {
            fs::create_dir_all(&dest)?;
        }

        for entry in fs::read_dir(working_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let filename = path.file_name().unwrap();
                let dest_path = dest.join(filename);
                fs::copy(&path, &dest_path)?;
            }
        }
    }

    Ok(())
}
