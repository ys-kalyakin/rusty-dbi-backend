mod cli;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use dbi_backend_core::filelist::FileList;

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        if let Err(e) = files.add_path(path) {
            eprintln!("warning: failed to add {}: {e}", path.display());
        }
    }

    println!("{} file(s) available", files.len());
    Ok(())
}
