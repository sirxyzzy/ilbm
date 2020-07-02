#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;
// use std::io::prelude::*;

use std::fs::File;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("starting up");

    for argument in env::args().skip(1) {
        info!("Trying to read {}", argument);
        let image = ilbm::read_from_file( File::open(argument)?)?;
        info!("Read an image, in memory size {}", image.pixels.len() * 3);
    }

    Ok(())
}