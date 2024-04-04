use std::{collections::HashMap, io::SeekFrom, os::windows::fs::MetadataExt, path::{Path, PathBuf}};

use deku::{DekuContainerRead, DekuContainerWrite, DekuRead, DekuUpdate, DekuWrite};
use tokio::{fs::{self, File}, io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}};

use crate::{error::Error, tact::blte::decode_blte};

const MAX_DATA_FILE_SIZE_BYTES: usize = 256000000;
const INDEX_FILENAME: &str = "index.shp";

pub struct SheepfileWriter {
    pub path: PathBuf,
    current_data_index: usize,
    current_data_file: File,
    entries: Vec<Entry>,
}

fn get_data_filename(index: usize) -> String {
    format!("data{}.baa", index)
}

impl SheepfileWriter {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        fs::create_dir_all(path.as_ref()).await?;
        let current_data_file = fs::File::create(path.as_ref().join(get_data_filename(0))).await?;
        Ok(SheepfileWriter {
            path: path.as_ref().to_path_buf(),
            current_data_index: 0,
            current_data_file,
            entries: Vec::new(),
        })
    }

    pub async fn append_entry(&mut self, file_id: u32, name_hash: u64, data: &[u8]) -> Result<(), Error> {
        let mut file_size = self.current_data_file.metadata().await?.file_size();
        if data.len() + file_size as usize > MAX_DATA_FILE_SIZE_BYTES {
            self.new_data_file().await?;
            file_size = 0;
        }
        self.entries.push(Entry {
            file_id,
            name_hash,
            data_file_index: self.current_data_index as u16,
            start_bytes: file_size as u32,
            size_bytes: data.len() as u32,
        });
        self.current_data_file.write_all(&data).await?;
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
        let path = self.path.join(get_data_filename(self.current_data_index));
        self.current_data_file = fs::File::create(path).await?;
        Ok(())
    }
}

pub struct SheepfileReader {
    pub entries: Vec<Entry>,
    pub file_ids_to_entry_index: HashMap<u32, usize>,
    pub name_hash_to_entry_index: HashMap<u64, usize>,
}

impl SheepfileReader {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        SheepfileReader::parse(&fs::read(path.as_ref().join(INDEX_FILENAME)).await?)
    }

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

    pub async fn get_entry_data<P: AsRef<Path>>(&self, path: P, entry: &Entry) -> Result<Vec<u8>, Error> {
        let file_path = path.as_ref().join(get_data_filename(entry.data_file_index as usize));
        let mut file = fs::File::open(file_path).await?;
        file.seek(SeekFrom::Start(entry.start_bytes as u64)).await?;
        let mut buf = vec![0; entry.size_bytes as usize];
        file.read_exact(&mut buf).await?;
        return Ok(buf)
    }
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
