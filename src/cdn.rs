use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::io::SeekFrom;

use log::{debug, error, info};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::Error;
use crate::file_db::FileDb;
use crate::tact::archive::{ArchiveIndex, ArchiveIndexEntry};
use crate::tact::btle::decode_blte;
use crate::tact::common::{CKey, EKey};
use crate::tact::encoding::EncodingFile;
use crate::tact::manifest::Manifest;
use crate::tact::root::RootFile;

#[derive(Clone)]
pub struct CDNHost {
    pub host: String,
    pub path: String,
}

impl CDNHost {
    pub fn new(host: &str, path: &str) -> Self {
        CDNHost {
            host: host.to_string(),
            path: path.to_string(),
        }
    }

    pub fn make_url(&self, key: &str, extra_path: &str) -> String {
        format!(
            "http://{}/{}/{}/{}/{}/{}",
            self.host,
            self.path,
            extra_path,
            &key[0..2],
            &key[2..4],
            key,
        )
    }
}

async fn read_or_cache<P: AsRef<Path>>(file_path: P, url: &str, maybe_range: Option<Range<usize>>) -> Result<Vec<u8>, Error> {
    match fs::try_exists(&file_path).await {
        Ok(true) => {
            debug!("cache: found {:?}", file_path.as_ref());
            if let Some(Range { start, end }) = maybe_range {
                let mut file = fs::File::open(file_path).await?;
                file.seek(SeekFrom::Start(start as u64)).await?;
                let mut buf = vec![0; end - start];
                file.read_exact(&mut buf).await?;
                Ok(buf)
            } else {
                Ok(fs::read(file_path).await?)
            }
        },
        _ => {
            debug!("cache: didn't find {:?}, requesting {}", file_path.as_ref(), &url);
            let buf = reqwest::get(url)
                .await?
                .bytes()
                .await?;
            fs::create_dir_all(file_path.as_ref().parent().unwrap())
                .await?;
            fs::write(file_path, &buf).await?;
            if let Some(Range { start, end }) = maybe_range {
                Ok(buf[start..end].to_vec())
            } else {
                Ok(buf.to_vec())
            }
        },
    }
}

#[derive(Clone)]
pub struct BlizzCache {
    pub cache_path: PathBuf,
    pub patch_server: String,
    pub product: String,
}

impl BlizzCache {
    pub fn new<P: AsRef<Path>>(cache_path: P, patch_server: &str, product: &str) -> Self {
        BlizzCache {
            cache_path: cache_path.as_ref().to_path_buf(),
            patch_server: patch_server.into(),
            product: product.into(),
        }
    }

    pub async fn fetch_data(&self, host: &CDNHost, directory: &str, key: &str) -> Result<Vec<u8>, Error> {
        let mut file_path = self.cache_path.join(directory);
        file_path.push(key);
        read_or_cache(file_path, &host.make_url(key, directory), None).await
    }

    pub async fn fetch_archive(&self, host: &CDNHost, archive: &ArchiveIndex) -> Result<Vec<u8>, Error> {
        let mut filename = self.cache_path.join("data");
        filename.push(&archive.key);
        read_or_cache(filename, &host.make_url(&archive.key, "data"), None).await
    }

    pub async fn fetch_archive_entry(&self, host: &CDNHost, archive: &ArchiveIndex, entry: &ArchiveIndexEntry) -> Result<Vec<u8>, Error> {
        let mut filename = self.cache_path.join("data");
        filename.push(&archive.key);
        let range = entry.offset_bytes as usize..entry.offset_bytes as usize + entry.size_bytes as usize;
        read_or_cache(filename, &host.make_url(&archive.key, "data"), Some(range)).await
    }
    
    async fn fetch_manifest(&self, manifest_name: &str) -> Result<Vec<u8>, Error> {
        let url = format!("{}/{}/{}", self.patch_server, self.product, manifest_name);
        let mut filename = self.cache_path.join("patch_server");
        filename.push(&self.product);
        filename.push(&manifest_name);
        read_or_cache(filename, &url, None).await
    }
}

#[derive(Clone)]
pub struct CDNFetcher {
    pub hosts: Vec<CDNHost>,
    pub archive_index: Vec<ArchiveIndex>,
    pub root: RootFile,
    pub cache: BlizzCache,
    pub encoding: EncodingFile,
    pub versions: Manifest,
    pub cdns: Manifest,
    pub cdn_config: HashMap<String, Vec<String>>,
    pub build_config: HashMap<String, Vec<String>>,
}

impl CDNFetcher {
    pub async fn init<P: AsRef<Path>>(cache_path: P, patch_server: &str, product: &str, region: &str) -> Result<Self, Error> {
        info!("intializing cache at {:?}", cache_path.as_ref());
        let cache = BlizzCache::new(cache_path, patch_server, product);

        info!("loading versions manifest");
        let versions = Manifest::parse(&cache.fetch_manifest("versions").await?)?;
        info!("loading CDNs manifest");
        let cdns = Manifest::parse(&cache.fetch_manifest("cdns").await?)?;

        let cdn_row = cdns.find_row("Name", region).unwrap();
        let path = cdns.get_field(cdn_row, "Path").unwrap();
        let hosts: Vec<CDNHost> = cdns.get_field(cdn_row, "Hosts").unwrap()
            .split_whitespace()
            .map(|host| CDNHost::new(host, path))
            .collect();

        let version_row = versions.find_row("Region", region).unwrap();
        let build_config_key = versions.get_field(version_row, "BuildConfig").unwrap();
        let cdn_config_key = versions.get_field(version_row, "CDNConfig").unwrap();

        info!("fetching CDN config");
        let cdn_config = parse_config(&String::from_utf8(cache.fetch_data(&hosts[0], "config", cdn_config_key).await?).expect("invalid config"));
        info!("fetching build config");
        let build_config = parse_config(&String::from_utf8(cache.fetch_data(&hosts[0], "config", build_config_key).await?).expect("invalid config"));

        info!("fetching encoding file");
        let encoding_key = &build_config.get("encoding").unwrap()[1];
        let encoding = EncodingFile::parse(&cache.fetch_data(&hosts[0], "data", encoding_key).await?)?;

        let archive_keys = cdn_config.get("archives").unwrap();
        let mut archive_index = Vec::new();
        for (i, archive_key) in archive_keys.iter().enumerate() {
            info!("[{}/{}] fetching archive index {}...", i, archive_keys.len(), archive_key);
            let archive_data = cache.fetch_data(&hosts[0], "data", &format!("{}.index", archive_key)).await?;
            archive_index.push(ArchiveIndex::parse(archive_key, &archive_data)?);
        }

        info!("fetching root file");
        let root_ckey: CKey = CKey::from_str(&build_config.get("root").unwrap()[0]).unwrap();
        let root_ekey = &encoding.get_ekey_for_ckey(&root_ckey).unwrap().to_string();
        let root_data = cache.fetch_data(&hosts[0], "data", root_ekey).await?;
        let root = RootFile::parse(&root_data)?;

        Ok(CDNFetcher {
            hosts,
            archive_index,
            root,
            cache,
            encoding,
            versions,
            cdns,
            cdn_config,
            build_config,
        })
    }

    pub fn build_file_db(&self) -> FileDb {
        let mut db = FileDb::new();
        for (&file_id, &index) in self.root.file_id_to_entry_index.iter() {
            let root_entry = &self.root.entries[index];
            let Some(ekey) = self.encoding.get_ekey_for_ckey(&root_entry.ckey) else {
                error!("couldn't find ekey for file id {}", file_id);
                continue;
            };
            let Some((archive, archive_entry)) = self.find_archive_entry(ekey) else {
                error!("couldn't find archive entry for file id {}", file_id);
                continue;
            };
            db.append(file_id, root_entry.name_hash, &archive.key, archive_entry.offset_bytes, archive_entry.size_bytes);
        }
        db
    }

    pub fn find_archive_entry(&self, ekey: &EKey) -> Option<(&ArchiveIndex, &ArchiveIndexEntry)> {
        for index in &self.archive_index {
            if let Some(entry) = index.get_entry_for_ekey(ekey) {
                return Some((index, entry));
            }
        }
        None
    }

    pub async fn fetch_archive(&self, archive: &ArchiveIndex) -> Result<Vec<u8>, Error> {
        let data = self.cache.fetch_archive(&self.hosts[0], archive).await?;
        Ok(data)
    }

    pub async fn fetch_ckey_from_archive(&self, ckey: &CKey) -> Result<Option<Vec<u8>>, Error> {
        let Some(ekey) = self.encoding.get_ekey_for_ckey(ckey) else {
            return Ok(None);
        };
        let Some((archive, entry)) = self.find_archive_entry(ekey) else {
            return Ok(None);
        };
        let data = self.cache.fetch_archive_entry(&self.hosts[0], archive, entry).await?;
        Ok(Some(data))
    }

    pub async fn fetch_file_id(&self, file_id: u32) -> Result<Vec<u8>, Error> {
        let ckey = self.root.get_ckey_for_file_id(file_id).ok_or(Error::MissingFileId(file_id))?;
        let compressed_data = self.fetch_ckey_from_archive(ckey).await?.ok_or(Error::MissingCKey)?;
        decode_blte(&compressed_data)
    }

    pub async fn fetch_file_name(&self, path: &str) -> Result<Vec<u8>, Error> {
        let ckey = self.root.get_ckey_for_file_path(path).ok_or(Error::MissingFilePath(path.to_string()))?;
        let compressed_data = self.fetch_ckey_from_archive(ckey).await?.ok_or(Error::MissingCKey)?;
        decode_blte(&compressed_data)
    }
}

fn parse_config(data: &str) -> HashMap<String, Vec<String>> {
    let mut result = HashMap::new();
    for line in data.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue
        }

        let (k, v) = line.split_once(" = ").expect("invalid line");
        result.insert(k.to_string(), v.split(' ').map(|s| s.to_string()).collect());
    }
    result
}
