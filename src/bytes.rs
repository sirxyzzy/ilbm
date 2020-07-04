use crate::{Error, Result};
use std::convert::TryInto;

//
// Note, this module is heavily inspired by the "bytes" crate, but differs
// mainly in being panic free, instead returning results.
// It is also a tad faster, I like fast
//

/// Expose interface as a Trait, doing thing this way leads to a more concise
/// calling interface
pub trait BigEndian {
    fn get_u8(&mut self) -> Result<u8>;
    fn get_u16(&mut self) -> Result<u16>;
    fn get_i8(&mut self) -> Result<i8>;    
    fn get_i16(&mut self) -> Result<i16>;
    fn get_u32(&mut self) -> Result<u32>;    
}

impl BigEndian for &[u8] {
    fn get_u8(&mut self) -> Result<u8>   {
        if self.len() < std::mem::size_of::<u8>() {
            return Err(Error::NoData);
        }

        let (bytes, rest) = self.split_at(std::mem::size_of::<u8>());
        *self = rest;

        // unwrap here should be safe, as I check the length above
        Ok(bytes[0])
    }

    fn get_i8(&mut self) -> Result<i8>   {
        if self.len() < std::mem::size_of::<i8>() {
            return Err(Error::NoData);
        }

        let (bytes, rest) = self.split_at(std::mem::size_of::<i8>());
        *self = rest;

        // unwrap here should be safe, as I check the length above
        Ok(bytes[0] as i8)
    }

    fn get_u16(&mut self) -> Result<u16>   {
        if self.len() < std::mem::size_of::<u16>() {
            return Err(Error::NoData);
        }
    
        let (bytes, rest) = self.split_at(std::mem::size_of::<u16>());
        *self = rest;
    
        // unwrap here should be safe, as I check the length above
        Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn get_i16(&mut self) -> Result<i16>   {
        if self.len() < std::mem::size_of::<i16>() {
            return Err(Error::NoData);
        }
    
        let (bytes, rest) = self.split_at(std::mem::size_of::<i16>());
        *self = rest;
    
        // unwrap here should be safe, as I check the length above
        Ok(i16::from_be_bytes(bytes.try_into().unwrap()))
    }
    
    fn get_u32(&mut self) -> Result<u32>   {
        if self.len() < std::mem::size_of::<u32>() {
            return Err(Error::NoData);
        }
    
        let (bytes, rest) = self.split_at(std::mem::size_of::<u32>());
        *self = rest;
    
        // unwrap here should be safe, as I check the length above
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }
}
