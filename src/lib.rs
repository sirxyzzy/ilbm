#![feature(bufreader_seek_relative)]

#[macro_use]
extern crate log;
pub mod iff;
mod bytes;

use std::fs::File;
use iff::{IffReader, IffChunk, ChunkId};
use std::io::BufReader;
use thiserror::Error;
use bytes::BigEndian;

/// Custom errors for ilbm library
#[derive(Error, Debug)]
pub enum Error {
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
}

/// Standardize my result Errors
pub type Result<T> = std::result::Result<T,Error>;

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
/// in terms of interpretation. It comes from the CAMG chunk 
#[derive(Copy, Debug, Clone, Default)]
pub struct DisplayMode (u32);

impl DisplayMode {
    pub fn is_ham(&self) -> bool {self.0 & 0x800 != 0} 
    pub fn is_halfbrite(&self) -> bool {self.0 & 0x80 != 0} 
}

impl std::fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let mode_type = if self.is_ham() { "HAM" } else if self.is_halfbrite() { "HALF" } else {""};
        write!(f, "0x{:X} {}", self.0, mode_type) 
    }
}

#[derive(Copy, Debug, Clone, Default)]
pub struct RgbValue (u8, u8, u8);

/// This is an amalgam of information drawn from
/// various chunks in the ILBM, mapped to more native
/// types such as usize for u16
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
            // Uncompressed...
            if self.raw_data.len() < self.width {
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

pub fn read_from_file(file: File) -> Result<IlbmImage> {
    let reader = IffReader::new(BufReader::new(file));

    for chunk in reader {
        debug!("Chunk {}", chunk);

        if chunk.is_form() {
            let is_ilbm = &chunk.form_type() == b"ILBM";
            let is_pbm = &chunk.form_type() == b"PBM ";
            if  is_ilbm || is_pbm {
                let mut image = IlbmImage{ form_type: chunk.form_type(), ..Default::default() };

                let mut map: Option<ColorMap> = None;
                let mut got_header = false;

                for sub_chunk in chunk.sub_chunks() {

                    image.chunk_types.push(sub_chunk.id());

                    match &sub_chunk.id().0 {
                        b"BMHD" => { 
                            read_bitmap_header(sub_chunk, &mut image)?;
                            debug!("after header {}", image);
                            got_header = true;
                        }

                        b"CMAP" => {
                            let m = read_color_map(sub_chunk)?;
                            debug!("Got color map, of map_size {}", m.colors.len());
                            image.map_size = m.colors.len();
                            map = Some(m);
                        }

                        b"CAMG" => {
                            let mode = read_display_mode(sub_chunk)?;
                            debug!("Got display mode: {}", mode);
                            image.display_mode = mode;
                        }

                        b"DPI " => {
                            let dpi = read_dpi(sub_chunk)?;
                            debug!("Got dpi: {}", dpi);
                            image.dpi = dpi;
                        }

                        b"BODY" => {
                            debug!("Got BODY!");

                            if !got_header {
                                return Err(Error::NoHeader)
                            }

                            read_body(sub_chunk, image.display_mode, map, &mut image)?;
                            return Ok(image)
                        }

                        _ => {
                            debug!("Skipping sub chunk {}", sub_chunk.id());
                            continue;
                        }
                    }
                }
            }
        }
    }

    Err(Error::NoImage)
}

fn read_dpi(chunk: IffChunk) -> Result<Size2D> {
    Ok(Size2D(
        chunk.data().get_u16()? as usize,
        chunk.data().get_u16()? as usize,
    ))
}

fn read_display_mode(chunk: IffChunk) -> Result<DisplayMode> {
    Ok(DisplayMode(chunk.data().get_u32()?))
}

fn read_color_map(chunk: IffChunk) -> Result<ColorMap> {
    let mut buf = &chunk.data()[..];

    let count = buf.len() / 3;

    let mut colors: Vec<RgbValue> = Vec::with_capacity(count);

    //
    // Old color maps used the top four bits only,
    // zero padding the lower four bits, this causes
    // colors to NEVER reach full brightness (0xF0 < 0xFF!)
    // We look out for this, as we generate, and adjust after in the rare case we need to
    //

    let mut found_low_bits = false;
    for _i in 0..count {
        let red = buf.get_u8()?;
        let green = buf.get_u8()?;
        let blue = buf.get_u8()?;

        if ((red & 0xf) | (green & 0xf) | (blue & 0xf)) != 0 {
            found_low_bits = true;
        }

        colors.push(RgbValue(red, green, blue));
    }

    // This is where we fix up 4 bit color maps, if we need to
    if !found_low_bits {
        info!("Found old color map, fixing up!...");
        colors.iter_mut().for_each(|color| *color = RgbValue(color.0 | (color.0 >> 4), color.1 | (color.1 >> 4), color.2 | (color.2 >> 4)));
    }

    Ok(ColorMap{colors})
}

fn read_body(chunk: IffChunk, mode:DisplayMode, map: Option<ColorMap>, image: &mut IlbmImage) -> Result<()> {
    debug!("{}", image);
    match map {
        Some(map) => read_body_with_cmap(chunk, mode, map, image),
        None => read_body_no_map(chunk, mode, image)
    }
}

/// Read a body with no color map, so HAM (6 planes) or deep (24 or 32)
fn read_body_no_map(_chunk: IffChunk, _mode:DisplayMode, _image: &mut IlbmImage) -> Result<()> {
    Err(Error::NotSupported("deep mode".to_string()))
}

/// Read a body using a color map, pixel data is interpreted as indexes into the map  
fn read_body_with_cmap(chunk: IffChunk, mode:DisplayMode, color_map: ColorMap, image: &mut IlbmImage) -> Result<()> {
    // Having a CMAP implies certain limitations, here we limit color indices to a u8
    // so the number of planes cannot exceed 8 (bits) and the map must be big enough
    if image.planes > 8 {
        return Err(Error::NotSupported("Color map with more than 8 planes".to_string()));
    }

    let Size2D(width, height) = image.size;
    let planes = image.planes;

    // Bytes per row (always EVEN)
    let row_stride = ((width + 15)/16) * 2;

    let mut rows = RowIter::new(chunk.data(), row_stride, image.compression);

    // We assemble all the resolved RGB values in here
    let mut pixels= Vec::<u8>::with_capacity(3 * width * height);

    for _row in 0..height {
        let mut row= vec![0u8;width];
        let mut plane_bit: u8 = 1;
        for _plane in 0..planes {
            let row_data = rows.next().ok_or(Error::NoData)?;

            // Read planes, each plane contributes 1 bit

            for (offset, byte) in row_data.iter().enumerate() {
                let mut plane_byte = *byte;

                for b in 0..8 {
                    if plane_byte & 0x80 != 0 {
                        let index = (offset * 8) + b;

                        // Check width, because of padding and rounding, we may
                        //  have more data than the width dictates
                        if index < width {
                            // Bit is on, so set the bit, corresponding with the plane, in the row data
                            row[index] |= plane_bit;
                        }
                    }
                    plane_byte <<= 1;
                }
            }

            // planes start at the low bit, so shift left the bit we or into the result
            plane_bit <<= 1; 
        }

        if image.masking == Masking::HasMask {
            // Read mask plane, we don't support this yet,
            // need to go RGBA to do so
            
            // get the next row
            let _row_data = rows.next().ok_or(Error::NoData)?;          
        }

        if mode.is_ham() {
            push_row_bytes_ham(row, planes, &color_map, &mut pixels)?;
        } else if mode.is_halfbrite() {
            push_row_bytes_halfbrite(row, &color_map, &mut pixels)?;
        } else {
            push_row_bytes(row, &color_map, &mut pixels)?;
        }
    }

    assert_eq!(pixels.len(), 3 * width * height);

    image.pixels = pixels;

    Ok(())
}

/// simple case where we simply index into the pixel map
fn push_row_bytes(row: Vec<u8>, color_map: &ColorMap, pixels: &mut Vec<u8>) -> Result<()> {
    // Resolve through color map, and add to output vector
    for p in row {
        let index = p as usize;
        let map_size = color_map.colors.len();

        if index >= map_size  {
            return Err(Error::NoMapEntry{index, map_size})
        }

        let rgb = color_map.colors[index];

        pixels.push(rgb.0);
        pixels.push(rgb.1);
        pixels.push(rgb.2);
    }

    Ok(())
}

/// HAM is tricky, it works by reserving two planes (hence two bits) to indicate
/// whether we index as normal (using planes-2 bits) or if we take those low order
// bits to modify the PREVIOUS value
fn push_row_bytes_ham(row: Vec<u8>, planes: usize, color_map: &ColorMap, pixels: &mut Vec<u8>) -> Result<()> {
    // Resolve through color map, and add to output vector
    let map_size = color_map.colors.len();

    // In ham, we steal two planes to determine the modify part
    // and mask the color index appropriately
    let mod_shift = planes - 2;
    let index_mask = (1 << mod_shift) - 1;

    // The modify color needs to be shifted back, as generally is is less than 8 bits long
    // but we always render to 8 bit components
    let mod_color_shift = 8 - mod_shift; // Left shift applied to color when modifying 

    // Make sure we have at least a border color, I don't want to panic
    if map_size == 0 {
        return Err(Error::NoMapEntry{index:0, map_size})
    }
    
    // If we modify at the start of the row, we modify the so called border color
    let mut color = color_map.colors[0];

    for p in row {
        let row_val = p as usize;
        let mod_bits = row_val >> mod_shift;  // After this shift, low order mod_bits indicate whether we modify
        let low_bits = row_val & index_mask;  // Mask off the mod bits for just the actual index (if used)
 
        // Based on the mod bits, either replace the color
        // with one in the color map, or modify one component
        // of the previous color
        match mod_bits {
            0 => {
                // Index as normal using low order bits
                if low_bits >= map_size  {
                    return Err(Error::NoMapEntry{index:low_bits, map_size})
                }  
                
                // Just use the color in the map, no "modify"
                color = color_map.colors[low_bits]
            }
            2 => {
                // Modify RED in previous, scale to make 8 bit
                let mut component = low_bits << mod_color_shift;

                // Sadly, that shifted zeros into the low end, so we would never reach peak intensity
                // we fix that up by grabbing the appropriate number of bits from the high end and
                // or-ing back to the low end
                component |= component >> (8-mod_color_shift);

                // Based on the mod_bits, modify the corresponding component
                // RgbValue(component, prev_color.1, prev_color.2)
                color.0 = component as u8;
            }
            1 => {
                // Modify BLUE in previous
                let mut component = low_bits << mod_color_shift;
                component |= component >> (8-mod_color_shift);
                // RgbValue(prev_color.0, prev_color.1, component)
                color.2 = component as u8;    
            }
            3 => {
                // Modify GREEN in previous
                let mut component = low_bits << mod_color_shift;
                component |= component >> (8-mod_color_shift);
                // RgbValue(prev_color.0, component, prev_color.2)
                color.1 = component as u8;   
            }
            _ => panic!("Logically, we cannot get here, as we only masked two bits, the compiler can't work that out!")
        }

        pixels.push(color.0);
        pixels.push(color.1);
        pixels.push(color.2);
    }

    Ok(())
}

/// HalfBrite is relatively simple, we need only half the colors in the color map,
/// as one bit (the lowest in the index) tells us to simply half (darken) what the rest
/// of the index tells us
fn push_row_bytes_halfbrite(row: Vec<u8>, color_map: &ColorMap, pixels: &mut Vec<u8>) -> Result<()> {
    // Resolve through color map, and add to output vector, but use only half the map
    // darkening pixels in the upper half
    for p in row {
        let index = (p >> 1) as usize;

        let map_size = color_map.colors.len();

        if index >= map_size  {
            return Err(Error::NoMapEntry{index, map_size})
        }

        let rgb = color_map.colors[index];

        if p & 1 != 0 {
            // Half brightness
            pixels.push(rgb.0 >> 1);
            pixels.push(rgb.1 >> 1);
            pixels.push(rgb.2 >> 1);
        } else {
            // Normal
            pixels.push(rgb.0);
            pixels.push(rgb.1);
            pixels.push(rgb.2);
        }
    }

    Ok(())
}

fn read_bitmap_header(chunk: IffChunk, image: &mut IlbmImage) -> Result<()> {
    let mut buf = &chunk.data()[..];

    assert!(buf.len() >= 20);
    image.size = Size2D(buf.get_u16()? as usize, buf.get_u16()? as usize);

    let _x = buf.get_i16()?;
    let _y = buf.get_i16()?;

    image.planes = buf.get_u8()? as usize;
    image.masking = as_masking(buf.get_u8()?);
    image.compression = buf.get_u8()? != 0;

    let _pad = buf.get_u8()?;

    image.transparent_color = buf.get_u16()? as usize;
    image.pixel_aspect = Size2D(buf.get_u8()? as usize, buf.get_u8()? as usize);
    image.page_size = Size2D(buf.get_i16()? as usize, buf.get_i16()? as usize);

    // Lets do some early validations of crazy
    if image.size.0 == 0 || image.size.1 == 0 {
        return Err(Error::InvalidHeader{ 
            expected: "non-zero height and width".to_string(),
            actual: format!("{}", image.size)
        });
    }

    if image.planes == 0 {
        return Err(Error::NoPlanes);
    }

    Ok(())
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
        let n = data.get_i8()? as i16;       

        if n >= 0 {
            for _i in 0..(n+1) {
                unpacked.push(data.get_u8()?);
            }
        } else if n != -128 {
            let b = data.get_u8()?;

            for _i in 0..(-n + 1) {
                unpacked.push(b);
            }
        }
    }

    if unpacked.len() != byte_width {
        Err(Error::InvalidData(format!("decompression unpacked too many bytes, expected {} but got {}", byte_width, unpacked.len())))
    } else {
        Ok((data, unpacked))
    }
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
