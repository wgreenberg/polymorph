use std::collections::HashMap;

use deku::{DekuRead, DekuContainerRead};

use crate::error::Error;
use crate::tact::{btle::decode_blte, common::CKey};


#[derive(DekuRead, Clone)]
#[deku(endian = "little")]
pub struct RootFileEntry {
    pub ckey: CKey,
    pub name_hash: u64,
}

pub struct RootFile {
    pub file_id_to_ckey: HashMap<u32, RootFileEntry>,
}

#[derive(DekuRead)]
struct RootBlock {
    #[deku(endian = "little")]
    pub num_files: u32,
    #[deku(endian = "little")]
    pub content_flags: u32,
    #[deku(endian = "little")]
    pub locale_flags: u32,
    #[deku(count = "num_files")]
    pub file_id_delta_table: Vec<u32>,
    #[deku(count = "num_files")]
    pub file_entries: Vec<RootFileEntry>,
}

impl RootFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {

        let decode = decode_blte(data)?;

        let mut out = RootFile { file_id_to_ckey: HashMap::new() };
        let mut rest = &decode[..];
        loop {
            let Ok(((new_rest, _), block)) = RootBlock::from_bytes((rest, 0)) else {
                break;
            };
            rest = new_rest;

            let mut file_id = 0;
            for (file_id_delta, entry) in std::iter::zip(block.file_id_delta_table.iter(), block.file_entries.iter()) {
                file_id += file_id_delta;
                out.file_id_to_ckey.insert(file_id, entry.clone());
                file_id += 1;
            }
        }

        Ok(out)
    }

    pub fn get_ckey_for_file_id(&self, file_id: u32) -> Option<CKey> {
        self.file_id_to_ckey.get(&file_id).map(|s| s.ckey)
    }
}
