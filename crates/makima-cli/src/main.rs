//! `makima`: inspect and manage Makima's configuration from the terminal.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use makima_core::config::{Config, DEFAULT_CONFIG};
use makima_store::Paths;

#[derive(Debug, Parser)]
#[command(
    name = "makima",
    version,
    about = "Run teams of coding agents side by side"
)]
struct Cli {
    /// Config file to use instead of ~/.config/makima/config.toml.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Work with the config file.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// List the agent presets the app offers (built-ins plus [[agents]]).
    Presets,
    /// List the templates new work can start from.
    Templates,
    /// List saved work, most recently opened first.
    Works {
        /// How many to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print where Makima keeps config, work and caches.
    Paths,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print where the config file is read from.
    Path,
    /// Write a commented config with every default.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Validate the config file and summarise it.
    Check,
}

/// The CLI's state, built once from the arguments and the environment.
#[derive(Debug)]
struct App {
    config_path: PathBuf,
}

impl App {
    /// Resolves the config path from `--config`, else the XDG location.
    fn new(config: Option<PathBuf>) -> Result<Self> {
        let config_path = match config {
            Some(path) => path,
            None => {
                Paths::resolve(None)
                    .context("locating the home directory")?
                    .config_file
            }
        };
        Ok(Self { config_path })
    }

    /// Makima's paths, with the config's `[storage] root` applied.
    fn paths(&self) -> Result<Paths> {
        let config = self.load()?;
        Paths::resolve(config.storage.root.as_deref()).context("locating the home directory")
    }

    fn run(&self, command: &Command) -> Result<()> {
        match command {
            Command::Config(ConfigCommand::Path) => {
                println!("{}", self.config_path.display());
                Ok(())
            }
            Command::Config(ConfigCommand::Init { force }) => self.init(*force),
            Command::Config(ConfigCommand::Check) => self.check(),
            Command::Presets => self.presets(),
            Command::Templates => self.templates(),
            Command::Works { limit } => self.works(*limit),
            Command::Paths => self.print_paths(),
        }
    }

    /// The config file, or the defaults if there isn't one.
    fn load(&self) -> Result<Config> {
        let path = &self.config_path;
        match fs::read_to_string(path) {
            Ok(text) => {
                Config::from_toml(&text).with_context(|| format!("parsing {}", path.display()))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    fn init(&self, force: bool) -> Result<()> {
        let path = &self.config_path;
        if path.exists() && !force {
            bail!(
                "{} already exists; pass --force to overwrite it",
                path.display()
            );
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        fs::write(path, DEFAULT_CONFIG).with_context(|| format!("writing {}", path.display()))?;
        println!("wrote {}", path.display());
        Ok(())
    }

    fn check(&self) -> Result<()> {
        let path = &self.config_path;
        if !path.exists() {
            println!("{} not found; using defaults", path.display());
            return Ok(());
        }
        let config = self.load()?;
        let appearance = &config.appearance;
        println!("{} is valid", path.display());
        println!(
            "  theme      light {:?}, dark {:?} ({:?} mode)",
            appearance.theme.light(),
            appearance.theme.dark(),
            appearance.mode
        );
        println!("  contrast   {}", appearance.contrast);
        println!("  terminal   {} pt", config.terminal.font_size);
        println!("  presets    {}", config.presets().len());
        Ok(())
    }

    fn templates(&self) -> Result<()> {
        for template in self.load()?.templates() {
            let kind = template
                .kind
                .map(makima_core::WorkKind::id)
                .unwrap_or_default();
            let access = if template.read_only {
                "  read-only"
            } else {
                ""
            };
            println!(
                "{:<12} {:<10} {:<42} {}{access}",
                template.id,
                kind,
                template.description,
                template.team_summary()
            );
        }
        Ok(())
    }

    fn works(&self, limit: usize) -> Result<()> {
        let paths = self.paths()?;
        if !paths.index_db.exists() {
            println!(
                "no saved work yet ({} doesn't exist)",
                paths.index_db.display()
            );
            return Ok(());
        }
        let store = makima_store::open(paths).context("opening the work index")?;
        for work in store
            .index()
            .recent(limit)
            .context("reading the work index")?
        {
            // `2026-10-07T09:30`; `get` can't split a character.
            let opened = work.opened_at.get(..16).unwrap_or(&work.opened_at);
            println!(
                "{opened}  {:<24} {:<9} {}",
                work.name,
                work.kind.map(makima_core::WorkKind::id).unwrap_or_default(),
                work.folder.unwrap_or_default()
            );
        }
        Ok(())
    }

    fn print_paths(&self) -> Result<()> {
        let paths = self.paths()?;
        println!("config      {}", self.config_path.display());
        println!("data        {}", paths.data_dir.display());
        println!("work        {}", paths.works_dir.display());
        println!("index       {}", paths.index_db.display());
        println!("state       {}", paths.state_dir.display());
        println!("cache       {}", paths.cache_dir.display());
        println!("recordings  {}", paths.recordings_dir.display());
        Ok(())
    }

    fn presets(&self) -> Result<()> {
        for preset in self.load()?.presets() {
            println!(
                "{:<14} {:<28} {}",
                preset.id(),
                preset.label(),
                preset.command().unwrap_or("(login shell)")
            );
        }
        Ok(())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match App::new(cli.config).and_then(|app| app.run(&cli.command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
