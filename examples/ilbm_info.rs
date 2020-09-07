#[macro_use]
extern crate log;

use argh::FromArgs;
use anyhow::Result;
use env_logger::{Builder};
use log::LevelFilter;
use std::path::{Path, PathBuf};
use std::fs;


#[derive(FromArgs)]
/// Read the information from one or many ILBM image files
struct Opts {
    /// whether or not to show debug output
    #[argh(switch, short = 'v')]
    verbose: bool,

    /// read pixels as well for a more complete check
    #[argh(switch, short = 'p')]
    pixels: bool,

    #[argh(positional)]
    files: Vec<String>,
}

fn main() -> Result<()> {
    let opts: Opts = argh::from_env();

    let mut builder = Builder::from_default_env();

    if opts.verbose {
        builder.filter(None, LevelFilter::Debug);
    }

    builder
        .init();

    info!("starting up");

    // Get a list of files, parameters are either files, or folders
    let files = all_files(&opts.files)?;

    if files.is_empty() {
        anyhow::bail!("I need some files or folders!");
    }

    let now = std::time::Instant::now();
    let mut count = 0;
    let mut failed = 0;

    for path in files {
        count += 1;
        let name = path.to_string_lossy();
        info!("Loading {}", name);

        let image_result = 
            ilbm::read_from_file( &path, ilbm::ReadOptions{ read_pixels: opts.pixels, page_scale: true});

        match image_result {
            Ok(image) => println!("{} {}", image, name),
            Err(e) => {
                failed += 1;
                println!("ERROR! {} {}", e, name)
            }
        }
    }

    if failed > 0 {
        println!("Processed {} files in {:?}, ({} failed)", count, now.elapsed(), failed);
    } else {
        println!("Processed {} files in {:?}", count, now.elapsed());
    }
    
    Ok(())
}

/// Take list or args, treat as files or folders and gather all
fn all_files(paths: &[String]) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in paths {
        get_files(&Path::new(&arg), &mut files)?;
    }
    Ok(files)
}

/// Recursively gather all files...
fn get_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        add_file(path.to_path_buf(), files);
    } else if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path_buf = entry.path();
            if path_buf.is_dir() {
                get_files(&path_buf, files)?;
            } else {
                add_file(path_buf, files);
            }
        }
    } else {
        debug!("{} is not a file or folder, skipping!", path.to_string_lossy());
    }
    Ok(())
}

fn add_file(path: PathBuf, files: &mut Vec<PathBuf>) {
    let name = path.file_name().unwrap().to_string_lossy().to_lowercase();

    debug!("Got file '{}'", name);

    if name.contains("read me") || name.contains("readme") || name.ends_with(".txt") || name.ends_with(".info") {
        debug!("Skipping {}", path.to_string_lossy());
        return;
    }

    files.push(path);
}
