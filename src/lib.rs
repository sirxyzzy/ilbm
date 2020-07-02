#![feature(bufreader_seek_relative)]

pub mod iff;

use std::fs::File;
use iff::{IffReader, IffChunk};
use std::io::BufReader;
use bytes::buf::Buf;
use thiserror::Error;

/// Custom errors for ilbm library
#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader {
        expected: String,
        found: String,
    },

    #[error("File does not contain image data")]
    NoImage,

    #[error("File does not contain image header (FORM.BMHD)")]
    NoHeader,

    #[error("Unexpected end of image data")]
    NoData,

    #[error("{0} not supported")]
    NotSupported(String),
}

/// Standardize my result Errors
type Result<T> = std::result::Result<T,Error>; 

#[repr(u8)]
enum Masking {
    None = 0, 
    HasMask = 1,
    HasTransparentColor = 2,
    Lasso = 3
}

#[derive(Copy, Debug, Clone)]
pub struct RgbValue (u8, u8, u8);

#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,

    /// Left to right in row, then bottom to top
    /// so indexes look like y * width + x where
    /// y=0 is the bottom  
    pub pixels: Vec<RgbValue>
}

#[derive(Debug, Clone)]
struct BitmapHeader {
    w: u16, h: u16, // raster width & height in pixels
    x: i16, y: i16, // Ignored: pixel position in a larger space to render this image
    planes: u8,    // # source bitplanes, 0 indicates this is a color map only
    masking: u8,
    compression: u8,
    pad: u8, // Unused
    transparent_color: u16,          // transparent "color number" (sort of)
    x_aspect: u8, y_aspect: u8,       // Ignored: pixel aspect, a ratio width : height
    page_width: i16, page_height: i16 // Ignored: source "page" size in pixels
}

#[derive(Debug, Clone)]
struct ColorMap {
    colors: Vec<RgbValue>
}

struct RowIter<'a> {
    raw_data: &'a [u8],
    width: usize,
    compressed: bool, 
}

impl<'a> RowIter<'a> {
    fn new(raw_data: &[u8], width: usize, compressed: bool) -> RowIter {
        RowIter{raw_data, width, compressed}
    }
}

impl<'a>  Iterator for RowIter<'a>  {
    type Item = Vec<u8>;
    fn next(&mut self) -> std::option::Option<<Self as std::iter::Iterator>::Item> {
        if self.compressed {
            match unpacker(&self.raw_data, self.width) {
                Ok((remaining, row)) => {
                    self.raw_data = remaining;
                    Some(row)
                },
                Err(_) => None
            }
        } else {
            if self.raw_data.len() > self.width {
                None
            } else {
                let width = self.width;
                let row = &self.raw_data[..width];
                self.raw_data = &self.raw_data[width..];
                Some(row.to_vec())
            }
        }
    }
}

pub fn read_from_file(file: File) -> Result<Image> {
    let reader = IffReader::new(BufReader::new(file));

    for chunk in reader {
        println!("Chunk {}", chunk);

        if chunk.is_form() {
            let is_ilbm = chunk.form_type() == b"ILBM";
            let is_lbm = chunk.form_type() == b"LBM ";
            if  is_ilbm || is_lbm {

                // We hopefully find and stash away some data which we find expect
                // to the BODY
                let mut header: Option<BitmapHeader> = None;
                let mut map: Option<ColorMap> = None;

                for sub_chunk in chunk.sub_chunks() {
                    println!("Sub chunk within form {}", sub_chunk);

                    match sub_chunk.id() {
                        b"BMHD" => { 
                            let h = read_bitmap_header(sub_chunk)?;

                            if h.planes == 0 {
                                return Err(Error::NoImage);
                            }

                            println!("Got {:?}", h);
                            header = Some(h);
                        }

                        b"CMAP" => {
                            let m = read_color_map(sub_chunk);
                            println!("Got color map, size {}", m.colors.len());
                            map = Some(m);
                        }

                        b"BODY" => {
                            println!("Got BODY!");
                            return read_body(sub_chunk, header, map);
                        }

                        x => {
                            println!("Skipping sub chunk {}", String::from_utf8_lossy(x));
                            continue;
                        }
                    }
                }
            }
        }
    }

    Err(Error::NoImage)
}

fn read_color_map(chunk: IffChunk) -> ColorMap {
    let mut buf = &chunk.data()[..];

    let count = buf.len() / 3;

    let mut colors: Vec<RgbValue> = Vec::with_capacity(count);

    for _i in 0..count {
        colors.push(
            RgbValue(
                buf.get_u8(),
                buf.get_u8(),
                buf.get_u8()
            )
        );
    }

    ColorMap{colors}
}

fn read_body(chunk: IffChunk, header: Option<BitmapHeader>, map: Option<ColorMap>) -> Result<Image> {
    match header {
        Some(header) => match map {
            Some(map) => read_body_with_cmap(chunk, header, map),
            None => read_body_no_map(chunk, header)
        },
        None => Err(Error::NoHeader)
    }
}

/// Read a body with no color map, so HAM (6 planes) or deep (24 or 32)
fn read_body_no_map(chunk: IffChunk, header: BitmapHeader) -> Result<Image> {
    return Err(Error::NotSupported("deep mode".to_string()));
}

/// Read a body using a color map, pixel data is interpreted as indexes into the map  
fn read_body_with_cmap(chunk: IffChunk, header: BitmapHeader, color_map: ColorMap) -> Result<Image> {
    // Having a CMAP implies certain limitations, here we limit color indices to a u8
    // so the number of planes cannot exceed 8 (bits) and the map must be big enough
    if header.planes > 8 {
        return Err(Error::NotSupported("Color map with more than 8 planes".to_string()));
    }

    let needed_colors = 1 << header.planes;

    if color_map.colors.len() < needed_colors {
        let m = format!("Color map needs {} entries to handle {} planes, but only has {}", needed_colors, header.planes, color_map.colors.len());
        return Err(Error::NotSupported(m));        
    }

    let width = header.w as usize;
    let height = header.h as usize;

    // Bytes per row (always EVEN)
    let row_stride = ((width + 15)/16) * 2;

    let mut rows = RowIter::new(chunk.data(), row_stride, header.compression != 0);

    // We assemble all the resolved RGB values in here
    let mut pixels= Vec::<RgbValue>::with_capacity(width * height);

    for _row in 0..height {
        let mut row= vec![0u8;width];
        let mut plane_bit: u8 = 1;
        for _plane in 0..header.planes {
            let row_data = rows.next().ok_or(Error::NoData)?;

            // Read planes, each plane contributes 1 bit

            // For all bytes in the row
            for offset in 0..row_stride {
                let mut plane_byte = row_data[offset];

                for b in 0..8 {
                    if plane_byte & 0x80 != 0 {
                        let index = (offset * 8) + b;

                        // Check width, because of padding and rounding, we may
                        //  have more data than the width dictates
                        if index < width {
                            // Bit is on, so set the bit, corresponding with the plane, in the row data
                            row[index] = row[index] | plane_bit;
                        }
                    }
                    plane_byte = plane_byte << 1;
                }
            }

            plane_bit = plane_bit << 1; 
        }

        if header.masking == (Masking::HasMask as u8) {
            // Read mask plane, right now we simply ignore it
            // but we must get the next row 

            let _row_data = rows.next().ok_or(Error::NoData)?;          
        }

        // Resolve through color map, and add to output vector
        pixels.extend(row.iter().map(|i| color_map.colors[*i as usize]));
    }

    assert_eq!(pixels.len(), width * height);

    Ok(Image{width, height, pixels})
}

fn read_bitmap_header(chunk: IffChunk) -> Result<BitmapHeader> {
    let mut buf = &chunk.data()[..];

    assert!(buf.len() >= 20);

    let header = BitmapHeader{
        w: buf.get_u16(),
        h: buf.get_u16(),
        x: buf.get_i16(),
        y: buf.get_i16(),
        planes: buf.get_u8(),
        masking: buf.get_u8(),
        compression: buf.get_u8(),
        pad: buf.get_u8(), // Unused
        transparent_color: buf.get_u16(),          // transparent "color number" (sort of)
        x_aspect: buf.get_u8(),
        y_aspect: buf.get_u8(),       // pixel aspect, a ratio width : height
        page_width: buf.get_i16(),
        page_height: buf.get_i16()
    };

    Ok(header)
}

// UnPacker:
//   LOOP until produced the desired number of bytes
//       Read the next source byte into n
//       SELECT n FROM
//           [0..127]   => copy the next n+1 bytes literally
//           [-1..-127] => replicate the next byte -n+1 times
//           -128       => no operation
//           ENDCASE;
//       ENDLOOP;
pub fn unpacker(input: &[u8], byte_width: usize) -> Result<(&[u8], Vec<u8>)> {
    let mut data = input;
    let mut unpacked: Vec<u8> = Vec::with_capacity(byte_width);

    while unpacked.len() < byte_width {
        let n = data.get_i8();
        if n >= 0 {
            for _i in 0..(n+1) {
                unpacked.push(data.get_u8());
            }
        } else {
            if n != -128 {
                let b = data.get_u8();
                for _i in 0..(-n + 1) {
                    unpacked.push(b);
                }
            }
        }
    }

    assert_eq!(unpacked.len(), byte_width, "Unpacker expanded to too many bytes");

    Ok((data, unpacked))
}

#[cfg(test)]
mod tests {
    use super::unpacker;

    #[test]
    fn unpack_1() {
        let compressed = [0u8, 66u8];
        let expected = [66u8];

        let (remaining, unpacked) = unpacker(&compressed, 1).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn unpack_2() {
        let compressed = [0u8, 66u8, 67u8];
        let expected = [66u8];

        let (remaining, unpacked) = unpacker(&compressed, 1).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn unpack_3() {
        let compressed = [2u8, 66u8, 67u8, 68u8, 69u8];
        let expected = [66u8, 67u8, 68u8];

        let (remaining, unpacked) = unpacker(&compressed, 3).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    #[should_panic]
    fn unpack_4() {
        let compressed = [2u8, 66u8, 67u8]; // This is intentionally broken, not enough data
        let (_remaining, _unpacked) = unpacker(&compressed, 3).unwrap();
    }

    #[test]
    fn unpack_5() {
        let compressed = [255u8, 10u8];
        let expected = [10u8, 10u8];

        let (remaining, unpacked) = unpacker(&compressed, 2).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn unpack_6() {
        let compressed = [253u8, 10u8];
        let expected = [10u8, 10u8, 10u8, 10u8];

        let (remaining, unpacked) = unpacker(&compressed, 4).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    #[should_panic]
    fn unpack_7() {
        let compressed = [250u8, 10u8]; // Broken, will generate too much data
        let (_remaining, _unpacked) = unpacker(&compressed, 1).unwrap();
    }
}
