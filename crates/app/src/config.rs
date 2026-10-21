//! Share definitions from the configuration file.
//!
//! Shares are configured in `~/.config/dbi-backend/config.toml` under a
//! `[[shares]]` array, one entry per share. Every entry needs `name` (a
//! display name), `server` (`host` or `host:port`, port 445 by default),
//! `share`, and `username`; optional `domain` and `root` (the browsable root
//! inside the share) are also read. Passwords are deliberately not configurable
//! here — they are asked at every connection.

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub use dbi_backend_core::source::smb::ShareConfig;

/// The configuration file's `[[shares]]` entries.
#[derive(Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    shares: Vec<toml::Table>,
    /// The `[filters]` section: which files a scan accepts.
    #[serde(default)]
    filters: Option<toml::Table>,
}

/// The filter settings one source asks for — the command line or the config
/// file. A setting the source says nothing about is unset; an empty list means
/// the source asked for nothing, not that it asked for the default.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FilterSettings {
    /// The accepted extensions, when the source names a set.
    pub extensions: Option<Vec<String>>,
    /// The include patterns.
    pub include: Vec<String>,
    /// The exclude patterns.
    pub exclude: Vec<String>,
}

/// Configuration plus everything the app should warn about: malformed or
/// duplicate entries named, parse failures reported.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoadedConfig {
    pub shares: Vec<ShareConfig>,
    pub filters: FilterSettings,
    pub warnings: Vec<String>,
}

/// Parse one `[[shares]]` table into a [`ShareConfig`].
fn parse_share(entry: &toml::Table) -> Result<ShareConfig, String> {
    let string = |key: &str| -> Result<String, String> {
        match entry.get(key) {
            Some(toml::Value::String(value)) => Ok(value.clone()),
            Some(_) => Err(format!("{key} must be a string")),
            None => Err(format!("{key} is required")),
        }
    };
    let name = string("name")?;
    let server = string("server")?;
    let share = string("share")?;
    let username = string("username")?;
    let domain = match entry.get("domain") {
        None | Some(toml::Value::String(_)) => entry
            .get("domain")
            .and_then(|v| v.as_str())
            .map(String::from),
        Some(_) => return Err("domain must be a string".to_string()),
    };
    let root = match entry.get("root") {
        None => String::new(),
        Some(toml::Value::String(value)) => value.trim_matches('/').to_string(),
        Some(_) => return Err("root must be a string".to_string()),
    };
    let (host, port) = parse_server(&server)?;
    Ok(ShareConfig {
        name,
        host,
        port,
        share,
        username,
        domain,
        root,
    })
}

/// `host` defaults to port 445; `host:port` pins another one.
fn parse_server(server: &str) -> Result<(String, u16), String> {
    match server.rsplit_once(':') {
        Some((host, port)) => {
            let port: u16 = port
                .parse()
                .map_err(|_| format!("bad port in server {server:?}"))?;
            Ok((host.to_string(), port))
        }
        None => Ok((server.to_string(), 445)),
    }
}

/// Build a share definition from the add-share form's six fields, applying the
/// same rules the config file loader applies. Collectable mistakes all come
/// back together, each naming its field: a required field left empty, a
/// `server` that is not `host` or `host:port`, and a display name already used
/// by one of `existing_names`. Whatever builds round-trips through the loader.
pub fn build_share(
    name: &str,
    server: &str,
    share: &str,
    username: &str,
    domain: &str,
    root: &str,
    existing_names: &[String],
) -> Result<ShareConfig, Vec<String>> {
    let mut errors = Vec::new();
    if name.is_empty() {
        errors.push("name is required".to_string());
    }
    if server.is_empty() {
        errors.push("server is required".to_string());
    }
    if share.is_empty() {
        errors.push("share is required".to_string());
    }
    if username.is_empty() {
        errors.push("username is required".to_string());
    }
    if !name.is_empty() && existing_names.iter().any(|taken| taken == name) {
        errors.push(format!("name {name:?} is already configured"));
    }
    // The port parse is only tried once the server is present; a missing
    // server is already on the list.
    let (host, port) = if server.is_empty() {
        (String::new(), 445)
    } else {
        match parse_server(server) {
            Ok(parts) => parts,
            Err(e) => {
                errors.push(e);
                (String::new(), 445)
            }
        }
    };
    let domain = if domain.is_empty() {
        None
    } else {
        Some(domain.to_string())
    };
    if errors.is_empty() {
        Ok(ShareConfig {
            name: name.to_string(),
            host,
            port,
            share: share.to_string(),
            username: username.to_string(),
            domain,
            root: root.trim_matches('/').to_string(),
        })
    } else {
        Err(errors)
    }
}

/// Parse the config contents, continuing past bad entries with warnings.
pub fn parse_contents(contents: &str) -> LoadedConfig {
    let mut loaded = LoadedConfig::default();
    let file: ConfigFile = match toml::from_str(contents) {
        Ok(file) => file,
        Err(e) => {
            loaded.warnings.push(format!("cannot parse config: {e}"));
            return loaded;
        }
    };
    let mut seen = std::collections::HashSet::new();
    for entry in &file.shares {
        match parse_share(entry) {
            Ok(share) => {
                if seen.insert(share.name.clone()) {
                    loaded.shares.push(share);
                } else {
                    loaded.warnings.push(format!(
                        "config: duplicate share name {}: keeping the first",
                        share.name
                    ));
                }
            }
            Err(e) => {
                let display = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|name| !name.is_empty())
                    .map(|name| format!("share {name:?}"))
                    .unwrap_or_else(|| "one share entry".to_string());
                loaded
                    .warnings
                    .push(format!("config: skipping {display}: {e}"));
            }
        }
    }
    if let Some(table) = &file.filters {
        loaded.filters = parse_filters(table, &mut loaded.warnings);
    }
    loaded
}

/// Read the `[filters]` section: the accepted extensions, the include
/// patterns and the exclude patterns. A malformed key is reported and leaves
/// that one setting unset; the section's other keys are still read.
fn parse_filters(table: &toml::Table, warnings: &mut Vec<String>) -> FilterSettings {
    let mut settings = FilterSettings::default();
    if let Some(value) = table.get("extensions") {
        match string_list(value) {
            Ok(list) if !list.is_empty() => settings.extensions = Some(list),
            Ok(_) => {}
            Err(e) => warnings.push(format!("config: filters: extensions {e}")),
        }
    }
    for (key, target) in [
        ("include", &mut settings.include),
        ("exclude", &mut settings.exclude),
    ] {
        if let Some(value) = table.get(key) {
            match string_list(value) {
                Ok(list) => *target = list,
                Err(e) => warnings.push(format!("config: filters: {key} {e}")),
            }
        }
    }
    settings
}

/// The strings of a TOML array, or why the value is not one.
fn string_list(value: &toml::Value) -> Result<Vec<String>, String> {
    let array = value
        .as_array()
        .ok_or_else(|| "must be a list of strings".to_string())?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| "must be a list of strings".to_string())
        })
        .collect()
}

/// The default config path: `<config dir>/dbi-backend/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("dbi-backend").join("config.toml"))
}

/// A configured share's `server` value in the file's dialect: `host` when the
/// port is the SMB default, `host:port` otherwise.
fn server_string(config: &ShareConfig) -> String {
    if config.port == 445 {
        config.host.clone()
    } else {
        format!("{}:{}", config.host, config.port)
    }
}

/// One `[[shares]]` entry: only the keys the loader reads, so a round trip
/// writes exactly what a hand-edited entry of the same share carries.
fn share_value(config: &ShareConfig) -> toml::Value {
    let mut entry = toml::Table::new();
    entry.insert("name".into(), toml::Value::String(config.name.clone()));
    entry.insert("server".into(), toml::Value::String(server_string(config)));
    entry.insert("share".into(), toml::Value::String(config.share.clone()));
    entry.insert(
        "username".into(),
        toml::Value::String(config.username.clone()),
    );
    if let Some(domain) = &config.domain {
        entry.insert("domain".into(), toml::Value::String(domain.clone()));
    }
    if !config.root.is_empty() {
        entry.insert("root".into(), toml::Value::String(config.root.clone()));
    }
    toml::Value::Table(entry)
}

/// Save the configured shares to the config file at `path`: the file's other
/// top-level tables are kept as parsed, the `[[shares]]` array is replaced with
/// `shares`, and the result replaces the file atomically (a temp file in the
/// same directory, renamed over the target). A file that cannot be parsed
/// yields a canonical document built from the list alone.
pub fn save_shares(shares: &[ShareConfig], path: &Path) -> std::io::Result<()> {
    let mut document: toml::Table = std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| toml::from_str(&contents).ok())
        .unwrap_or_default();
    document.insert(
        "shares".into(),
        toml::Value::Array(shares.iter().map(share_value).collect()),
    );
    let contents = toml::to_string(&document)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the config path has no parent directory",
            )
        })?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config.toml".into())
    ));
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path)
}

impl LoadedConfig {
    /// Read a config file at an explicit path, warning when it cannot load.
    fn from_read(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => parse_contents(&contents),
            Err(e) => LoadedConfig {
                shares: Vec::new(),
                filters: FilterSettings::default(),
                warnings: vec![format!("cannot read {}: {e}", path.display())],
            },
        }
    }
}

/// Load the user's configuration. A missing file is no warning — it is the
/// unconfigured state.
pub fn load_user_config() -> LoadedConfig {
    let Some(path) = config_path() else {
        return LoadedConfig::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => parse_contents(&contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LoadedConfig::default(),
        Err(_) => LoadedConfig::from_read(&path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `toml` blocks the README shows, in the order they appear.
    fn readme_toml_blocks() -> Vec<String> {
        let readme =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"))
                .unwrap();
        let mut blocks = Vec::new();
        let mut rest = readme.as_str();
        while let Some(start) = rest.find("```toml") {
            let body = &rest[start + "```toml".len()..];
            let end = body.find("```").unwrap();
            blocks.push(body[..end].to_string());
            rest = &body[end..];
        }
        blocks
    }

    #[test]
    fn the_readme_snippets_parse() {
        // The README's `toml` blocks are documentation of the real format:
        // they must stay valid, warning-free input for the loader.
        let blocks = readme_toml_blocks();
        assert!(blocks.len() >= 2, "the README documents both sections");
        for block in &blocks {
            let loaded = parse_contents(block);
            assert_eq!(
                loaded.warnings,
                Vec::<String>::new(),
                "the block parses without a warning: {block}"
            );
        }
    }

    #[test]
    fn the_readme_share_snippet_parses() {
        // The README's share block is documentation of the real format: it
        // must stay a valid, warning-free input for the loader.
        let blocks = readme_toml_blocks();
        let block = blocks
            .iter()
            .find(|block| block.contains("[[shares]]"))
            .expect("the README documents a share");
        let loaded = parse_contents(block);

        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(loaded.shares.len(), 1);
        let share = &loaded.shares[0];
        assert_eq!(share.name, "nas");
        assert_eq!(share.host, "192.0.2.4");
        assert_eq!(share.port, 445);
        assert_eq!(share.share, "games");
        assert_eq!(share.username, "you");
        assert_eq!(share.domain, None);
        assert_eq!(share.root, "");
    }

    #[test]
    fn the_readme_filters_snippet_parses() {
        let blocks = readme_toml_blocks();
        let block = blocks
            .iter()
            .find(|block| block.contains("[filters]"))
            .expect("the README documents the filters section");
        let loaded = parse_contents(block);

        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(
            loaded.filters.extensions,
            Some(vec!["nsp".to_string(), "nsz".to_string()])
        );
        assert_eq!(loaded.filters.include, vec!["*.nsp".to_string()]);
        assert_eq!(loaded.filters.exclude, vec!["*PATCH*".to_string()]);
    }

    fn config(contents: &str) -> LoadedConfig {
        parse_contents(contents)
    }

    #[test]
    fn parses_a_valid_entry() {
        let loaded = config(
            "[[shares]]\nname = \"Games\"\nserver = \"192.168.0.4\"\nshare = \"games\"\nusername = \"test1\"\n",
        );
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(
            loaded.shares,
            vec![ShareConfig {
                name: "Games".into(),
                host: "192.168.0.4".into(),
                port: 445,
                share: "games".into(),
                username: "test1".into(),
                domain: None,
                root: String::new(),
            }]
        );
    }

    #[test]
    fn parses_host_port_domain_and_root() {
        let loaded = config(
            "[[shares]]\nname = \"Games\"\nserver = \"192.168.0.4:445\"\nshare = \"games\"\nusername = \"test1\"\ndomain = \"WORKGROUP\"\nroot = \"/library/switch/\"\n",
        );
        assert_eq!(loaded.warnings, Vec::<String>::new());
        let share = &loaded.shares[0];
        assert_eq!(share.port, 445);
        assert_eq!(share.domain.as_deref(), Some("WORKGROUP"));
        assert_eq!(share.root, "library/switch");
    }

    #[test]
    fn missing_explicit_file_loads_empty_with_a_warning() {
        let loaded = LoadedConfig::from_read(Path::new("/nonexistent/dbi-backend/config.toml"));
        assert_eq!(loaded.shares, Vec::new());
        assert_eq!(
            loaded.warnings,
            vec!["cannot read /nonexistent/dbi-backend/config.toml: No such file or directory (os error 2)".to_string()],
        );
    }

    #[test]
    fn an_empty_file_is_unconfigured_without_warnings() {
        assert_eq!(config(""), LoadedConfig::default());
    }

    #[test]
    fn a_malformed_entry_warns_and_the_valid_one_survives() {
        let loaded = config(
            "[[shares]]\nserver = \"192.168.0.4\"\nshare = \"games\"\n\n[[shares]]\nname = \"Good\"\nserver = \"host\"\nshare = \"games\"\nusername = \"user\"\n",
        );
        assert_eq!(loaded.shares.len(), 1);
        assert_eq!(loaded.shares[0].name, "Good");
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("skipping"));
        assert!(loaded.warnings[0].contains("name is required"));
    }

    #[test]
    fn a_field_of_the_wrong_type_warns() {
        let loaded = config(
            "[[shares]]\nname = 3\nserver = \"192.168.0.4\"\nshare = \"games\"\nusername = \"u\"\n",
        );
        assert!(loaded.shares.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("name must be a string"));
    }

    #[test]
    fn a_bad_port_warns() {
        let loaded = config(
            "[[shares]]\nname = \"A\"\nserver = \"host:bogus\"\nshare = \"s\"\nusername = \"u\"\n",
        );
        assert!(loaded.shares.is_empty());
        assert!(loaded.warnings[0].contains("bad port"));
    }

    #[test]
    fn duplicate_names_keep_the_first_and_warn() {
        let loaded = config(
            "[[shares]]\nname = \"Games\"\nserver = \"192.168.0.4\"\nshare = \"games\"\nusername = \"test1\"\n\n[[shares]]\nname = \"Games\"\nserver = \"other\"\nshare = \"other\"\nusername = \"u\"\n",
        );
        assert_eq!(loaded.shares.len(), 1);
        assert_eq!(loaded.shares[0].host, "192.168.0.4");
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("duplicate"));
    }

    // --- the add-share form's build-and-validate entry point ---------------

    fn no_names() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn build_share_accepts_a_full_entry() {
        let built = build_share(
            "Backups",
            "192.0.2.9:9445",
            "archives",
            "me",
            "WORKGROUP",
            "/library/switch/",
            &no_names(),
        );
        assert_eq!(
            built,
            Ok(ShareConfig {
                name: "Backups".into(),
                host: "192.0.2.9".into(),
                port: 9445,
                share: "archives".into(),
                username: "me".into(),
                domain: Some("WORKGROUP".into()),
                root: "library/switch".into(),
            })
        );
    }

    fn share_names_in_order(shares: &[ShareConfig]) -> Vec<String> {
        shares.iter().map(|share| share.name.clone()).collect()
    }

    #[test]
    fn build_share_reports_every_missing_required_field() {
        let built = build_share("", "", "", "", "", "", &no_names());
        let errors = built.err().unwrap();
        for required in ["name", "server", "share", "username"] {
            assert!(
                errors.iter().any(|error| error.contains(required)),
                "the missing {required} is named: {errors:?}"
            );
        }

        // One clearing later, the rest still refuse to submit.
        let errors = build_share("Late", "", "", "", "", "", &no_names())
            .err()
            .unwrap();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn build_share_rejects_a_bad_server() {
        let built = build_share("Late", "host:bogus", "s", "u", "", "", &no_names());
        let errors = built.err().unwrap();
        assert!(
            errors.iter().any(|error| error.contains("bad port")),
            "the bad server is named: {errors:?}"
        );
    }

    #[test]
    fn build_share_rejects_a_duplicate_name() {
        let built = build_share(
            "Games",
            "192.0.2.4",
            "games",
            "u",
            "",
            "",
            &["Games".to_string(), "Other".to_string()],
        );
        let errors = built.err().unwrap();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Games") && error.contains("already configured")),
            "the taken name is reported: {errors:?}"
        );
    }

    #[test]
    fn build_share_round_trips_through_the_loader() {
        // Whatever the form builds must satisfy the same loader the file feeds:
        // one parse of the built values in TOML form equals the built config.
        let built = build_share(
            "Backups",
            "192.0.2.9",
            "archives",
            "me",
            "",
            "library/switch",
            &no_names(),
        )
        .unwrap();
        let contents = format!(
            "[[shares]]\nname = {:?}\nserver = {:?}\nshare = {:?}\nusername = {:?}\nroot = {:?}\n",
            built.name, built.host, built.share, built.username, built.root,
        );
        let loaded = parse_contents(&contents);
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(share_names_in_order(&loaded.shares), vec!["Backups"]);
        assert_eq!(loaded.shares[0], built);
    }

    fn share(name: &str) -> ShareConfig {
        ShareConfig {
            name: name.to_string(),
            host: "192.0.2.4".to_string(),
            port: 445,
            share: "games".to_string(),
            username: "test1".to_string(),
            domain: None,
            root: String::new(),
        }
    }

    // --- saving the share list back into the config file -------------------

    fn saved_contents(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn saving_adds_the_entry_without_a_password_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[note]\nwords = \"hand-edited\"\
             \n\n[[shares]]\
             \nname = \"Old\"\
             \nserver = \"192.0.2.1\"\
             \nshare = \"games\"\
             \nusername = \"you\"\n",
        )
        .unwrap();

        let new_share = ShareConfig {
            name: "Late".to_string(),
            host: "nas.example".to_string(),
            port: 9445,
            share: "backup".to_string(),
            username: "me".to_string(),
            domain: Some("WORKGROUP".to_string()),
            root: "library/switch".to_string(),
        };
        save_shares(&[share("Old"), new_share.clone()], &path).unwrap();

        let contents = saved_contents(&path);
        // The file the loader reads next holds both shares and no password.
        let loaded = parse_contents(&contents);
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(share_names_in_order(&loaded.shares), vec!["Old", "Late"]);
        assert_eq!(loaded.shares[1], new_share);
        // The unrelated section is untouched.
        assert!(contents.contains("[note]"));
        assert!(contents.contains("hand-edited"));
    }

    #[test]
    fn saving_writes_canonical_server_and_omits_empty_optionals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        save_shares(&[share("Nas")], &path).unwrap();

        let contents = saved_contents(&path);
        assert!(
            contents.contains("server = \"192.0.2.4\""),
            "port 445 stays bare: {contents}"
        );
        assert!(!contents.contains("domain"));
        assert!(!contents.contains("root"));
        assert!(
            !contents.to_lowercase().contains("password"),
            "the saved file must not mention passwords: {contents}"
        );
    }

    #[test]
    fn a_save_over_an_unparseable_file_writes_the_canonical_document() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "= not toml at all =").unwrap();

        save_shares(&[share("Nas")], &path).unwrap();

        let loaded = parse_contents(&saved_contents(&path));
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(share_names_in_order(&loaded.shares), vec!["Nas"]);
        assert_eq!(loaded.shares[0], share("Nas"));
    }

    #[test]
    fn a_save_keeps_the_filters_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[filters]\nextensions = [\"nsp\"]\ninclude = [\"*.nsp\"]\n",
        )
        .unwrap();

        save_shares(&[share("Nas")], &path).unwrap();

        let loaded = parse_contents(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(loaded.filters.extensions, Some(vec!["nsp".to_string()]));
        assert_eq!(loaded.filters.include, vec!["*.nsp".to_string()]);
        assert_eq!(share_names_in_order(&loaded.shares), vec!["Nas"]);
    }

    #[test]
    fn a_failed_save_leaves_the_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original =
            "[[shares]]\nname = \"Old\"\nserver = \"s\"\nshare = \"g\"\nusername = \"u\"\n";
        std::fs::write(&path, original).unwrap();

        // A plain file where the save needs a directory: creation refuses, so
        // nothing can be written, and the reachable file is untouched.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let unreachable = blocker.join("config.toml");
        assert!(save_shares(&[share("Nas")], &unreachable).is_err());

        // The reachable file still holds exactly what it held before.
        assert_eq!(saved_contents(&path), original);
        assert!(
            std::fs::read_to_string(&unreachable).is_err(),
            "no half-written file appears"
        );
    }

    #[test]
    fn reads_the_filters_section() {
        let loaded = config(
            "[filters]\nextensions = [\"nsp\", \"nsz\"]\ninclude = [\"*.nsp\"]\nexclude = [\"*PATCH*\"]\n",
        );
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(
            loaded.filters.extensions,
            Some(vec!["nsp".to_string(), "nsz".to_string()])
        );
        assert_eq!(loaded.filters.include, vec!["*.nsp".to_string()]);
        assert_eq!(loaded.filters.exclude, vec!["*PATCH*".to_string()]);
    }

    #[test]
    fn a_malformed_filters_entry_warns_and_keeps_the_rest() {
        let loaded = config(
            "[[shares]]\nname = \"nas\"\nserver = \"192.0.2.4\"\nshare = \"games\"\nusername = \"you\"\n\n\
             [filters]\nextensions = \"nsp\"\ninclude = [\"*.nsp\"]\n",
        );
        assert_eq!(loaded.shares.len(), 1, "the share is still listed");
        assert_eq!(loaded.filters.extensions, None, "the bad setting is unset");
        assert_eq!(
            loaded.filters.include,
            vec!["*.nsp".to_string()],
            "the section's other keys are still read"
        );
        assert_eq!(loaded.warnings.len(), 1, "the malformed entry is reported");
    }

    #[test]
    fn a_file_without_the_filters_section_asks_for_nothing() {
        let loaded = config(
            "[[shares]]\nname = \"nas\"\nserver = \"192.0.2.4\"\nshare = \"games\"\nusername = \"you\"\n",
        );
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(loaded.filters.extensions, None);
        assert!(loaded.filters.include.is_empty());
        assert!(loaded.filters.exclude.is_empty());
    }

    #[test]
    fn an_empty_filters_section_asks_for_nothing() {
        let loaded = config("[filters]\nextensions = []\n");
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_eq!(loaded.filters.extensions, None);
    }
}
