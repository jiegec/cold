use cold::{link::link, opt::parse_opts};
use tracing::info;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    info!("Launched with args: {:?}", args);

    // parse arguments
    let opt = parse_opts(&args)?;

    info!("Parsed options: {opt:?}");

    link(&opt)?;
    Ok(())
}
