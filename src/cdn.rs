use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::{debug, info};
use tokio::fs;

use crate::error::Error;
use crate::parse_config;
use crate::tact::archive::{ArchiveIndex, ArchiveIndexEntry};
use crate::tact::btle::decode_blte;
use crate::tact::common::{hexstring, hexunstring, CKey, EKey};
use crate::tact::encoding::EncodingFile;
use crate::tact::manifest::Manifest;
use crate::tact::root::RootFile;
use crate::util::fetch_data_fragment;

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

pub struct CDNCache {
    pub cache_path: PathBuf,
}

impl CDNCache {
    pub fn new<P: AsRef<Path>>(cache_path: P) -> Self {
        CDNCache {
            cache_path: cache_path.as_ref().to_path_buf(),
        }
    }

    pub async fn fetch_data(&self, host: &CDNHost, directory: &str, key: &str) -> Result<Vec<u8>, Error> {
        let mut file_path = self.cache_path.join(directory);
        file_path.push(key);
        match fs::try_exists(&file_path).await {
            Ok(true) => Ok(fs::read(file_path).await?),
            _ => {
                debug!("fetching url {}", host.make_url(key, directory));
                let buf = reqwest::get(host.make_url(key, directory))
                    .await?
                    .bytes()
                    .await?;
                fs::create_dir_all(file_path.parent().unwrap())
                    .await?;
                fs::write(file_path, &buf).await?;
                Ok(buf.to_vec())
            },
        }
    }

    pub async fn fetch_archive(&self, host: &CDNHost, archive: &ArchiveIndex) -> Result<Vec<u8>, Error> {
        let mut filename = self.cache_path.join("data");
        filename.push(&archive.key);
        if let Ok(true) = fs::try_exists(&filename).await {
            return Ok(fs::read(&filename).await?);
        }

        info!("archive {:?} missing, fetching...", &filename);
        let buf = reqwest::get(host.make_url(&archive.key, "data"))
            .await?
            .bytes()
            .await?;
        fs::write(&filename, &buf).await?;
        Ok(buf.into())
    }

    pub async fn fetch_archive_entry(&self, host: &CDNHost, archive: &ArchiveIndex, entry: &ArchiveIndexEntry) -> Result<Vec<u8>, Error> {
        let mut filename = self.cache_path.join("data");
        filename.push(&archive.key);
        if let Ok(true) = fs::try_exists(&filename).await {
            info!("found archive {:?}", &filename);
            return fetch_data_fragment(&filename, entry.offset_bytes as usize, entry.size_bytes as usize).await;
        }

        info!("archive {:?} missing, fetching...", &filename);
        let buf = reqwest::get(host.make_url(&archive.key, "data"))
            .await?
            .bytes()
            .await?;
        fs::write(&filename, &buf).await?;
        Ok(buf[(entry.offset_bytes as usize)..(entry.offset_bytes as usize) + (entry.size_bytes as usize)].to_vec())
    }
}

pub struct CDNFetcher {
    pub hosts: Vec<CDNHost>,
    pub archive_index: Vec<ArchiveIndex>,
    pub root: RootFile,
    pub cache: CDNCache,
    pub encoding: EncodingFile,
    pub versions: Manifest,
    pub cdns: Manifest,
    pub cdn_config: HashMap<String, Vec<String>>,
    pub build_config: HashMap<String, Vec<String>>,
}

impl CDNFetcher {
    pub async fn init<P: AsRef<Path>>(cache_path: P, patch_server: &str, product: &str, region: &str) -> Result<Self, Error> {
        info!("intializing cache at {:?}", cache_path.as_ref());

        info!("fetching versions manifest");
        let versions = Manifest::fetch_manifest(patch_server, product, "versions").await?;
        info!("fetching CDNs manifest");
        let cdns = Manifest::fetch_manifest(patch_server, product, "cdns").await?;

        let cache = CDNCache::new(cache_path);

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
        let mut i = 0;
        for archive_key in archive_keys {
            info!("[{}/{}] fetching archive {}...", i, archive_keys.len(), archive_key);
            let archive_data = cache.fetch_data(&hosts[0], "data", &format!("{}.index", archive_key)).await?;
            archive_index.push(ArchiveIndex::parse(archive_key, &archive_data)?);
            i += 1;
        }

        info!("fetching root file");
        let root_ckey: CKey = hexunstring(&build_config.get("root").unwrap()[0]);
        let root_ekey = hexstring(&encoding.get_ekey_for_ckey(root_ckey).unwrap());
        let root_data = cache.fetch_data(&hosts[0], "data", &root_ekey).await?;
        let root = RootFile::parse(&root_data)?;

        info!("done!");
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

    pub fn find_archive_entry(&self, ekey: EKey) -> Option<(&ArchiveIndex, &ArchiveIndexEntry)> {
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

    pub async fn fetch_ckey_from_archive(&self, ckey: CKey) -> Result<Option<Vec<u8>>, Error> {
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
}
