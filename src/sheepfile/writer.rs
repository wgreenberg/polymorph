use std::path::{Path, PathBuf};

use deku::DekuContainerWrite;
use tokio::{fs::{self, File}, io::AsyncWriteExt};

use crate::{error::Error, sheepfile::{get_data_filename, Entry, Index, INDEX_FILENAME}};

const MAX_DATA_FILE_SIZE_BYTES: usize = 256000000;

pub struct SheepfileWriter {
    pub path: PathBuf,
    current_data_index: usize,
    current_data_file: File,
    current_data_file_size: usize,
    entries: Vec<Entry>,
}

impl SheepfileWriter {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        fs::create_dir_all(path.as_ref()).await?;
        let current_data_file = fs::File::create(path.as_ref().join(get_data_filename(0))).await?;
        Ok(SheepfileWriter {
            path: path.as_ref().to_path_buf(),
            current_data_index: 0,
            current_data_file_size: 0,
            current_data_file,
            entries: Vec::new(),
        })
    }

    pub async fn append_entry(&mut self, file_id: u32, name_hash: u64, data: &[u8]) -> Result<(), Error> {
        if data.len() + self.current_data_file_size > MAX_DATA_FILE_SIZE_BYTES {
            self.new_data_file().await?;
        }
        self.entries.push(Entry {
            file_id,
            name_hash,
            data_file_index: self.current_data_index as u16,
            start_bytes: self.current_data_file_size as u32,
            size_bytes: data.len() as u32,
        });
        self.current_data_file.write_all(&data).await?;
        self.current_data_file_size += data.len();
        Ok(())
    }

    pub async fn finish(self) -> Result<(), Error> {
        let mut index_file = fs::File::create(self.path.join(INDEX_FILENAME)).await?;
        let index = Index {
            num_entries: self.entries.len() as u32,
            entries: self.entries
        };
        index_file.write_all(&index.to_bytes().unwrap()).await?;
        Ok(())
    }

    async fn new_data_file(&mut self) -> Result<(), Error> {
        self.current_data_index += 1;
        self.current_data_file_size = 0;
        let path = self.path.join(get_data_filename(self.current_data_index));
        self.current_data_file = fs::File::create(path).await?;
        Ok(())
    }
}
