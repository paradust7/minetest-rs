mod proxy;

use anyhow::bail;
use clap::ArgGroup;
use clap::Parser;
use minetest_protocol::audit_on;
use proxy::MinetestProxy;
use std::net::SocketAddr;
use std::time::Duration;

/// mtshark - Minetest proxy that gives detailed inspection of protocol
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(ArgGroup::new("source").required(true).args(["listen", "bind"])))]
struct Args {
    /// Listen on port
    #[arg(group = "source", short, long)]
    listen: Option<u16>,

    /// Listen with specific bind address (ip:port)
    #[arg(group = "source", short, long)]
    bind: Option<SocketAddr>,

    /// Target server (address:port)
    #[arg(short, long, required = true)]
    target: SocketAddr,

    /// Verbosity level (up to -vvv)
    #[arg(short, long, default_value_t = 0, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Enable audit mode
    #[arg(short, long, default_value_t = false)]
    audit: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // tokio::main makes rust-analyzer fragile,
    // so put the code in a separate place.
    real_main().await
}

async fn real_main() -> anyhow::Result<()> {
    std::env::set_var("RUST_BACKTRACE", "1");

    let args = Args::parse();

    if args.audit {
        audit_on();
        println!("Auditing is ON.");
        println!("Proxy will terminate if an invalid packet is received,");
        println!("or if serialization/deserialization do not match exactly.");
    }

    let bind_addr: SocketAddr = if let Some(listen_port) = args.listen {
        if args.target.is_ipv4() {
            format!("0.0.0.0:{}", listen_port).parse()?
        } else {
            format!("[::]:{}", listen_port).parse()?
        }
    } else if let Some(bind_addr) = args.bind {
        bind_addr
    } else {
        bail!("One of --listen or --bind must be specified");
    };

    let _proxy = MinetestProxy::new(bind_addr, args.target, args.verbose);
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
