use std::io::{BufRead, BufReader, Cursor};
use std::fmt;
use std::convert::TryInto;

#[derive(PartialEq, Copy, Clone, Debug)]
pub struct ChunkId (
    pub [u8;4]
);

impl PartialEq<[u8;4]> for ChunkId {
    fn eq(&self, other: &[u8;4]) -> bool {
        self.0 == *other
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl Default for ChunkId {
    fn default() -> Self { ChunkId([63;4]) } // b"????"
}

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
    pub fn id(&self) -> ChunkId { self.ck_id }
    pub fn data(&self) -> &[u8] { &self.data }
    pub fn is_form(&self) -> bool { &self.ck_id == b"FORM" }
    pub fn form_type(&self) -> ChunkId { 
        if self.is_form() && self.data.len() >= 4 {
            // Since we validated the length, we can safely unwrap here
            ChunkId(self.data[..4].try_into().unwrap())
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
            write!(f, "FORM Chunk {}, length {}", self.form_type(), self.data.len())
        } else {
            write!(f, "{} Chunk, length {}", self.ck_id, self.data.len())
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

        debug!("Found Chunk {} {}", ck_id, len);

        let mut data = vec![0u8; len as usize];

        if self.reader.read_exact(&mut data).is_err() {
            return None;
        }

        self.position += (len as u64) + 8;

        Some(IffChunk{ck_id, data})
    }
}

