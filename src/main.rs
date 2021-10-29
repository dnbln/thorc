use std::{
    fmt::Write,
    fs,
    io::{self, BufRead, Write as IoWrite},
    path::{Components, Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    str::FromStr,
};

use clap::Parser;
use directories::ProjectDirs;
use thorc::{
    config::Config,
    index::TemplateIndex,
    remote_index::RemoteIndex,
    repo_def::{GitProvider, RepoDef},
    ro::RO,
    template::check_template_name,
    template::{SetupKind, Template},
};

#[derive(Parser)]
struct Opts {
    #[clap(short, long, parse(from_os_str))]
    config: Option<PathBuf>,

    #[clap(short = 'i', long = "index", parse(from_os_str))]
    local_templates_index: Option<PathBuf>,

    #[clap(subcommand)]
    subcmd: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
    AddToIndex(AddToIndexCommand),
    AddLocalToIndex(AddLocalToIndexCommand),
    RemoveFromIndex(RemoveFromIndexCommand),
    List,
    Find(FindCommand),
    New(NewCommand),
    AddRemoteIndex(AddRemoteIndexCommand),
    RemoveRemoteIndex(RemoveRemoteIndexCommand),

    // utils
    EditToml(EditTomlCommand),
    EditJson(EditJsonCommand),
}

#[derive(Parser)]
pub struct AddToIndexCommand {
    #[clap(long, parse(try_from_str), default_value = "github")]
    git_provider: GitProvider,
    #[clap(short, long)]
    user: String,
    #[clap(long)]
    repo: String,
    #[clap(long)]
    git_ref: String,
    #[clap(long)]
    issue: Option<usize>,
    #[clap(long)]
    description: Option<String>,

    name: String,
}

#[derive(Parser)]
pub struct AddLocalToIndexCommand {
    #[clap(parse(from_os_str))]
    path: PathBuf,
    #[clap(long)]
    description: Option<String>,
    name: String,
}

#[derive(Parser)]
pub struct RemoveFromIndexCommand {
    name: String,
}

#[derive(Parser)]
pub struct FindCommand {
    term: String,
}

pub enum IndexName {
    Local,
    Remote(String),
}

impl<'a> From<&'a str> for IndexName {
    fn from(s: &'a str) -> Self {
        match s {
            "local" => IndexName::Local,
            s => IndexName::Remote(s.to_string()),
        }
    }
}

#[derive(Parser)]
pub struct NewCommand {
    #[clap(short, long, parse(from_str))]
    index: Option<IndexName>,
    template_name: String,
    #[clap(long)]
    project_name: Option<String>,
    directory: PathBuf,
    #[clap(long)]
    allow_dirty: bool,
}

#[derive(Parser)]
pub struct AddRemoteIndexCommand {
    #[clap(long, parse(try_from_str), default_value = "github")]
    git_provider: GitProvider,
    #[clap(short, long)]
    user: String,
    #[clap(long)]
    repo: String,
    #[clap(long)]
    git_ref: String,
    #[clap(long, parse(from_os_str))]
    path: PathBuf,
    #[clap(long)]
    description: Option<String>,

    name: String,
}

#[derive(Parser)]
pub struct RemoveRemoteIndexCommand {
    name: String,
}

#[derive(Parser)]
pub struct EditTomlCommand {
    toml_file: PathBuf,
    objcet_path: ObjectPath,
}

#[derive(Parser)]
pub struct EditJsonCommand {
    json_file: PathBuf,
    objcet_path: ObjectPath,
}

pub struct ObjectPath {
    pb: PathBuf,
}

impl FromStr for ObjectPath {
    type Err = <PathBuf as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(|pb| Self { pb })
    }
}

const NAME: &'static str = env!("CARGO_PKG_NAME");
const CONFIG_FILE_NAME: &'static str = concat!(env!("CARGO_PKG_NAME"), ".conf");

fn proj_dirs() -> ProjectDirs {
    ProjectDirs::from("", "", NAME).unwrap()
}

fn config_dir() -> PathBuf {
    let proj_dirs = proj_dirs();
    proj_dirs.config_dir().to_owned()
}

fn cache_dir() -> PathBuf {
    let proj_dirs = proj_dirs();
    proj_dirs.cache_dir().to_owned()
}

fn config_file() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

fn local_index_file() -> PathBuf {
    config_dir().join("local_templates.toml")
}

macro_rules! err {
    ($($args:tt)*) => {
        {
            panic!($($args)*)
        }
    };
}

fn load_config(config: &Option<PathBuf>) -> (PathBuf, Config) {
    let config_file = config.clone().unwrap_or_else(config_file);
    let config = fs::read_to_string(&config_file).expect("Cannot read config file");
    let config = toml::from_str::<Config>(&config).expect("Cannot parse config file");

    (config_file, config)
}

fn edit_config<F>(config: &Option<PathBuf>, f: F)
where
    F: FnOnce(Config) -> Config,
{
    let (config_file, config) = load_config(config);
    let config = f(config);

    let config_str = toml::to_string_pretty(&config).expect("Couldn't serialize local index");
    fs::write(&config_file, &config_str).expect("Couldn't write local index");
}

fn load_local_index(local_templates_index: &Option<PathBuf>) -> (PathBuf, TemplateIndex) {
    let local_index_file = local_templates_index
        .clone()
        .unwrap_or_else(local_index_file);
    let local_index = fs::read_to_string(&local_index_file).expect("Cannot read local index file");
    let local_index =
        toml::from_str::<TemplateIndex>(&local_index).expect("Cannot parse local index file");

    (local_index_file, local_index)
}

fn edit_index<F>(local_templates_index: &Option<PathBuf>, f: F)
where
    F: FnOnce(TemplateIndex) -> TemplateIndex,
{
    let (local_index_file, local_index) = load_local_index(local_templates_index);
    let local_index = f(local_index);

    let index_str = toml::to_string_pretty(&local_index).expect("Couldn't serialize local index");
    fs::write(&local_index_file, &index_str).expect("Couldn't write local index");
}

fn self_bin_path() -> PathBuf {
    std::env::current_exe().expect("Cannot get self binary")
}

fn main() {
    let Opts {
        ref config,
        ref local_templates_index,
        subcmd,
    } = Opts::parse();

    tracing_subscriber::fmt::SubscriberBuilder::default()
        .pretty()
        .init();

    let cache = cache_dir();

    match subcmd {
        Subcommand::AddToIndex(AddToIndexCommand {
            git_provider,
            user,
            repo,
            git_ref,
            issue,
            name,
            description,
        }) => edit_index(local_templates_index, |mut local_index| {
            if let Some(t) = local_index.templates.iter().find(|it| it.name() == name) {
                err!("Template already exists in index, pointing to {:?}", t);
            }

            if let Err(err) = check_template_name(&name) {
                err!("Invalid name: {}", err);
            }

            let t = Template::Repo {
                name,
                description,
                repo: RepoDef {
                    git_provider,
                    user,
                    repo,
                    git_ref,
                },
                issue,
                setup: None,
            };

            local_index.templates.insert(t);

            local_index
        }),
        Subcommand::AddLocalToIndex(AddLocalToIndexCommand {
            path,
            description,
            name,
        }) => edit_index(local_templates_index, |mut local_index| {
            if local_index.for_remote {
                err!("Local templates may not be added to indexes intended to be used remotely");
            }

            if let Err(err) = check_template_name(&name) {
                err!("Invalid name: {}", err);
            }

            if let Some(t) = local_index.templates.iter().find(|it| it.name() == name) {
                err!("Template already exists in index, pointing to {:?}", t);
            }

            let t = Template::Local {
                name,
                description,
                path,
            };

            local_index.templates.insert(t);

            local_index
        }),
        Subcommand::RemoveFromIndex(RemoveFromIndexCommand { name }) => {
            edit_index(local_templates_index, |mut local_index| {
                if let Err(err) = check_template_name(&name) {
                    err!("Invalid name: {}", err);
                }

                if !local_index.templates.remove(name.as_str()) {
                    err!("Template {} doesn't exists in index", name);
                }

                local_index
            })
        }
        Subcommand::List => {
            let (_, local_index) = load_local_index(local_templates_index);

            for template in local_index.templates.iter() {
                println!("{}", template.one_line_summary());
            }
        }
        Subcommand::Find(FindCommand { term }) => {
            let (_, local_index) = load_local_index(local_templates_index);
            let (_, config) = load_config(config);

            let first_result = local_index.find(&term);
            let mut result = first_result.compose("<local>");

            let remote_indexes = config
                .remote_indexes
                .iter()
                .map(|remote_index| {
                    (
                        &remote_index.name,
                        remote_index.get_index(&cache).expect("Cannot get index"),
                    )
                })
                .collect::<Vec<_>>();

            for (remote_name, index) in remote_indexes.iter() {
                let find_result = index.find(&term);
                let composed = find_result.compose(*remote_name);
                result.merge_ref(composed);
            }

            if !result.name_and_description.is_empty() {
                println!("Templates that matched both name and description:");

                for &(index, template) in result.name_and_description.iter() {
                    println!("[{}] {}", index, template.one_line_summary());
                }
            }

            if !result.name_only.is_empty() {
                println!("Templates that matched only name:");

                for &(index, template) in result.name_only.iter() {
                    println!("[{}] {}", index, template.one_line_summary());
                }
            }

            if !result.description_only.is_empty() {
                println!("Templates that matched only description:");

                for &(index, template) in result.description_only.iter() {
                    println!("[{}] {}", index, template.one_line_summary());
                }
            }
        }
        Subcommand::New(NewCommand {
            index,
            template_name,
            project_name,
            directory,
            allow_dirty,
        }) => {
            let (_, local_index) = load_local_index(local_templates_index);
            let (_, config) = load_config(config);

            if let Err(err) = check_template_name(&template_name) {
                err!("Invalid name: {}", err);
            }

            if directory.exists() {
                if !directory.is_dir() {
                    err!(
                        "{} already exists and is not a directory",
                        directory.display()
                    );
                } else if !allow_dirty && directory.read_dir().unwrap().next().is_some() {
                    err!("{} already exists and is not empty", directory.display());
                }
            }

            let indexes = config
                .get_all_remote_indexes(&cache)
                .expect("Cannot get indexes");

            let index_v = index.map(|it| match it {
                IndexName::Local => RO::Ref(&local_index),
                IndexName::Remote(r) => {
                    match config.remote_indexes.iter().find(|it| it.name == r) {
                        Some(index) => {
                            RO::Owned(index.get_index(&cache).expect("Cannot get index"))
                        }
                        None => err!("Invalid index: {}", r),
                    }
                }
            });

            let template = match &index_v {
                Some(index) => index.find_exact(&template_name),
                None => local_index
                    .find_exact(&template_name)
                    .or_else(|| find_template(&indexes, &template_name)),
            };

            let template = match template {
                Some(template) => template,
                None => err!("Unknown template: {}", template_name),
            };

            let template_path = template.download(&cache).expect("Cannot download template");

            fs::create_dir_all(&directory).expect("Cannot create directory");

            thorc::utils::copy(&template_path, &directory).expect("Cannot copy template");

            finish_setup(
                &self_bin_path(),
                &template,
                &directory,
                project_name
                    .as_ref()
                    .map(|it| it.as_str())
                    .unwrap_or_else(|| directory.file_name().unwrap().to_str().unwrap()),
            )
            .expect("Cannot finish setup");
        }
        Subcommand::AddRemoteIndex(AddRemoteIndexCommand {
            name,
            description,
            git_provider,
            user,
            repo,
            git_ref,
            path,
        }) => edit_config(config, |mut config| {
            if name == "local" {
                err!("Cannot add a remote index named 'local'");
            }

            let remote_index = RemoteIndex {
                name,
                description,
                path,
                repo: RepoDef {
                    git_provider,
                    user,
                    repo,
                    git_ref,
                },
            };

            config.remote_indexes.push(remote_index);

            config
        }),
        Subcommand::RemoveRemoteIndex(RemoveRemoteIndexCommand { name }) => {
            edit_config(config, |mut config| {
                if name == "local" {
                    err!("Cannot remove index named 'local'");
                }

                let remote_index = config
                    .remote_indexes
                    .iter()
                    .enumerate()
                    .find(|(_, index)| index.name == name)
                    .unwrap_or_else(|| err!("No remote called '{}' found", name))
                    .0;

                config.remote_indexes.remove(remote_index);

                config
            })
        }
        Subcommand::EditToml(EditTomlCommand {
            toml_file,
            objcet_path,
        }) => {
            let stdin = io::stdin();
            let mut input_str = String::new();

            for line in stdin.lock().lines() {
                writeln!(&mut input_str, "{}", line.unwrap()).unwrap();
            }

            let mut input = input_str
                .parse::<toml_edit::Document>()
                .expect("Failed to parse input");
            let input = std::mem::replace(&mut input["value"], toml_edit::Item::None);
            let mut toml_file_value = fs::read_to_string(&toml_file)
                .unwrap()
                .parse::<toml_edit::Document>()
                .unwrap();

            patch_toml(
                &mut toml_file_value.root,
                input,
                &mut objcet_path.pb.components(),
            );

            let toml_file_str = toml_file_value.to_string();
            fs::write(&toml_file, toml_file_str).unwrap();
        }
        Subcommand::EditJson(EditJsonCommand {
            json_file,
            objcet_path,
        }) => {
            let stdin = io::stdin();
            let mut input_str = String::new();

            for line in stdin.lock().lines() {
                writeln!(&mut input_str, "{}", line.unwrap()).unwrap();
            }

            let input = serde_json::from_str::<serde_json::Value>(&input_str)
                .expect("Failed to parse input");
            let mut json_file_value =
                serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&json_file).unwrap())
                    .unwrap();

            patch_json(
                &mut json_file_value,
                input,
                &mut objcet_path.pb.components(),
            );

            let json_file_str = serde_json::to_string_pretty(&json_file_value).unwrap();
            fs::write(&json_file, json_file_str).unwrap();
        }
    }
}

fn patch_toml(
    original_value: &mut toml_edit::Item,
    new_value: toml_edit::Item,
    path: &mut Components,
) {
    let next = path.next();

    match next {
        Some(c) => {
            let c = c.as_os_str().to_str().unwrap();

            if let Ok(int) = usize::from_str(c) {
                patch_toml(&mut original_value[int], new_value, path);
            } else {
                patch_toml(&mut original_value[c], new_value, path);
            }
        }
        None => {
            *original_value = new_value;
        }
    }
}

fn patch_json(
    original_value: &mut serde_json::Value,
    new_value: serde_json::Value,
    path: &mut Components,
) {
    let next = path.next();

    match next {
        Some(c) => {
            let c = c.as_os_str().to_str().unwrap();

            if let Ok(int) = usize::from_str(c) {
                patch_json(
                    &mut original_value.as_array_mut().unwrap()[int],
                    new_value,
                    path,
                );
            } else {
                patch_json(
                    &mut original_value.as_object_mut().unwrap()[c],
                    new_value,
                    path,
                );
            }
        }
        None => {
            *original_value = new_value;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunHookError {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("status not success: {0}")]
    StatusNotSuccess(ExitStatus),
}

fn hook_path(dir: &Path, name: &str) -> PathBuf {
    let mut pb = dir.join("thor");
    pb.push(name);
    pb
}

fn hook_exists(dir: &Path, name: &str) -> bool {
    hook_path(dir, name).exists()
}

fn run_hook<F>(
    self_bin: &Path,
    directory: &Path,
    hook_name: &str,
    args: F,
) -> Result<(), RunHookError>
where
    F: for<'a> FnOnce(&'a mut Command) -> &'a mut Command,
{
    let hook = hook_path(directory, hook_name);

    if hook.exists() {
        if hook.is_file() {
            let mut cmd = std::process::Command::new(&hook);
            args(&mut cmd);
            cmd.env("THORC", self_bin);

            tracing::debug!("Running: {:?}", cmd);

            let mut child = cmd.spawn()?;
            let exit = child.wait()?;

            if !exit.success() {
                return Err(RunHookError::StatusNotSuccess(exit));
            }
        } else {
            tracing::warn!("Looks like {} is not a file", hook.display());
        }
    } else {
        tracing::info!("Looks like {} doesn't exist, not running", hook.display());
    }

    Ok(())
}

fn finish_setup(
    self_bin: &Path,
    template: &Template,
    directory: &Path,
    project_name: &str,
) -> Result<(), RunHookError> {
    const SETUP_HOOK_NAME: &'static str = "setup";

    if hook_exists(directory, SETUP_HOOK_NAME) {
        run_hook(self_bin, directory, SETUP_HOOK_NAME, |command| {
            command.arg(directory).arg(project_name)
        })
    } else {
        if let Template::Repo {
            setup: Some(setup_kind),
            ..
        } = template
        {
            match setup_kind {
                SetupKind::Rust => run_sh(
                    r#"#!/usr/bin/env bash
                        dir="$1"
                        name="$2"
    
                        echo "Setting up for rust" >&2
                        echo "value = \"$name\"" | $THORC edit-toml "$dir/Cargo.toml" "package/name" || exit $?
                        "#,
                    |cmd| cmd.arg(directory).arg(project_name),
                ),
                SetupKind::Npm => run_sh(
                    r#"#!/usr/bin/env bash
                    dir="$1"
                    name="$2"

                    echo "Setting up for npm" >&2
                    echo "\"$name\"" | $THORC edit-json "$dir/package.json" "name" || exit $?
                    "#,
                    |cmd| cmd.arg(directory).arg(project_name),
                ),
            }
        } else {
            tracing::warn!(
                "No setup hook found for {}; you may need to change some things manually",
                template.name()
            );
            Ok(())
        }
    }
}

fn run_sh<F>(sh: &str, args: F) -> Result<(), RunHookError>
where
    F: FnOnce(&mut Command) -> &mut Command,
{
    let mut cmd = std::process::Command::new("/usr/bin/env");
    cmd.stdin(Stdio::piped()).arg("bash").arg("-s").arg("-");
    args(&mut cmd);
    cmd.env("THORC", self_bin_path());

    tracing::debug!("Running: {:?}", cmd);

    let mut child = cmd.spawn()?;

    write!(&mut child.stdin.as_ref().unwrap(), "{}", sh)?;

    let exit = child.wait()?;

    if !exit.success() {
        return Err(RunHookError::StatusNotSuccess(exit));
    }

    Ok(())
}

fn find_template<'a>(indexes: &'a [TemplateIndex], name: &str) -> Option<&'a Template> {
    for index in indexes {
        if let Some(template) = index.find_exact(name) {
            return Some(template);
        }
    }

    None
}
