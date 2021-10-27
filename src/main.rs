use std::{fs, path::PathBuf};

use clap::Parser;
use directories::ProjectDirs;
use thorc::{
    check_template_name, Config, GitProvider, RemoteIndex, RepoDef, Template, TemplateIndex, RO,
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
            eprintln!($($args)*);

            std::process::exit(1)
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
    std::fs::write(&config_file, &config_str).expect("Couldn't write local index");
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
    std::fs::write(&local_index_file, &index_str).expect("Couldn't write local index");
}

fn main() {
    let Opts {
        ref config,
        ref local_templates_index,
        subcmd,
    } = Opts::parse();

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

            thorc::copy(&template_path, &directory).expect("Cannot copy template");
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
    }
}

fn find_template<'a>(indexes: &'a [TemplateIndex], name: &str) -> Option<&'a Template> {
    for index in indexes {
        if let Some(template) = index.find_exact(name) {
            return Some(template);
        }
    }

    None
}
