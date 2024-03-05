use cold::{link::link, opt::parse_opts};
use log::info;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    info!("Launched with args: {:?}", args);

    // parse arguments
    let opt = parse_opts(&args)?;

    info!("Parsed options: {opt:?}");

    link(&opt)?;
    Ok(())
}
