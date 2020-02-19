use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use clap::{App, Arg, ArgGroup};
use glob::Pattern;
use crate::config::ManifestMode::TimestampTest;
use std::fmt::Display;
use serde::export::Formatter;
use crate::config::PathDefinition::{Remote, Local, Server};

#[derive(Debug,Clone,PartialEq,Eq)]
pub enum PathDefinition {
    Local(PathBuf),
    Server(String),
    Remote(String, String)
}

impl Display for PathDefinition {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            Local(pb) => {
                f.write_str(&format!("Local({})", pb.to_string_lossy()))
            },
            Server(s) => {
                f.write_str(&format!("Server({})", s))
            },
            Remote(host, path) => {
                f.write_str(&format!("Remote(host={},path={})", host, path))
            },
        }
    }
}

#[cfg(test)]
mod test_paths {
    use super::*;

    #[test]
    fn parse_local() {
        let path = PathDefinition::parse("/a/local/path");
        assert_eq!(Local(PathBuf::from("/a/local/path")), path)
    }

    #[test]
    fn parse_remote() {
        let path = PathDefinition::parse("remote://user@a.host.name:remote/path");
        assert_eq!(Remote("user@a.host.name".to_owned(), "remote/path".to_owned()), path);
    }

    #[test]
    fn parse_server() {
        let path = PathDefinition::parse("server://server.name:1991");
        assert_eq!(Server("server.name:1991".to_owned()), path);
    }
}

impl PathDefinition {
    fn parse(string: &str) -> PathDefinition {
        if string.starts_with("remote://") {
            let src = &string[9..];
            let path_sep = src.find(":").unwrap();
            let remote = &src[..path_sep];
            let remote_path = &src[path_sep +1 ..];
            Remote(String::from(remote), String::from(remote_path))
        } else if string.starts_with("server://") {
            Server(String::from(&string[9..]))
        } else {
            Local(PathBuf::from(string))
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ManifestMode {
    TimestampTest,
    Hash,
}

impl Display for ManifestMode {

    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let str = match self {
                TimestampTest => "timestamp",
                ManifestMode::Hash => "hash",
            };
        f.write_str(str)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ProcessRole {
    Sender,
    Receiver,
    Server
}

#[derive(Debug, Clone)]
pub struct HashSettings {
    force_rebuild: bool,
    mode: ManifestMode,
    exclude_patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
pub struct Configuration {
    role: Option<ProcessRole>,
    source: Option<PathDefinition>,
    target: Option<PathDefinition>,
    verbose: bool,
    hash: HashSettings,
    manifest_path: Option<PathBuf>,
    server_port: Option<u16>,
    force_pipeline: bool
}

impl HashSettings {

    #[inline]
    pub fn exclude_patterns(&self) -> &Vec<Pattern> {
        &self.exclude_patterns
    }

    #[inline]
    pub fn force_rebuild(&self) -> bool {
        self.force_rebuild
    }
    #[inline]
    pub fn manifest_mode(&self) -> ManifestMode {
        self.mode
    }

    pub fn is_excluded(&self, str: &Path) -> bool {
        for pattern in &self.exclude_patterns {
            if pattern.matches_path(str) {
                return true;
            }
        }

        false
    }

    pub fn with_additional_exclusion(&self, exclude: &Path) -> HashSettings {
        let mut copy = self.clone();
        let pattern = Pattern::new(exclude.to_string_lossy().as_ref()).unwrap();
        copy.exclude_patterns.push(pattern);

        copy
    }
}

#[cfg(test)]
mod test_excludes {
    use super::*;
    use glob::PatternError;

    #[test]
    fn apply_excludes() -> Result<(), PatternError>{
        let settings = HashSettings{
            force_rebuild: false,
            mode: ManifestMode::TimestampTest,
            exclude_patterns: vec![Pattern::new("ab*ca")?]
        };

        assert_eq!(settings.is_excluded(&PathBuf::from("abnahfpaclca")), true);
        assert_eq!(settings.is_excluded(&PathBuf::from("anotherfile.txt")), false);

        Ok(())
    }

    #[test]
    fn apply_excludes_with_additional() -> Result<(), PatternError>{
        let settings = HashSettings{
            force_rebuild: false,
            mode: ManifestMode::TimestampTest,
            exclude_patterns: vec![Pattern::new("ab*ca")?]
        }.with_additional_exclusion(&PathBuf::from("anotherfile.txt"));

        assert_eq!(settings.is_excluded(&PathBuf::from("abnahfpaclca")), true);
        assert_eq!(settings.is_excluded(&PathBuf::from("anotherfile.txt")), true);

        Ok(())
    }
}

impl Configuration {
    #[inline]
    pub fn force_pipeline(&self) -> bool {
        self.force_pipeline
    }

    #[inline]
    pub fn server_port(&self) -> u16 {
        self.server_port.unwrap()
    }

    #[inline]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path.as_ref().unwrap()
    }

    #[inline]
    pub fn role(&self) -> Option<ProcessRole> {
        self.role
    }


    #[inline]
    pub fn target(&self) -> &PathDefinition {
        &self.target.as_ref().unwrap()
    }

    #[inline]
    pub fn source(&self) -> &PathDefinition {
        &self.source.as_ref().unwrap()
    }

    pub fn hash_settings(&self) -> &HashSettings {
        &self.hash
    }

    #[inline]
    pub fn verbose(&self) -> bool {
        self.verbose
    }
}


pub fn configure() -> Result<Configuration, Error> {
    let args = App::new("usync")
        .version("1.0")
        .author("Elisabeth 'TerraNova' Schulz")
        .arg(Arg::with_name("force-pipeline")
            .hidden(true)
            .long("force-pipeline")
        )
        .arg(
            Arg::with_name("rebuild manifest")
                .help("rebuild the required manifest(s), even if it already exists")
                .long("force-rebuild-manifest")
        )
        .arg(
            Arg::with_name("role")
                .help("Role of a remote-spawned instance.")
                .long("role")
                .takes_value(true)
                .possible_values(&["sender", "receiver", "server"])
        )
        .arg(
            Arg::with_name("source")
                .help("Sync source directory")
                .long("source")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("target")
                .help("Sync target directory")
                .long("target")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("manifest file")
                .long("manifest-file")
                .help("Stored manifest file (relative to source directory)")
                .takes_value(true)
                .default_value(".usync.manifest")
        )
        .arg(Arg::with_name("hash-mode")
            .help("hashing mode")
            .long("hash-mode")
            .takes_value(true)
            .default_value("hash")
            .possible_values(&["hash", "timestamp"])
        )
        .arg(
            Arg::with_name("verbose")
                .help("Verbose output")
                .long("verbose")
                .short("v")
                .takes_value(false)
        )
        .group(ArgGroup::with_name("server")
            .arg("server-port")
        )
        .arg(Arg::with_name("server-port")
            .help("Port for the server to listen on")
            .long("server-port")
            .takes_value(true)
            .default_value("9715")
        )
        .arg(
            Arg::with_name("exclude")
                .help("exclude glob (specify multiple times for several patterns")
                .multiple(true)
                .number_of_values(1)
                .long("exclude")
                .takes_value(true)
        )
        .get_matches();
    let source = args.value_of("source").map(PathDefinition::parse);
    let target = args.value_of("target").map(PathDefinition::parse);
    let server_port = args.value_of("server-port").and_then(|v| {
        return match v.parse::<u16>() {
            Ok(v) => Some(v),
            Err(_) => None
        }
    });

    let mut exclude_patterns = Vec::new();

    if args.values_of("exclude").is_some() {
        for pattern in args.values_of("exclude").unwrap() {
            exclude_patterns.push(Pattern::new(pattern).map_err(|pe| Error::new(ErrorKind::Other, pe))?)
        }
    }
    let role = args.value_of("role");
    let role = match role {
        Some("sender") => Some(ProcessRole::Sender),
        Some("receiver") => Some(ProcessRole::Receiver),
        Some("server") => Some(ProcessRole::Server),
        _ => None
    };


    Ok(Configuration {
            hash: HashSettings {
                force_rebuild: args.is_present("rebuild manifest"),
                mode: if args.value_of("hash-mode").unwrap() == "hash" {
                    ManifestMode::Hash
                } else {
                    ManifestMode::TimestampTest
                },
                exclude_patterns,
            },
            source,
            target,
            verbose: (role == Some(ProcessRole::Server) || role.is_none()) && args.is_present("verbose"),
            manifest_path: args.value_of("manifest file").map(PathBuf::from),
            role,
            server_port,
            force_pipeline: args.is_present("force-pipeline")
        })
}
