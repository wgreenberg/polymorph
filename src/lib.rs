use std::{collections::HashMap, io::SeekFrom, path::{Path, PathBuf}};

use log::{debug, info};
use deku::{DekuContainerRead, DekuError, DekuRead};
use thiserror::Error;
use tokio::{fs, io::{AsyncReadExt, AsyncSeekExt}};

use miniz_oxide::inflate::{decompress_to_vec_zlib, DecompressError};

const PATCH_SERVER: &str = "http://us.patch.battle.net:1119";
const PRODUCT: &str = "wow_classic";
const REGION: &str = "us";

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to make HTTP request")]
    HTTPRequestError(#[from] reqwest::Error),
    #[error("I/O error")]
    IOError(#[from] std::io::Error),
    #[error("Deku parsing error")]
    DekuError(#[from] DekuError),
    #[error("Missing CKey")]
    MissingCKey,
    #[error("Invalid Zlib")]
    ZlibError(DecompressError),
    #[error("Couldn't find file id {0}")]
    MissingFileId(u32),
}

pub struct Manifest {
    pub field_names: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Manifest {
    pub async fn fetch_manifest(patch_server: &str, product: &str, path: &str) -> Result<Self, Error> {
        let body = reqwest::get(format!("{patch_server}/{product}/{path}"))
            .await?
            .text()
            .await?;
        let mut lines = body.lines();

        let header = lines.next().unwrap();
        let mut field_names = Vec::new();
        for field_def in header.split('|') {
            let (field_name, _field_type) = field_def.split_once('!').unwrap();
            field_names.push(field_name.to_string());
        }

        let mut rows = Vec::new();
        for line in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            rows.push(line.split('|').map(|s| s.to_string()).collect());
        }

        Ok(Manifest {
            field_names,
            rows,
        })
    }

    pub fn get_field_index(&self, needle: &str) -> Option<usize> {
        self.field_names.iter().position(|haystack| haystack == needle)
    }

    pub fn get_field(&self, row: usize, field: &str) -> Option<&str> {
        let field_index = self.get_field_index(field)?;
        let row = self.rows.get(row)?;
        Some(row.get(field_index)?.as_str())
    }

    pub fn find_row(&self, field: &str, value: &str) -> Option<usize> {
        let field_index = self.get_field_index(field)?;
        self.rows.iter().position(|row| row[field_index] == value)
    }
}

type EKey = [u8; 16];
type CKey = [u8; 16];

fn hexstring(hex: &[u8]) -> String {
    let mut result = String::new();
    for b in hex {
        result.push_str(&format!("{:x}", b));
    }
    result
}

fn hexunstring(s: &str) -> [u8; 16] {
    let mut key = [0; 16];
    for i in 0..16 {
        let hex = &s[i*2..i*2+2];
        key[i] = u8::from_str_radix(hex, 16).unwrap();
    }
    key
}

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

async fn fetch_data_fragment<P: AsRef<Path>>(path: P, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset as u64)).await?;
    let mut buf = vec![0; size];
    file.read(&mut buf).await?;
    Ok(buf)
}

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

pub struct ArchiveIndex {
    entries: HashMap<EKey, ArchiveIndexEntry>,
    pub key: String,
}

#[derive(DekuRead)]
pub struct ArchiveIndexEntry {
    pub ekey: EKey,
    #[deku(endian = "big")]
    pub size_bytes: u32,
    #[deku(endian = "big")]
    pub offset_bytes: u32,
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
                if entry.ekey == [0; 16] {
                    break;
                }

                entries.insert(entry.ekey, entry);
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

    pub fn get_entry_for_ekey(&self, ekey: EKey) -> Option<&ArchiveIndexEntry> {
        self.entries.get(&ekey)
    }
}

#[derive(DekuRead, Debug)]
pub struct BLTEChunk {
    #[deku(endian = "big")]
    pub compressed_size: u32,
    #[deku(endian = "big")]
    pub uncompressed_size: u32,
    #[deku(endian = "big")]
    pub checksum: [u8; 0x10],
}

#[derive(DekuRead, Debug)]
#[deku(magic = b"BLTE")]
pub struct BLTEHeader {
    #[deku(endian = "big")]
    pub data_offset: u32,
    pub flag: u8,
    #[deku(endian = "big", bytes = 3)]
    pub chunk_count: u32,
    #[deku(count = "chunk_count")]
    pub chunks: Vec<BLTEChunk>,
}

pub fn decode_blte(buf: &[u8]) -> Result<Vec<u8>, Error> {
    let header = BLTEHeader::from_bytes((buf, 0))?.1;
    let mut out = Vec::new();

    let mut data_offs = header.data_offset as usize;
    for chunk in &header.chunks {
        let chunk_buf = &buf[data_offs .. data_offs + (chunk.compressed_size as usize)];
        let frame_type = chunk_buf[0] as char;
        let chunk_data = &chunk_buf[1 .. chunk_buf.len()];
        match frame_type {
            'N' => out.extend(chunk_data),
            'Z' => out.extend(decompress_to_vec_zlib(chunk_data).map_err(Error::ZlibError)?),
            c => panic!("{}", c),
        }
        data_offs += chunk.compressed_size as usize;
    }

    Ok(out)
}

#[derive(DekuRead, Clone)]
#[deku(endian = "little")]
pub struct RootFileEntry {
    pub ckey: EKey,
    pub name_hash: u64,
}

pub struct RootFile {
    pub file_id_to_ckey: HashMap<u32, RootFileEntry>,
}

#[derive(DekuRead)]
struct RootBlock {
    #[deku(endian = "little")]
    pub num_files: u32,
    #[deku(endian = "little")]
    pub content_flags: u32,
    #[deku(endian = "little")]
    pub locale_flags: u32,
    #[deku(count = "num_files")]
    pub file_id_delta_table: Vec<u32>,
    #[deku(count = "num_files")]
    pub file_entries: Vec<RootFileEntry>,
}

impl RootFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {

        let decode = decode_blte(data)?;

        let mut out = RootFile { file_id_to_ckey: HashMap::new() };
        let mut rest = &decode[..];
        loop {
            let Ok(((new_rest, _), block)) = RootBlock::from_bytes((rest, 0)) else {
                break;
            };
            rest = new_rest;

            let mut file_id = 0;
            for (file_id_delta, entry) in std::iter::zip(block.file_id_delta_table.iter(), block.file_entries.iter()) {
                file_id += file_id_delta;
                out.file_id_to_ckey.insert(file_id, entry.clone());
                file_id += 1;
            }
        }

        Ok(out)
    }

    pub fn get_ckey_for_file_id(&self, file_id: u32) -> Option<CKey> {
        self.file_id_to_ckey.get(&file_id).map(|s| s.ckey)
    }
}

#[derive(Debug)]
pub struct EncodingFile {
    pub ckey_to_ekey: HashMap<EKey, EKey>,
}

#[derive(DekuRead, Debug)]
#[deku(endian = "big")]
struct EncodingFilePage {
    pub ekey_count: u8,
    #[deku(pad_bytes_before = "1")]
    pub size: u32, // Technically this is a 40-bit size value. We chop off the first byte here... hope it doesn't matter!
    pub ckey: EKey,
    #[deku(count = "ekey_count")]
    pub ekey: Vec<EKey>,
}

#[derive(DekuRead, Debug)]
#[deku(magic = b"EN", endian = "big")]
struct EncodingFileHeader {
    pub version: u8,
    pub hash_size_ckey: u8,
    pub hash_size_ekey: u8,
    pub page_size_ckey: u16,
    pub page_size_ekey: u16,
    pub page_count_ckey: u32,
    pub page_count_ekey: u32,
    #[deku(assert_eq = "0")]
    _pad1: u8,
    pub espec_page_size: u32,
}

impl EncodingFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let decode = decode_blte(data)?;
        let ((rest, _), header) = EncodingFileHeader::from_bytes((&decode, 0))?;

        let mut out = EncodingFile { ckey_to_ekey: HashMap::new() };
        let page_start_ckey = header.espec_page_size + header.page_count_ckey * ((header.hash_size_ckey as u32) + 0x10);
        let page_size_ckey = (header.page_size_ckey as u32) * 1024;

        for i in 0..header.page_count_ckey {
            let offs = (page_start_ckey + page_size_ckey * i) as usize;
            let page_end = offs + (page_size_ckey as usize);

            let mut page_rest = &rest[offs .. page_end];
            loop {

                let Ok(((new_page_rest, _), page)) = EncodingFilePage::from_bytes((page_rest, 0)) else {
                    break;
                };

                page_rest = new_page_rest;

                if page.ekey_count == 0 {
                    break;
                }

                out.ckey_to_ekey.insert(page.ckey, page.ekey[0]);
            }
        }

        Ok(out)
    }

    pub fn get_ekey_for_ckey(&self, ckey: EKey) -> Option<EKey> {
        return self.ckey_to_ekey.get(&ckey).cloned();
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

impl CDNFetcher {
    pub async fn init<P: AsRef<Path>>(cache_path: P) -> Result<Self, Error> {
        info!("intializing cache at {:?}", cache_path.as_ref());

        info!("fetching versions manifest");
        let versions = Manifest::fetch_manifest(PATCH_SERVER, PRODUCT, "versions").await?;
        info!("fetching CDNs manifest");
        let cdns = Manifest::fetch_manifest(PATCH_SERVER, PRODUCT, "cdns").await?;

        let cache = CDNCache::new(cache_path);

        let cdn_row = cdns.find_row("Name", REGION).unwrap();
        let path = cdns.get_field(cdn_row, "Path").unwrap();
        let hosts: Vec<CDNHost> = cdns.get_field(cdn_row, "Hosts").unwrap()
            .split_whitespace()
            .map(|host| CDNHost::new(host, path))
            .collect();

        let version_row = versions.find_row("Region", REGION).unwrap();
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

    pub async fn fetch_ckey_from_archive(&self, ckey: EKey) -> Result<Option<Vec<u8>>, Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hexstring() {
        let hex: [u8; 4] = [0x13, 0x12, 0xde, 0xad];
        assert_eq!(hexstring(&hex), "1312dead");
    }

    #[test]
    fn test_hexunstring() {
        let s = "0017a402f556fbece46c38dc431a2c9b";
        let hex: EKey = [0x00, 0x17, 0xa4, 0x02, 0xf5, 0x56, 0xfb, 0xec, 0xe4, 0x6c, 0x38, 0xdc, 0x43, 0x1a, 0x2c, 0x9b];
        assert_eq!(hexunstring(s), hex);
    }

    #[test]
    fn test_blte_decode() {
        let test_file = std::fs::read("./test/test1.blte.out").unwrap();

        let buf = decode_blte(&test_file).unwrap();
        dbg!(buf);
    }

    #[test]
    fn test_encoding_file() {
        let test_file = std::fs::read("./test/encoding.out").unwrap();

        let file = EncodingFile::parse(&test_file).unwrap();
        dbg!(file);
    }

    #[test]
    fn test_root_file() {
        let test_file = std::fs::read("./test/root.out").unwrap();

        let file = RootFile::parse(&test_file).unwrap();
        dbg!(file.file_id_to_ckey.len());
    }
}
