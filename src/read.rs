use std::path::Path;
use crate::compression;
use crate::*;
use crate::bytes::BigEndian;
use crate::iff::{IffReader, IffChunk};

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

/// An iterator for image rows, that can decompress on the fly
impl<'a>  Iterator for RowIter<'a>  {
    type Item = Vec<u8>;
    fn next(&mut self) -> std::option::Option<<Self as std::iter::Iterator>::Item> {
        if self.compressed {
            match compression::unpacker(&self.raw_data, self.width) {
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

pub fn read_file<P: AsRef<Path>>(path: P, options: ReadOptions) -> Result<IlbmImage> {
    // We choose to buffer the entire file, it is quite a bit faster,
    // and Amiga image files tend to be small anyway
    let all_bytes = std::fs::read(path)?;
    let reader = IffReader::new(std::io::Cursor::new(all_bytes));

    for chunk in reader {
        debug!("Chunk {}", chunk);

        // We only look at forms, they encapsulate several sub-chunks
        if chunk.is_form() {
            let is_ilbm = &chunk.form_type() == b"ILBM";

            if is_ilbm {
                let mut image = IlbmImage{ form_type: chunk.form_type(), ..Default::default() };

                let mut map: Option<ColorMap> = None;
                let mut got_header = false;
                let mut got_camg = false;

                for sub_chunk in chunk.sub_chunks() {

                    image.chunk_types.push(sub_chunk.id());

                    match &sub_chunk.id().0 {
                        b"BMHD" => { 
                            read_bitmap_header(sub_chunk, &mut image)?;
                            debug!("after header {}", image);
                            if image.masking != Masking::NoMask {
                                warn!("Image masking (transparency) not supported!");
                            }
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
                            got_camg = true;
                            image.display_mode = mode;
                        }

                        b"DPI " => {
                            let dpi = read_dpi(sub_chunk)?;
                            debug!("Got dpi: {}", dpi);
                            image.dpi = dpi;
                        }

                        b"BODY" => {
                            debug!("Got BODY! {}", image);

                            if !got_header {
                                return Err(IlbmError::NoHeader)
                            }

                            // Reportedly, some HAM6 files are missing the CAMG chunk. 
                            // A file with no CAMG chunk, 6 bit planes, and 16 palette colors assumed to be HAM6
                            if !got_camg && image.planes == 6 && image.map_size == 16 {
                                // force on HAM
                                warn!("Looks like HAM6, but  didn't get a CAMG, forcing HAM");
                                image.display_mode = DisplayMode::ham();
                            }


                            if image.display_mode.is_halfbrite() && image.planes != 6 {
                                return Err(IlbmError::NotSupported(format!("Halfbright only works with 6 planes, but I have {}", image.planes)));
                            }

                            if options.read_pixels {
                                read_body(sub_chunk, image.display_mode, map, &mut image)?;
                            }

                            if options.page_scale {
                                // This is a bit of a heuristic, but 
                                // only the Amiga messes with page sizes where
                                // the width is so much less that the height,
                                // and in those cases the pixels are essentially double-wide
                                if image.page_size.width() < image.page_size.height() {
                                    debug!("Scaling image to suit modern screen aspect ratios!");

                                    let old = &image.pixels; 
                                    let mut new = Vec::<u8>::with_capacity(image.pixels.len() * 2);

                                    // iterate over the old pixels
                                    for i in (0..old.len()).step_by(3) {
                                        new.push(old[i]);
                                        new.push(old[i+1]);
                                        new.push(old[i+2]);
                                        new.push(old[i]);
                                        new.push(old[i+1]);
                                        new.push(old[i+2]);
                                    }

                                    image.pixels = new;
                                    image.size.0 *= 2;
                                }
                            }

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

    Err(IlbmError::NoImage)
}

fn read_dpi(chunk: IffChunk) -> Result<Size2D> {
    Ok(Size2D(
        chunk.data().get_u16()? as usize,
        chunk.data().get_u16()? as usize,
    ))
}

fn read_display_mode(chunk: IffChunk) -> Result<DisplayMode> {
    Ok(DisplayMode::new(chunk.data().get_u32()?))
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
        None => read_body_no_map(chunk, image)
    }
}


/// Read a body using a color map, pixel data is interpreted as indexes into the map  
fn read_body_with_cmap(chunk: IffChunk, mode:DisplayMode, color_map: ColorMap, image: &mut IlbmImage) -> Result<()> {
    // Having a CMAP implies certain limitations, here we limit color indices to a u8
    // so the number of planes cannot exceed 8 (bits) and the map must be big enough
    if image.planes > 8 {
        return Err(IlbmError::NotSupported("Color map with more than 8 planes".to_string()));
    }

    let Size2D(width, height) = image.size;
    let planes = image.planes;

    // Bytes per row (always EVEN)
    let row_stride = ((width + 15)/16) * 2;

    let mut rows = RowIter::new(chunk.data(), row_stride, image.compression);

    // We assemble all the resolved RGB values in here
    let mut pixels= Vec::<u8>::with_capacity(3 * width * height);

    for _row in 0..height {
        // This is the row data we are trying to assemble from planes, an array of bytes
        let mut row= vec![0u8;width];

        // Each plane gives us one bit, this one
        let mut plane_bit: u8 = 1;
        for _plane_number in 0..planes {
            let plane_data = rows.next().ok_or(IlbmError::NoData)?;

            // Read planes, each plane contributes 1 bit

            for (offset, byte) in plane_data.iter().enumerate() {
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

            // planes start at the low bit, so shift left the bit we plan to set next
            plane_bit <<= 1; 
        }

        if image.masking == Masking::HasMask {
            // Read mask plane, we don't support this yet,
            // need to go RGBA to do so
            
            // get the next row
            let _row_data = rows.next().ok_or(IlbmError::NoData)?;          
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

/// Read a body with no color map, so HAM (6 planes) or deep (24 or 32)
fn read_body_no_map(chunk: IffChunk, image: &mut IlbmImage) -> Result<()> {

    // Having no CMAP means we support up to 32 planes (although 24 is more common)
    // so we build planes into a single u32 
    if image.planes > 32 {
        return Err(IlbmError::NotSupported("Too many plans for deep color!".to_string()));
    }

    let Size2D(width, height) = image.size;
    let planes = image.planes;

    // Bytes per row (always EVEN)
    let row_stride = ((width + 15)/16) * 2;

    let mut rows = RowIter::new(chunk.data(), row_stride, image.compression);

    // We assemble all the resolved RGB values in here
    let mut pixels= Vec::<u8>::with_capacity(3 * width * height);

    for _row in 0..height {
        // This is the row data we are trying to assemble from planes, an array of 32 bit values we will interpret as RGB
        let mut row= vec![0u32;width];

        // Each plane gives us one bit, this one
        let mut plane_bit: u32 = 1;
        for _plane_number in 0..planes {
            let plane_data = rows.next().ok_or(IlbmError::NoData)?;

            // Read planes, each plane contributes 1 bit

            for (offset, byte) in plane_data.iter().enumerate() {
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

            // planes start at the low bit, so shift left the bit we plan to set next
            plane_bit <<= 1; 
        }

        if image.masking == Masking::HasMask {
            // Read mask plane, we don't support this yet,
            // need to go RGBA to do so
            
            // get the next row
            let _row_data = rows.next().ok_or(IlbmError::NoData)?;          
        }

        // Resolve without color map
        for p in row {
            let rgb = p.to_be_bytes();

            // No color map, use value as is, ignore top byte, if it's there, it's alpha
            pixels.push(rgb[3]);
            pixels.push(rgb[2]);
            pixels.push(rgb[1]);
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
            return Err(IlbmError::NoMapEntry{index, map_size})
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
        return Err(IlbmError::NoMapEntry{index:0, map_size})
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
                    return Err(IlbmError::NoMapEntry{index:low_bits, map_size})
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
            _ => unreachable!("Logically, we cannot get here, as we only have two bits")
        }

        pixels.push(color.0);
        pixels.push(color.1);
        pixels.push(color.2);
    }

    Ok(())
}

/// HalfBrite is relatively simple, we need only half the colors in the color map,
/// as one bit (from the last plane) tells us to simply half (darken) what the rest
/// of the index tells us
fn push_row_bytes_halfbrite(row: Vec<u8>, color_map: &ColorMap, pixels: &mut Vec<u8>) -> Result<()> {
    // Resolve through color map, and add to output vector, but use only half the map
    // darkening pixels in the upper half

    // Note, we rely here on having 6 planes, we already validated that
    let low_bits = 0x1f;
    let half_bit = 0x20;

    for p in row {
        let index = (p & low_bits) as usize;

        let map_size = color_map.colors.len();

        if index >= map_size  {
            return Err(IlbmError::NoMapEntry{index, map_size})
        }

        let rgb = color_map.colors[index];

        if p & half_bit != 0 {
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
        return Err(IlbmError::InvalidHeader{ 
            expected: "non-zero height and width".to_string(),
            actual: format!("{}", image.size)
        });
    }

    if image.planes == 0 {
        return Err(IlbmError::NoPlanes);
    }

    Ok(())
}
