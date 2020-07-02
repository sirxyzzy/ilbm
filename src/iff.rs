use std::io::{Read, Seek, SeekFrom, BufRead, BufReader, Cursor, Result};
use std::fmt;
use std::convert::TryInto;
use crate::Error;

type ChunkId = [u8;4];

pub struct IffReader<R: BufRead> {
    reader: R,
    position: u64
}

impl<R: BufRead> IffReader<R> {
    pub fn new(reader: R) -> IffReader<R> {
        IffReader{reader, position: 0}
    }
}

pub struct IffChunk {
    ck_id: ChunkId,
    data: Vec<u8>
}

impl IffChunk {
    pub fn id(&self) -> &ChunkId { &self.ck_id }
    pub fn data(&self) -> &[u8] { &self.data }
    pub fn is_form(&self) -> bool { &self.ck_id == b"FORM" }
    pub fn form_type(&self) -> &ChunkId { 
        if self.is_form() && self.data.len() >= 4 {
            self.data[..4].try_into().unwrap()
        } else {
            panic!("Ouch, where is my form type!");
        }
    }
    pub fn sub_chunks(self) -> IffReader<BufReader<Cursor<Vec<u8>>>> {
        let mut reader = BufReader::new(Cursor::new(self.data));
        reader.seek_relative(4).unwrap();
        IffReader{reader, position: 4}
    } 
}

impl fmt::Display for IffChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_form() && self.data.len() >= 4 {
            write!(f, "FORM Chunk {}, length {}", String::from_utf8_lossy(&self.data[..4]), self.data.len())
        } else {
            write!(f, "{} Chunk, length {}", String::from_utf8_lossy(&self.ck_id), self.data.len())
        }
    }
}

impl<R: BufRead> Iterator for IffReader<R> {
    type Item = IffChunk;
    fn next(&mut self) -> Option<IffChunk> {
        if self.position&1 == 1 {
            // Read and throw away a padding byte
            let mut dummy = [0u8; 1];
        
            if self.reader.read_exact(&mut dummy).is_err() {
                return None;
            }
        }

        let mut ck_id = [0u8; 4];

        if self.reader.read_exact(&mut ck_id).is_err() {
            return None;
        }

        let mut len_bytes = [0u8; 4];

        if self.reader.read_exact(&mut len_bytes).is_err() {
            return None;
        }

        let len = u32::from_be_bytes(len_bytes);

        info!("Found Chunk {} {}", String::from_utf8_lossy(&ck_id), len);

        let mut data = vec![0u8; len as usize];

        if self.reader.read_exact(&mut data).is_err() {
            return None;
        }

        self.position += (len as u64) + 8;

        Some(IffChunk{ck_id, data})
    }
}

