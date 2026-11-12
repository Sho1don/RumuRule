//! Fetch a subscription URL and print the imported servers.
//!
//! ```sh
//! cargo run -p rumurule-core --example fetch_sub -- https://example.com/sub
//! ```

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Some(url) = std::env::args().nth(1) else {
        eprintln!("usage: fetch_sub <url>");
        std::process::exit(2);
    };

    let fetched = match rumurule_core::fetch(&url, None).await {
        Ok(fetched) => fetched,
        Err(error) => {
            eprintln!("fetch error: {error}");
            std::process::exit(1);
        }
    };

    if let Some(title) = &fetched.title {
        println!("subscription: {title}");
    }

    let imported = rumurule_core::parse_subscription(&fetched.body);
    if imported.is_empty() {
        eprintln!("no supported links found");
        std::process::exit(1);
    }

    for item in imported {
        println!(
            "{:<16} {:<12} {}:{}",
            item.profile.name, item.profile.protocol, item.profile.server, item.profile.port
        );
    }
}
