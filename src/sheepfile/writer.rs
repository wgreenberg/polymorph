use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}};

use deku::DekuContainerWrite;
use log::{error, info};
use tokio::{fs::{self, File}, io::AsyncWriteExt};

use crate::{cdn::CDNFetcher, error::Error, sheepfile::{get_data_filename, Entry, Index, INDEX_FILENAME}, tact::{archive::{ArchiveIndex, ArchiveIndexEntry}, blte::decode_blte}};

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

    pub async fn write_cdn_files(mut self, cdns: &[&mut CDNFetcher]) -> Result<(), Error> {
        let mut all_entries: Vec<(u32, u64, &ArchiveIndexEntry, &ArchiveIndex, &&mut CDNFetcher)> = Vec::new();
        let mut all_file_ids = HashSet::new();
        for cdn in cdns {
            let mut archive_to_entries: HashMap<&str, (&ArchiveIndex, Vec<(u32, u64, &ArchiveIndexEntry, &ArchiveIndex, &&mut CDNFetcher)>)> = HashMap::new();
            for (&file_id, &index) in cdn.root.file_id_to_entry_index.iter() {
                if all_file_ids.contains(&file_id) {
                    continue;
                }
                let root_entry = &cdn.root.entries[index];
                let Some(ekey) = cdn.encoding.get_ekey_for_ckey(&root_entry.ckey) else {
                    error!("skipping file id {}, couldn't find ekey", file_id);
                    continue;
                };
                let Some((archive, archive_entry)) = cdn.find_archive_entry(ekey) else {
                    error!("skipping file id {}, couldn't find archive entry", file_id);
                    continue;
                };
                let (_, entries) = archive_to_entries.entry(&archive.key).or_insert((archive, Vec::new()));
                entries.push((file_id, root_entry.name_hash, archive_entry, &archive, cdn));
                all_file_ids.insert(file_id);
            }

            let n_archives = archive_to_entries.len();
            for (i, (archive, entries)) in archive_to_entries.into_values().enumerate() {
                let index_entries: Vec<&ArchiveIndexEntry> = entries.iter().map(|entry| entry.2).collect();
                info!("[{}/{}] fetching archive {} (contains {} entries)...", i, n_archives, &archive.key, index_entries.len());
                let _ = cdn.cache.fetch_archive_entries(&cdn.hosts[0], archive, index_entries.as_slice()).await?;
                all_entries.extend(entries);
            }
        }

        info!("writing {} fileIDs to sheepfile...", all_entries.len());
        all_entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (file_id, name_hash, archive_entry, archive, cdn) in all_entries {
            let data = cdn.cache.fetch_archive_entry(&cdn.hosts[0], archive, archive_entry).await?;
            match decode_blte(&data) {
                Ok(uncompressed_data) => self.append_entry(file_id, name_hash, &uncompressed_data).await?,
                Err(Error::UnsupportedEncryptedData) => {
                    info!("file {} contains encrypted data, skipping", file_id);
                    continue;
                },
                Err(e) => return Err(e),
            }
        }

        self.finish().await
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
