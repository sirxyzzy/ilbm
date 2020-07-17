use std::io::Cursor;
use std::fmt;
use std::io::prelude::*;

const FORM: ChunkId = ChunkId::new(b"FORM");

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct ChunkId (
    [u8;4]
);

impl ChunkId {
    pub const fn new(id: &[u8;4]) -> ChunkId {
        ChunkId(*id)
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl Default for ChunkId {
    fn default() -> Self { ChunkId::new(b"????") }
}

pub struct IffReader<R> {
    reader: R,
    skip: bool
}

impl<R:Read> IffReader<R> {
    pub fn new(reader: R) -> IffReader<R> {
        IffReader{reader, skip: false}
    }
}

pub struct IffChunk {
    ck_id: ChunkId,
    data: Vec<u8>
}

impl IffChunk {
    pub fn id(&self) -> ChunkId { self.ck_id }
    pub fn data(&self) -> &[u8] { &self.data }
    pub fn is_form(&self) -> bool { 
        self.ck_id == FORM  
    }
    pub fn is_form_type(&self, form_type:&[u8;4]) -> bool { 
        self.is_form() && self.data.len() >= 4 && form_type == &self.data[..4]  
    }
    pub fn sub_chunks(&self) -> IffReader<Cursor<&[u8]>> {
         IffReader::new( Cursor::new(&self.data[4..]))
    } 
}

impl fmt::Display for IffChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_form() && self.data.len() >= 4 {
            write!(f, "FORM Chunk {}, length {}", String::from_utf8_lossy(&self.data[..4]), self.data.len())
        } else {
            write!(f, "{} Chunk, length {}", self.ck_id, self.data.len())
        }
    }
}

impl<R: Read> Iterator for IffReader<R> {
    type Item = IffChunk;
    fn next(&mut self) -> Option<IffChunk> {
        // Do we need a padding byte, due to a previous odd read
        if self.skip {
            // Throw away the padding byte
            let mut dummy = [0u8; 1];
        
            if self.reader.read_exact(&mut dummy).is_err() {
                return None;
            }
        }

        let mut id = [0u8; 4];

        if self.reader.read_exact(&mut id).is_err() {
            return None;
        }

        let ck_id = ChunkId(id);

        let mut len_bytes = [0u8; 4];

        if self.reader.read_exact(&mut len_bytes).is_err() {
            return None;
        }

        let len = u32::from_be_bytes(len_bytes);

        // If we get an odd size, we need to skip a trailing byte,
        // before we fetch the next chunk, so take note of that
        self.skip = len & 1 != 0;

        debug!("Found Chunk {} {}", ck_id, len);

        let mut data = vec![0u8; len as usize];

        if self.reader.read_exact(&mut data).is_err() {
            return None;
        }

        Some(IffChunk{ck_id, data})
    }
}

