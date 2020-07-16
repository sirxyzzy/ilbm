#[macro_use]
extern crate log;
pub mod iff;
mod bytes;
mod compression;
mod read;

use iff::ChunkId;
use thiserror::Error;
use std::path::Path;

/// Global settings when reading image files
pub struct ReadOptions {
    pub read_pixels: bool,
    pub page_scale: bool,
}

/// Main entry point
pub fn read_from_file<P: AsRef<Path>>(file: P, options: ReadOptions) -> Result<IlbmImage> {
    read::read_file(file, options)
}

/// Custom errors for ilbm library
#[derive(Error, Debug)]
pub enum IlbmError {
    #[error("invalid header (expected {expected:?}, found {actual:?})")]
    InvalidHeader {
        expected: String,
        actual: String,
    },

    #[error("invalid data: {0}")]
    InvalidData (
        String
    ),

    #[error("File does not contain image data")]
    NoImage,

    #[error("No planes, possibly a color map with no image data")]
    NoPlanes,

    #[error("File does not contain image header (FORM.BMHD)")]
    NoHeader,

    #[error("Color map of map_size {map_size:?} has no entry for {index:?}")]
    NoMapEntry{ index: usize, map_size: usize},

    #[error("Unexpected end of image data")]
    NoData,

    #[error("{0} not supported")]
    NotSupported(String),

    #[error("IO Error")]
    Io {
        #[from]
        source: std::io::Error
    },
}

/// Standardize my result Errors
pub type Result<T> = std::result::Result<T,IlbmError>;

#[derive(Debug,Clone,Copy, PartialEq)]
pub enum Masking {
    NoMask, 
    HasMask,
    HasTransparentColor,
    Lasso
}

impl Default for Masking {
    fn default() -> Self { Masking::NoMask }
}

fn as_masking(v: u8) -> Masking {
    match v {
        0 => Masking::NoMask,
        1 => Masking::HasMask,
        2 => Masking::HasTransparentColor,
        3 => Masking::Lasso,
        x => {
            error!("Masking value of {} unsupported, mapping to None", x);
            Masking::NoMask
        }
    }
}

/// Display mode, aka ModeID is Amiga specific, and quite complex
/// in terms of interpretation. However, our usage is pretty trivial
// It comes from the CAMG chunk 
#[derive(Copy, Debug, Clone, Default)]
pub struct DisplayMode (u32);

impl DisplayMode {
    pub fn is_ham(&self) -> bool {self.0 & 0x800 != 0} 
    pub fn is_halfbrite(&self) -> bool {self.0 & 0x80 != 0}

    pub fn new(mode: u32) -> DisplayMode {
        DisplayMode(mode)
    }
    
    pub fn ham() -> DisplayMode {
        DisplayMode(0x800)
    }
}

impl std::fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        if self.is_ham() { 
            write!(f, "0x{:X} HAM", self.0)
        } else if self.is_halfbrite() { 
            write!(f, "0x{:X} HALF", self.0) 
        } else {
            write!(f, "0x{:X}", self.0)
        }
    }
}

#[derive(Copy, Debug, Clone, Default)]
pub struct RgbValue (u8, u8, u8);

/// This is an amalgam of information drawn from
/// various chunks in the ILBM, mapped to more native
/// types such as usize for u16, and enums for masking
#[derive(Debug, Default)]
pub struct IlbmImage {
    pub form_type: ChunkId,
    pub size: Size2D,
    pub map_size: usize,
    pub chunk_types: Vec<ChunkId>,
    pub planes: usize,
    pub masking: Masking,
    pub compression: bool,
    pub display_mode: DisplayMode,
    pub dpi: Size2D,
    pub pixel_aspect: Size2D,
    pub transparent_color: usize, // Actually a color index
    pub page_size: Size2D,

    /// RGB data triples
    /// Left to right in row, then top to bottom
    /// so indexes look like y * width + x where
    /// y=0 is the top  
    pub pixels: Vec<u8>
}

impl std::fmt::Display for IlbmImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let compressed = if self.compression { "Comp" } else { "" };
        write!(f, "{} {} dpi:{} p:{} {} {:?} map:{} mode:{} aspect:{} trans:{} page:{}",
        self.form_type, self.size, self.dpi, self.planes,
        compressed, self.masking, self.map_size, self.display_mode, 
        self.pixel_aspect, self.transparent_color, self.page_size)
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Size2D (usize,usize);

impl Size2D {
    pub fn width(&self) -> usize {self.0}
    pub fn height(&self) -> usize {self.1}
}

impl std::fmt::Display for Size2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}x{}", self.width(), self.height()) 
    }
}

#[derive(Debug, Clone)]
struct ColorMap {
    colors: Vec<RgbValue>
}
