use std::collections::HashMap;

use deku::{DekuRead, DekuContainerRead};

use crate::error::Error;
use crate::tact::{btle::decode_blte, common::CKey};


#[derive(DekuRead, Clone)]
pub struct RootFileEntry {
    pub ckey: CKey,
    #[deku(endian = "little")]
    pub name_hash: u64,
}

#[derive(DekuRead)]
struct RootBlock {
    #[deku(endian = "little")]
    _num_files: u32,
    #[deku(endian = "little")]
    _content_flags: u32,
    #[deku(endian = "little")]
    _locale_flags: u32,
    #[deku(count = "_num_files")]
    file_id_delta_table: Vec<u32>,
    #[deku(count = "_num_files")]
    file_entries: Vec<RootFileEntry>,
}

#[derive(Clone)]
pub struct RootFile {
    pub entries: Vec<RootFileEntry>,
    pub file_id_to_entry_index: HashMap<u32, usize>,
    pub name_hash_to_entry_index: HashMap<u64, usize>,
}

impl RootFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {

        let decode = decode_blte(data)?;

        let mut entries = Vec::new();
        let mut file_id_to_entry_index = HashMap::new();
        let mut name_hash_to_entry_index = HashMap::new();
        let mut rest = &decode[..];
        loop {
            let Ok(((new_rest, _), block)) = RootBlock::from_bytes((rest, 0)) else {
                break;
            };
            rest = new_rest;

            let mut file_id = 0;
            for (file_id_delta, entry) in std::iter::zip(block.file_id_delta_table, block.file_entries) {
                file_id += file_id_delta;
                file_id_to_entry_index.insert(file_id, entries.len());
                name_hash_to_entry_index.insert(entry.name_hash, entries.len());
                file_id += 1;
                entries.push(entry);
            }
        }

        Ok(RootFile {
            entries,
            file_id_to_entry_index,
            name_hash_to_entry_index,
        })
    }

    fn get_entry_ckey(&self, entry_index: usize) -> &CKey {
        &self.entries[entry_index].ckey
    }

    pub fn get_ckey_for_file_id(&self, file_id: u32) -> Option<&CKey> {
        self.file_id_to_entry_index.get(&file_id).map(|index| self.get_entry_ckey(*index))
    }
    
    // from https://wowdev.wiki/TACT#hashpath
    pub fn get_ckey_for_file_path(&self, name: &str) -> Option<&CKey> {
        let normalized = name.to_ascii_uppercase().replace("/", "\\");
        let name_hash = hashers::jenkins::lookup3(normalized.as_bytes());
        self.name_hash_to_entry_index.get(&name_hash).map(|index| self.get_entry_ckey(*index))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_file() {
        let test_file = std::fs::read("./test/root.out").unwrap();

        let file = RootFile::parse(&test_file).unwrap();
        dbg!(file.file_id_to_entry_index.len());
    }
}
