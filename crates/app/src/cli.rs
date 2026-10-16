use clap::Parser;
use std::path::PathBuf;

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
}
