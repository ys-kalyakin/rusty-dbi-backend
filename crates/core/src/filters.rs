//! Which files a directory scan accepts.

use globset::{GlobBuilder, GlobMatcher};
use std::collections::BTreeSet;
use std::path::Path;

/// The extensions a scan accepts when nothing else is configured.
pub const DEFAULT_EXTENSIONS: [&str; 2] = ["nsp", "nsz"];

/// A pattern that does not compile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid pattern `{pattern}`: {message}")]
pub struct PatternError {
    /// The pattern as it was written.
    pub pattern: String,
    /// What the matcher said about it.
    pub message: String,
}

/// The rules that decide which files a directory scan accepts.
#[derive(Debug, Clone)]
pub struct Filters {
    /// The accepted extensions, without the dot, lowercased.
    extensions: BTreeSet<String>,
    /// The include patterns: a file must match one of them when any exist.
    include: Vec<Pattern>,
    /// The exclude patterns: a file matching one of them is skipped.
    exclude: Vec<Pattern>,
}

/// One compiled pattern, kept together with the text it was written as.
#[derive(Debug, Clone)]
struct Pattern {
    text: String,
    matcher: GlobMatcher,
}

impl Default for Filters {
    /// The default rules: the `nsp` and `nsz` extensions and no patterns.
    fn default() -> Self {
        Self {
            extensions: default_extensions(),
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

impl Filters {
    /// The default rules.
    pub fn new() -> Self {
        Self::default()
    }

    /// Rules with the given accepted extensions. An empty set means the
    /// default set: a half-written configuration must not silently empty the
    /// offered list.
    pub fn with_extensions<I, S>(extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = BTreeSet::new();
        for extension in extensions {
            let extension = normalize_extension(extension.as_ref());
            if !extension.is_empty() {
                set.insert(extension);
            }
        }
        if set.is_empty() {
            set = default_extensions();
        }
        Self {
            extensions: set,
            ..Self::default()
        }
    }

    /// Add an include pattern. A file that matches no include pattern is
    /// skipped once any include pattern exists.
    pub fn add_include(&mut self, pattern: &str) -> Result<(), PatternError> {
        let compiled = Pattern::compile(pattern)?;
        self.include.push(compiled);
        Ok(())
    }

    /// Add an exclude pattern. A file matching it is skipped.
    pub fn add_exclude(&mut self, pattern: &str) -> Result<(), PatternError> {
        let compiled = Pattern::compile(pattern)?;
        self.exclude.push(compiled);
        Ok(())
    }

    /// The accepted extensions.
    pub fn extensions(&self) -> impl Iterator<Item = &str> {
        self.extensions.iter().map(String::as_str)
    }

    /// The include patterns, as they were written.
    pub fn include_patterns(&self) -> Vec<&str> {
        self.include.iter().map(|p| p.text.as_str()).collect()
    }

    /// The exclude patterns, as they were written.
    pub fn exclude_patterns(&self) -> Vec<&str> {
        self.exclude.iter().map(|p| p.text.as_str()).collect()
    }

    /// Whether a file with this name is accepted by a scan.
    pub fn accepts(&self, name: &str) -> bool {
        self.accepts_extension(name) && self.accepts_patterns(name)
    }

    /// Whether the file's extension is in the accepted set.
    pub fn accepts_extension(&self, name: &str) -> bool {
        match extension_of(name) {
            Some(extension) => self.extensions.contains(&extension),
            None => false,
        }
    }

    /// Whether the name passes the include and exclude patterns.
    pub fn accepts_patterns(&self, name: &str) -> bool {
        if self.exclude.iter().any(|p| p.matcher.is_match(name)) {
            return false;
        }
        if self.include.is_empty() {
            return true;
        }
        self.include.iter().any(|p| p.matcher.is_match(name))
    }
}

impl Pattern {
    /// Compile a pattern, matching the file name case-insensitively.
    fn compile(pattern: &str) -> Result<Self, PatternError> {
        let glob = GlobBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| PatternError {
                pattern: pattern.to_string(),
                message: e.to_string(),
            })?;
        Ok(Self {
            text: pattern.to_string(),
            matcher: glob.compile_matcher(),
        })
    }
}

/// The default accepted extension set.
fn default_extensions() -> BTreeSet<String> {
    DEFAULT_EXTENSIONS.iter().map(|e| e.to_string()).collect()
}

/// An extension as it is stored: lowercased, without the leading dot.
fn normalize_extension(extension: &str) -> String {
    extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

/// The lowercased extension of a file name, if it has one.
fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_set_accepts_nsp_and_nsz() {
        let filters = Filters::new();
        assert!(filters.accepts("game.nsp"));
        assert!(filters.accepts("game.nsz"));
        assert!(!filters.accepts("notes.txt"));
    }

    #[test]
    fn a_configured_set_replaces_the_default() {
        let filters = Filters::with_extensions(["xci", "nsp"]);
        assert!(filters.accepts("game.xci"));
        assert!(filters.accepts("game.nsp"));
        assert!(!filters.accepts("game.nsz"));
    }

    #[test]
    fn an_empty_set_falls_back_to_the_default() {
        let filters = Filters::with_extensions(Vec::<String>::new());
        assert!(filters.accepts("game.nsp"));
        assert!(filters.accepts("game.nsz"));
        assert!(!filters.accepts("notes.txt"));
    }

    #[test]
    fn the_extension_is_matched_case_insensitively() {
        let filters = Filters::new();
        assert!(filters.accepts("GAME.NSP"));
        assert!(filters.accepts("Patch.NsZ"));
    }

    #[test]
    fn an_extension_may_carry_a_leading_dot() {
        let filters = Filters::with_extensions([".XCI"]);
        assert!(filters.accepts("game.xci"));
        assert!(!filters.accepts("game.nsp"));
    }

    #[test]
    fn a_file_without_an_extension_is_rejected() {
        let filters = Filters::new();
        assert!(!filters.accepts("README"));
        assert!(!filters.accepts(".hidden"));
    }

    #[test]
    fn an_excluded_name_is_rejected() {
        let mut filters = Filters::new();
        filters.add_exclude("*PATCH*").unwrap();
        assert!(!filters.accepts("Game [PATCH].nsp"));
        assert!(filters.accepts("Game.nsp"));
    }

    #[test]
    fn a_bracket_is_a_character_class() {
        let mut filters = Filters::new();
        filters.add_exclude("*[PATCH]*").unwrap();
        // The bracket introduces a class of characters, so the pattern matches
        // any name holding one of them.
        assert!(!filters.accepts("Game [PATCH].nsp"));
        assert!(!filters.accepts("Game.nsp"));
        // A name holding none of the class' characters is kept.
        assert!(filters.accepts("Sky.nsz"));
    }

    #[test]
    fn a_name_matching_no_include_pattern_is_rejected() {
        let mut filters = Filters::new();
        filters.add_include("*.nsp").unwrap();
        assert!(filters.accepts("Game.nsp"));
        assert!(!filters.accepts("Game.nsz"));
    }

    #[test]
    fn no_include_pattern_keeps_every_file() {
        let filters = Filters::new();
        assert!(filters.accepts("Game.nsp"));
        assert!(filters.accepts("Game.nsz"));
    }

    #[test]
    fn a_pattern_matches_nothing_when_it_matches_nothing() {
        let mut filters = Filters::new();
        filters.add_exclude("*.txt").unwrap();
        assert!(filters.accepts("Game.nsp"));
    }

    #[test]
    fn a_question_mark_matches_one_character() {
        let mut filters = Filters::new();
        filters.add_exclude("Game?.nsp").unwrap();
        assert!(!filters.accepts("Game1.nsp"));
        assert!(filters.accepts("Game12.nsp"));
    }

    #[test]
    fn a_character_class_matches_one_of_its_characters() {
        let mut filters = Filters::new();
        filters.add_exclude("Game[12].nsp").unwrap();
        assert!(!filters.accepts("Game1.nsp"));
        assert!(!filters.accepts("Game2.nsp"));
        assert!(filters.accepts("Game3.nsp"));
    }

    #[test]
    fn a_pattern_is_matched_against_the_file_name_only() {
        let mut filters = Filters::new();
        filters.add_include("*/games/*").unwrap();
        assert!(!filters.accepts("Game.nsp"));
    }

    #[test]
    fn a_pattern_ignores_case() {
        let mut filters = Filters::new();
        filters.add_exclude("*[patch]*").unwrap();
        assert!(!filters.accepts("Game [PATCH].nsp"));
    }

    #[test]
    fn an_invalid_pattern_is_reported() {
        let mut filters = Filters::new();
        let error = filters.add_exclude("Game[").unwrap_err();
        assert_eq!(error.pattern, "Game[");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn the_patterns_are_kept_as_written() {
        let mut filters = Filters::new();
        filters.add_include("*.nsp").unwrap();
        filters.add_exclude("*[PATCH]*").unwrap();
        assert_eq!(filters.include_patterns(), vec!["*.nsp"]);
        assert_eq!(filters.exclude_patterns(), vec!["*[PATCH]*"]);
    }

    #[test]
    fn the_accepted_extensions_are_reported() {
        let filters = Filters::with_extensions(["xci", "nsp"]);
        let mut extensions: Vec<&str> = filters.extensions().collect();
        extensions.sort_unstable();
        assert_eq!(extensions, vec!["nsp", "xci"]);
    }
}
