use clap::{Arg, App};
use crate::config::ManifestMode::{Rehash, TimestampTest};

#[derive(Debug, Copy, Clone)]
pub enum ManifestMode {
    AssumeValid,
    TimestampTest,
    Rehash,
    NoManifest,
}

#[derive(Debug)]
pub struct Arguments {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) verbose: bool,
    pub(crate) manifest_path: String,
    pub(crate) manifest_mode: ManifestMode,
}

pub fn configure() -> Arguments {
    let args = App::new("usync")
        .version("1.0")
        .author("Elisabeth 'TerraNova' Schulz")
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
            Arg::with_name("manifest-file")
                .long("manifest-file")
                .takes_value(true)
                .default_value("./.usync.manifest")
        )
        .arg(
            Arg::with_name("no-manifest")
                .help("Do not use a manifest, re-hash entire tree")
                .long("no-manifest")
                .takes_value(false)
        )
        .arg(
            Arg::with_name("verbose")
                .help("Verbose output")
                .long("verbose")
                .short("v")
                .takes_value(false)
        )
        .get_matches();

    Arguments {
        source: args.value_of("source").unwrap().to_string(),
        target: args.value_of("target").unwrap().to_string(),
        verbose: args.is_present("verbose"),
        manifest_path: args.value_of("manifest-file").unwrap().to_string(),
        manifest_mode: if args.is_present("no-manifest") { ManifestMode::NoManifest} else { ManifestMode::TimestampTest }
    }
}
