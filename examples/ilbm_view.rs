#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;

use std::fs::{self,File};
use show_image::{ImageInfo, make_window, KeyCode, Window};
use std::time::Duration;

use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    env_logger::builder()
        .init();

    info!("starting up");

    // Get a list of files, parameters are either files, or folders
    let files = args_to_file_list()?;

    if files.len() == 0 {
        anyhow::bail!("I need some files or folders!");
    }

    let mut file_iter = files.iter();

    // the rest of this is just the viewing logic...

    // Create a window to display the image.
    let window = make_window("ILBM View").unwrap();

    load_and_show_image(file_iter.next().unwrap(), &window)?;

    // Print keyboard events until Escape is pressed, then exit.
    // If the user closes the window, wait_key() will return an error and the loop also exits.
    while let Ok(event) = window.wait_key(Duration::from_millis(100)) {
        if let Some(event) = event {
            if event.key == KeyCode::Escape {
                break;
            } else if event.key == KeyCode::Enter {
                match file_iter.next() {
                    Some(p) => load_and_show_image(p, &window)?,
                    None => break
                }          
            }
        }
    }

    // Make sure all background tasks are stopped cleanly.
    show_image::stop().unwrap();

    info!("DONE");
    
    Ok(())
}

/// Load an image from a file, and render it in a window
fn load_and_show_image(path: &PathBuf, window: &Window) -> Result<()> {
    let name = path.to_string_lossy();
    println!("Loading {}", name);
    let image = ilbm::read_from_file( &path)?;
    println!("{}", image);


    // Change to a form that show_image understands
    let pixels_and_info = (image.pixels, ImageInfo::rgb8(image.size.width(), image.size.height()));

    // stuff it in the window
    window.set_image(pixels_and_info, name).unwrap();

    Ok(())
}

/// Take list or args, treat as files or folders and gather all
fn args_to_file_list() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in env::args().skip(1) {
        get_files(&Path::new(&arg), &mut files)?;
    }
    Ok(files)
}

/// Recursively gather all files...
fn get_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path_buf = entry.path();
            if path_buf.is_dir() {
                get_files(&path_buf, files)?;
            } else {
                files.push(path_buf);
            }
        }
    } else {
        debug!("{} is not a file or folder, skipping!", path.to_string_lossy());
    }
    Ok(())
}
