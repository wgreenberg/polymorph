use deku::{DekuRead, DekuContainerRead};
use miniz_oxide::inflate::decompress_to_vec_zlib;

use crate::error::Error;

#[derive(DekuRead, Debug)]
pub struct BLTEChunk {
    #[deku(endian = "big")]
    pub compressed_size: u32,
    #[deku(endian = "big")]
    pub uncompressed_size: u32,
    #[deku(endian = "big")]
    pub checksum: [u8; 0x10],
}

#[derive(DekuRead, Debug)]
#[deku(magic = b"BLTE")]
pub struct BLTEHeader {
    #[deku(endian = "big")]
    pub data_offset: u32,
    pub flag: u8,
    #[deku(endian = "big", bytes = 3)]
    pub chunk_count: u32,
    #[deku(count = "chunk_count")]
    pub chunks: Vec<BLTEChunk>,
}

pub fn decode_blte(buf: &[u8]) -> Result<Vec<u8>, Error> {
    let header = BLTEHeader::from_bytes((buf, 0))?.1;
    let mut out = Vec::new();

    let mut data_offs = header.data_offset as usize;
    for chunk in &header.chunks {
        let chunk_buf = &buf[data_offs .. data_offs + (chunk.compressed_size as usize)];
        let frame_type = chunk_buf[0] as char;
        let chunk_data = &chunk_buf[1 .. chunk_buf.len()];
        match frame_type {
            'N' => out.extend(chunk_data),
            'Z' => out.extend(decompress_to_vec_zlib(chunk_data).map_err(Error::ZlibError)?),
            c => panic!("{}", c),
        }
        data_offs += chunk.compressed_size as usize;
    }

    Ok(out)
}
