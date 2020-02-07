use clap::{App, Arg, Values};
use std::path::{PathBuf, Path};
use std::io::{Error, ErrorKind};
use glob::Pattern;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ManifestMode {
    TimestampTest,
    Hash,
}

#[derive(Debug,Clone)]
pub struct Configuration {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) verbose: bool,
    pub(crate) force_rebuild_manifest: bool,
    pub(crate) manifest_path: PathBuf,
    pub(crate) manifest_mode: ManifestMode,
    exclude_patterns: Vec<Pattern>
}

impl Configuration {
    pub fn with_additional_exclusion(&self, exclude: &Path) -> Configuration {
        let mut copy = self.clone();
        let pattern = Pattern::new(exclude.to_string_lossy().as_ref()).unwrap();
        copy.exclude_patterns.push(pattern);

        copy
    }

    pub fn is_excluded(&self, str: &Path) -> bool {
        for pattern in &self.exclude_patterns {
            if pattern.matches_path(str) {
                return true;
            }
        }

        false
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
            Arg::with_name("source")
                .help("Sync source directory")
                .long("source")
                .takes_value(true)
                .required(true)
        )
        .arg(
            Arg::with_name("target")
                .help("Sync target directory")
                .long("target")
                .takes_value(true)
                .required(true)
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
        .arg(
            Arg::with_name("exclude")
                .help("exclude glob (specify multiple times for several patterns")
                .multiple(true)
                .number_of_values(1)
                .long("exclude")
                .takes_value(true)
        )
        .get_matches();
    let src = Path::new(args.value_of("source").unwrap());
    let trg = Path::new(args.value_of("target").unwrap());

    let mut exclude_patterns = Vec::new();

    if args.values_of("exclude").is_some() {
        for pattern in args.values_of("exclude").unwrap() {
            exclude_patterns.push(Pattern::new(pattern).map_err(|pe| Error::new(ErrorKind::Other, pe))?)
        }
    }


    if !src.exists() {
        Err(Error::new(ErrorKind::NotFound, "Source path not available"))
    } else if !trg.exists() {
        Err(Error::new(ErrorKind::NotFound, "Target path not available"))
    } else {
        Ok(Configuration {
            source: src.into(),
            target: trg.into(),
            verbose: args.is_present("verbose"),
            force_rebuild_manifest: args.is_present("rebuild manifest"),
            manifest_path: Path::new(args.value_of("manifest file").unwrap()).into(),
            manifest_mode:  if args.value_of("hash-mode").unwrap() == "hash" {
                ManifestMode::Hash
            } else {
                ManifestMode::TimestampTest
            },
            exclude_patterns
        })
    }

}
