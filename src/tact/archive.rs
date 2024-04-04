use std::collections::HashMap;
use std::ops::Range;

use deku::{DekuRead, DekuContainerRead};

use crate::{error::Error, tact::common::EKey};
use crate::tact::common::NULL_EKEY;

#[derive(DekuRead, Debug)]
pub struct ArchiveIndexFooter {
    pub toc_hash: [u8; 0x10],
    #[deku(assert_eq = "1")]
    pub version: u8,
    #[deku(pad_bytes_before = "2", assert_eq = "4")]
    pub block_size_kb: u8,
    #[deku(assert_eq = "4")]
    pub offset_bytes: u8,
    #[deku(assert_eq = "4")]
    pub size_bytes: u8,
    #[deku(assert_eq = "16")]
    pub key_size_in_bytes: u8,
    #[deku(assert_eq = "8")]
    pub checksum_size: u8,
    pub num_files: u32,
}

#[derive(Clone)]
pub struct ArchiveIndex {
    pub entries: HashMap<EKey, ArchiveIndexEntry>,
    pub key: String,
}

#[derive(DekuRead, Clone)]
pub struct ArchiveIndexEntry {
    pub ekey: EKey,
    #[deku(endian = "big")]
    pub size_bytes: u32,
    #[deku(endian = "big")]
    pub offset_bytes: u32,
}

impl ArchiveIndexEntry {
    pub fn get_byte_range(&self) -> Range<usize> {
        let start = self.offset_bytes as usize;
        let end = start + self.size_bytes as usize;
        start..end
    }
}

impl ArchiveIndex {
    pub fn parse(key: &str, data: &[u8]) -> Result<Self, Error> {
        let mut entries: HashMap<EKey, ArchiveIndexEntry> = HashMap::new();
        let footer_offset = data.len() - 0x24;
        let (_, footer): (_, ArchiveIndexFooter) = ArchiveIndexFooter::from_bytes((&data[footer_offset..], 0))?;

        let block_size = (footer.block_size_kb as usize) << 10;
        let mut num_files = 0;
        let mut block_start = 0;
        loop {
            let block_end = block_start + block_size;
            let mut block_data = &data[block_start..block_end];
            loop {
                let Ok(((new_block_data, _), entry)) = ArchiveIndexEntry::from_bytes((block_data, 0)) else {
                    break;
                };

                block_data = new_block_data;
                if entry.ekey == NULL_EKEY {
                    break;
                }

                entries.insert(entry.ekey.clone(), entry);
                num_files += 1;
            }

            block_start += block_size;

            if num_files >= footer.num_files {
                break;
            }
        }

        Ok(ArchiveIndex {
            entries,
            key: key.into(),
        })
    }

    pub fn get_entry_for_ekey(&self, ekey: &EKey) -> Option<&ArchiveIndexEntry> {
        self.entries.get(ekey)
    }
}
