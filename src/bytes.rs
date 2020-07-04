use crate::{Error, Result};
use std::convert::TryInto;

pub trait BigEndian {
    fn get_u8(&mut self) -> Result<u8>;
    fn get_u16(&mut self) -> Result<u16>;
    fn get_i8(&mut self) -> Result<i8>;    
    fn get_i16(&mut self) -> Result<i16>;    
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
}
