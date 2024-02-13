use cold::{link, parse_opts};
use log::info;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = std::env::args().collect::<Vec<_>>();
    info!("launched with args: {:?}", args);

    // parse arguments
    let opt = parse_opts(&args)?;

    info!("parsed options: {opt:?}");

    link(&opt)?;
    Ok(())
}
