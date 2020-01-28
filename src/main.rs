

mod config;
mod tree;


fn main() {
    let cfg = config::configure();
    let hash = tree::FileEntry::new(cfg.source.as_str());
    println!("Hello, world! You called me with {:#?}", cfg);
    println!("Hashing {}: {:?}", cfg.source, hash);
}
