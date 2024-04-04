
use deku::{DekuContainerWrite, DekuRead, DekuUpdate, DekuWrite};

#[cfg(feature = "sheepfile-reader")]
pub mod reader;

#[cfg(feature = "sheepfile-writer")]
pub mod writer;

pub const INDEX_FILENAME: &str = "index.shp";

pub fn get_data_filename(index: usize) -> String {
    format!("data{}.baa", index)
}

#[derive(DekuRead, DekuWrite)]
pub struct Index {
    pub num_entries: u32,
    #[deku(count = "num_entries")]
    pub entries: Vec<Entry>,
}

#[derive(DekuRead, DekuWrite, Debug, Clone)]
pub struct Entry {
    pub file_id: u32,
    pub name_hash: u64,
    pub data_file_index: u16,
    pub start_bytes: u32,
    pub size_bytes: u32,
}
