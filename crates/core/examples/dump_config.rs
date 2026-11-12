//! Dump an Xray configuration for a share link, useful for debugging.
//!
//! ```sh
//! cargo run -p rumurule-core --example dump_config -- 'vless://...'
//! ```

fn main() {
    let Some(link) = std::env::args().nth(1) else {
        eprintln!("usage: dump_config <share-link>");
        std::process::exit(2);
    };

    let profile = match rumurule_core::parse_link(&link) {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("parse error: {error}");
            std::process::exit(1);
        }
    };

    let config = rumurule_core::config::build_config(&profile);
    println!("{}", serde_json::to_string_pretty(&config).expect("serialize"));
}
