use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use clap::{App, Arg, ArgGroup};
use glob::Pattern;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ManifestMode {
    TimestampTest,
    Hash,
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
    source: Option<PathBuf>,
    target: Option<PathBuf>,
    verbose: bool,
    hash: HashSettings,
    manifest_path: Option<PathBuf>,
    server_port: Option<u16>
}

impl HashSettings {
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

impl Configuration {
    #[inline]
    pub fn server_port(&self) -> u16 {
        self.server_port.unwrap()
    }

    #[inline]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path.as_ref().unwrap()
    }

    #[inline]
    pub fn target(&self) -> &Path {
        &self.target.as_ref().unwrap()
    }

    #[inline]
    pub fn role(&self) -> Option<ProcessRole> {
        self.role
    }

    #[inline]
    pub fn source(&self) -> &Path {
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
    let source = args.value_of("source").map(PathBuf::from);
    let target = args.value_of("target").map(PathBuf::from);
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
            verbose: role.is_none() && args.is_present("verbose"),
            manifest_path: args.value_of("manifest file").map(PathBuf::from),
            role,
            server_port
        })
}
