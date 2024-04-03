use std::{collections::HashMap, path::Path, str::FromStr};

use deku::{DekuContainerRead, DekuContainerWrite, DekuRead, DekuUpdate, DekuWrite};
use tokio::fs;

use crate::{error::Error, tact::common::EKey};

#[derive(Default)]
pub struct FileDb {
    pub file_id_to_file_info_index: HashMap<u32, usize>,
    pub name_hash_to_file_info_index: HashMap<u64, usize>,
    pub file_infos: Vec<FileInfo>,
}

impl FileDb {
    pub fn new() -> Self {
        FileDb::default()
    }

    pub fn append(&mut self, file_id: u32, name_hash: u64, archive_key_str: &str, start_bytes: u32, size_bytes: u32) {
        let EKey(archive_key) = EKey::from_str(archive_key_str).unwrap();
        let info = FileInfo {
            file_id,
            name_hash,
            archive_key,
            start_bytes,
            size_bytes,
        };
        self.file_id_to_file_info_index.insert(file_id, self.file_infos.len());
        self.name_hash_to_file_info_index.insert(name_hash, self.file_infos.len());
        self.file_infos.push(info);
    }

    pub async fn write_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let serialized = SerializedFileInfos {
            num_files: self.file_infos.len() as u32,
            files: self.file_infos.clone(),
        };
        fs::write(path, serialized.to_bytes()?).await?;
        Ok(())
    }

    pub async fn parse(data: &[u8]) -> Result<Self, Error> {
        let (_, serialized) = SerializedFileInfos::from_bytes((data, 0))?;
        let mut db = FileDb::new();
        for file in serialized.files {
            db.file_id_to_file_info_index.insert(file.file_id, db.file_infos.len());
            db.name_hash_to_file_info_index.insert(file.name_hash, db.file_infos.len());
            db.file_infos.push(file);
        }
        Ok(db)
    }
}

#[derive(DekuRead, DekuWrite)]
pub struct SerializedFileInfos {
    pub num_files: u32,
    #[deku(count = "num_files")]
    pub files: Vec<FileInfo>,
}

#[derive(DekuRead, DekuWrite, Debug, Clone)]
pub struct FileInfo {
    pub file_id: u32,
    pub name_hash: u64,
    pub archive_key: [u8; 16],
    pub start_bytes: u32,
    pub size_bytes: u32,
}

impl FileInfo {
    pub fn archive_key_str(&self) -> &str {
        std::str::from_utf8(&self.archive_key).unwrap()
    }
}
