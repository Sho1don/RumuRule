//! Dump a TUN configuration for a stored profile, useful for debugging.
//!
//! ```sh
//! cargo run -p rumurule-core --example dump_tun -- ~/.config/slop.rumurule/profiles.json 2
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: dump_tun <profiles.json> <index>");
        std::process::exit(2);
    };
    let index: usize = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let store = match rumurule_core::Store::load(std::path::Path::new(path)) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            std::process::exit(1);
        }
    };
    let Some(entry) = store.entries.get(index) else {
        eprintln!("no entry at index {index}");
        std::process::exit(1);
    };

    let base = match entry.config.clone() {
        Some(config) => config,
        None => match rumurule_core::parse_link(&entry.link) {
            Ok(profile) => rumurule_core::config::build_config(&profile),
            Err(error) => {
                eprintln!("parse error: {error}");
                std::process::exit(1);
            }
        },
    };

    let config = rumurule_core::config::tun_config(&base, true);
    println!("{}", serde_json::to_string_pretty(&config).expect("serialize"));
}
