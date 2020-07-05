#[macro_use]
extern crate log;

use anyhow::Result;
use std::env;

use std::fs::{self,File};

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

    let now = std::time::Instant::now();
    let mut count = 0;
    let mut failed = 0;

    for path in files {
        count += 1;
        let name = path.to_string_lossy();
        info!("Loading {}", name);
        match ilbm::read_from_file( File::open(&path)?) {
            Ok(image) => {
                println!("{} {}", image, name);
            }
            Err(e) => {
                println!("ERROR! Failed to load {} {}", name, e);
                failed += 1;
            }
        }
    }

    println!("Processed {} files in {:?}, {} files failed to load", count, now.elapsed(), failed);
    
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
