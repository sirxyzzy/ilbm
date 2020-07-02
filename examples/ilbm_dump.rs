#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;
// use std::io::prelude::*;

use std::fs::File;
use show_image::{ImageInfo, make_window, KeyCode};
use std::time::Duration;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("starting up");

    for argument in env::args().skip(1) {
        info!("Trying to read {}", argument);
        let image = ilbm::read_from_file( File::open(&argument)?)?;
        info!("Read an image, in memory size {}", image.pixels.len() * 3);

        let image2 = (image.pixels, ImageInfo::rgb8(image.width, image.height));

        // Create a window and display the image.
        let window = make_window("image").unwrap();
        window.set_image(image2, argument).unwrap();

        // Print keyboard events until Escape is pressed, then exit.
        // If the user closes the window, wait_key() will return an error and the loop also exits.
        while let Ok(event) = window.wait_key(Duration::from_millis(100)) {
            if let Some(event) = event {
                if event.key == KeyCode::Escape {
                    break;
                }
            }
        }

        // Make sure all background tasks are stopped cleanly.
        show_image::stop().unwrap();

        info!("DONE");
    }

    Ok(())
}