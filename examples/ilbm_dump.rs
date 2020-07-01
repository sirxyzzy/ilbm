use anyhow::Result;
use std::env;
// use std::io::prelude::*;

use std::fs::File;

fn main() -> Result<()> {
    for argument in env::args().skip(1) {
        println!("Trying to read {}", argument);
        ilbm::read_from_file( File::open(argument)?)?;
    }

    Ok(())
}