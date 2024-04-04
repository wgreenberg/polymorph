use std::collections::HashMap;

use deku::DekuContainerRead;

use crate::{error::Error, sheepfile::{Entry, Index}};


pub struct SheepfileReader {
    pub entries: Vec<Entry>,
    pub file_ids_to_entry_index: HashMap<u32, usize>,
    pub name_hash_to_entry_index: HashMap<u64, usize>,
}

impl SheepfileReader {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let (_, index) = Index::from_bytes((data, 0))?;
        let mut file_ids_to_entry_index = HashMap::new();
        let mut name_hash_to_entry_index = HashMap::new();
        for (i, entry) in index.entries.iter().enumerate() {
            file_ids_to_entry_index.insert(entry.file_id, i);
            name_hash_to_entry_index.insert(entry.name_hash, i);
        }
        Ok(SheepfileReader {
            entries: index.entries,
            file_ids_to_entry_index,
            name_hash_to_entry_index,
        })
    }

    pub fn get_entry_for_file_id(&self, file_id: u32) -> Option<&Entry> {
        let index = *self.file_ids_to_entry_index.get(&file_id)?;
        Some(&self.entries[index])
    }

    pub fn get_entry_for_name(&self, name: &str) -> Option<&Entry> {
        let normalized = name.to_ascii_uppercase().replace("/", "\\");
        let name_hash = hashers::jenkins::lookup3(normalized.as_bytes());
        let index = *self.name_hash_to_entry_index.get(&name_hash)?;
        Some(&self.entries[index])
    }
}
