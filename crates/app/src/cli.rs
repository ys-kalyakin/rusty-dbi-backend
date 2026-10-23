use clap::Parser;
use std::path::PathBuf;

use crate::config::FilterSettings;

#[derive(Parser, Debug)]
#[command(
    name = "dbi-backend",
    version,
    about = "USB backend for DBI on Nintendo Switch"
)]
pub struct Cli {
    /// Files or directories to offer to the Switch.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// An extension a directory scan accepts. Repeatable.
    #[arg(long = "ext", value_name = "EXT")]
    pub ext: Vec<String>,
    /// A pattern a scanned file must match. Repeatable.
    #[arg(long = "include", value_name = "GLOB")]
    pub include: Vec<String>,
    /// A pattern a scanned file must not match. Repeatable.
    #[arg(long = "exclude", value_name = "GLOB")]
    pub exclude: Vec<String>,
}

impl Cli {
    /// The filter settings this command line asks for. A setting the command
    /// line says nothing about is left unset, so the config file can fill it.
    pub fn filter_settings(&self) -> FilterSettings {
        FilterSettings {
            extensions: if self.ext.is_empty() {
                None
            } else {
                Some(self.ext.clone())
            },
            include: self.include.clone(),
            exclude: self.exclude.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_paths() {
        let cli = Cli::try_parse_from(["dbi-backend", "/a.nsp", "/dir"]).unwrap();
        assert_eq!(
            cli.paths,
            vec![PathBuf::from("/a.nsp"), PathBuf::from("/dir")]
        );
    }

    #[test]
    fn parses_no_paths() {
        let cli = Cli::try_parse_from(["dbi-backend"]).unwrap();
        assert!(cli.paths.is_empty());
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(Cli::try_parse_from(["dbi-backend", "--nope"]).is_err());
    }

    #[test]
    fn parses_the_filter_options() {
        let cli = Cli::try_parse_from([
            "dbi-backend",
            "--ext",
            "nsp",
            "--ext",
            "nsz",
            "--include",
            "*.nsp",
            "--exclude",
            "*PATCH*",
        ])
        .unwrap();
        assert_eq!(cli.ext, vec!["nsp".to_string(), "nsz".to_string()]);
        assert_eq!(cli.include, vec!["*.nsp".to_string()]);
        assert_eq!(cli.exclude, vec!["*PATCH*".to_string()]);
    }

    #[test]
    fn the_filter_settings_carry_what_the_command_line_asks_for() {
        let cli = Cli::try_parse_from(["dbi-backend", "--ext", "xci"]).unwrap();
        let settings = cli.filter_settings();
        assert_eq!(settings.extensions, Some(vec!["xci".to_string()]));
        assert!(settings.include.is_empty());
        assert!(settings.exclude.is_empty());
    }

    #[test]
    fn no_filter_option_asks_for_nothing() {
        let cli = Cli::try_parse_from(["dbi-backend"]).unwrap();
        let settings = cli.filter_settings();
        assert_eq!(settings.extensions, None);
        assert!(settings.include.is_empty());
        assert!(settings.exclude.is_empty());
    }
}
