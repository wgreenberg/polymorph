
#![allow(unused, dead_code)]

use std::{collections::HashMap, io::SeekFrom, path::{Path, PathBuf}};

use deku::{bitvec::BitSlice, DekuContainerRead, DekuError, DekuRead};
use thiserror::Error;
use tokio::{fs, io::{AsyncReadExt, AsyncSeekExt}};

use miniz_oxide::inflate::{decompress_to_vec_zlib, DecompressError};

const PATCH_SERVER: &str = "http://us.patch.battle.net:1119";
const PRODUCT: &str = "wow_classic";
const REGION: &str = "us";

#[derive(Error, Debug)]
enum Error {
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
}

struct Manifest {
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
            if line.len() == 0 || line.starts_with('#') {
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

fn hexstring(hex: &[u8]) -> String {
    let mut result = String::new();
    for b in hex {
        result.push_str(&format!("{:x}", b));
    }
    result
}

struct CDNHost {
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

struct CDNCache {
    cache_path: PathBuf,
}

impl CDNCache {
    pub fn new(cache_path: &str) -> Self {
        CDNCache {
            cache_path: PathBuf::from(cache_path),
        }
    }

    pub async fn fetch_data(&self, host: &CDNHost, directory: &str, key: &str) -> Result<Vec<u8>, Error> {
        let mut file_path = self.cache_path.join(directory);
        file_path.push(key);
        match fs::try_exists(&file_path).await {
            Ok(true) => Ok(fs::read(file_path).await?),
            _ => {
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

    pub async fn fetch_archive_entry(&self, host: &CDNHost, archive: &ArchiveIndex, entry: &ArchiveIndexEntry) -> Result<Vec<u8>, Error> {
        let mut filename = self.cache_path.join("/data");
        filename.push(&archive.key);
        if let Ok(true) = fs::try_exists(&filename).await {
            return fetch_data_fragment(&filename, entry.offset_bytes, entry.size_bytes).await;
        }

        let buf = reqwest::get(host.make_url(&archive.key, "/data"))
            .await?
            .bytes()
            .await?;
        fs::write(&filename, &buf).await?;
        Ok(buf[entry.offset_bytes..entry.offset_bytes + entry.size_bytes].to_vec())
    }
}

async fn fetch_data_fragment<P: AsRef<Path>>(path: P, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset as u64)).await?;
    let mut buf = Vec::with_capacity(size);
    file.read(&mut buf).await;
    Ok(buf)
}

#[derive(DekuRead, Debug)]
struct ArchiveIndexFooter {
    pub toc_hash: u16,
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
    pub num_blocks: u32,
}

#[derive(DekuRead)]
struct ArchiveIndexEntry {
    pub ekey: EKey,
    #[deku(endian = "big")]
    pub size_bytes: usize,
    #[deku(endian = "big", pad_bytes_after = "0x18")]
    pub offset_bytes: usize,

}

struct ArchiveIndex {
    entries: HashMap<EKey, ArchiveIndexEntry>,
    key: String,
}

impl ArchiveIndex {
    pub fn parse(key: &str, data: &[u8]) -> Result<Self, Error> {
        let mut entries: HashMap<EKey, ArchiveIndexEntry> = HashMap::new();
        let footer_offset = data.len() - 0x24;
        let (_, footer): (_, ArchiveIndexFooter) = ArchiveIndexFooter::from_bytes((&data[footer_offset..], 0))?;

        let block_size = (footer.block_size_kb as usize) << 10;
        for i in 0..footer.num_blocks as usize {
            let block_start = i * block_size;
            let block_end = block_start + block_size;
            let mut block_data = &data[block_start..block_end];
            while block_data.len() > block_size {
                let ((new_block_data, _), entry) = ArchiveIndexEntry::from_bytes((&block_data[..], 0))?;
                block_data = new_block_data;
                if &entry.ekey == &[0; 16] {
                    break;
                }
                entries.insert(entry.ekey, entry);
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
struct BLTEChunk {
    #[deku(endian = "big")]
    pub compressed_size: u32,
    #[deku(endian = "big")]
    pub uncompressed_size: u32,
    #[deku(endian = "big")]
    pub checksum: [u8; 0x10],
}

#[derive(DekuRead, Debug)]
#[deku(magic = b"BLTE")]
struct BLTEHeader {
    #[deku(endian = "big")]
    pub data_offset: u32,
    pub flag: u8,
    #[deku(endian = "big", bytes = 3)]
    pub chunk_count: u32,
    #[deku(count = "chunk_count")]
    pub chunks: Vec<BLTEChunk>,
}

fn decode_blte(buf: &[u8]) -> Result<Vec<u8>, Error> {
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

struct RootFile {
    pub file_id_to_ckey: HashMap<u32, String>,
}

impl RootFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        todo!()
    }

    pub fn get_ckey_for_file_id(&self, file_id: u32) -> Option<&str> {
        self.file_id_to_ckey.get(&file_id).map(|s| s.as_str())
    }
}

struct EncodingFile {
}

impl EncodingFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        todo!()
    }

    pub fn get_ekey_for_ckey(&self, ckey: &str) -> Option<EKey> {
        todo!()
    }
}

struct CDNFetcher {
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
        if line.is_empty() || line.starts_with("#") {
            continue
        }

        let (k, v) = line.split_once(" = ").expect("invalid line");
        result.insert(k.to_string(), v.split(' ').map(|s| s.to_string()).collect());
    }
    result
}

impl CDNFetcher {
    pub async fn init(cache_path: &str) -> Result<Self, Error> {
        let versions = Manifest::fetch_manifest(PATCH_SERVER, PRODUCT, "versions").await?;
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

        let cdn_config = parse_config(&String::from_utf8(cache.fetch_data(&hosts[0], "/config", cdn_config_key).await?).expect("invalid config"));
        let build_config = parse_config(&String::from_utf8(cache.fetch_data(&hosts[0], "/config", build_config_key).await?).expect("invalid config"));

        let encoding_key = &build_config.get("encoding").unwrap()[0];
        let encoding = EncodingFile::parse(&cache.fetch_data(&hosts[0], "/data", encoding_key).await?)?;

        let mut archive_index = Vec::new();
        for archive_key in cdn_config.get("archives").unwrap() {
            let archive_data = cache.fetch_data(&hosts[0], "/data", &format!("{}.index", archive_key)).await?;
            archive_index.push(ArchiveIndex::parse(&archive_key, &archive_data)?);
        }

        let root_ckey = &build_config.get("root").unwrap()[0];
        let root_ekey = hexstring(&encoding.get_ekey_for_ckey(&root_ckey).unwrap());
        let root_data = cache.fetch_data(&hosts[0], "/data", &root_ekey).await?;
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

    pub fn find_archive_entry(&self, ekey: EKey) -> Option<(&ArchiveIndex, &ArchiveIndexEntry)> {
        for index in &self.archive_index {
            if let Some(entry) = index.get_entry_for_ekey(ekey) {
                return Some((index, entry));
            }
        }
        None
    }

    pub async fn fetch_ckey_from_archive(&self, ckey: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(ekey) = self.encoding.get_ekey_for_ckey(ckey) else {
            return Ok(None);
        };
        let Some((archive, entry)) = self.find_archive_entry(ekey) else {
            return Ok(None);
        };
        let data = self.cache.fetch_archive_entry(&self.hosts[0], archive, entry).await?;
        Ok(Some(data))
    }
}

#[tokio::main]
async fn main() {
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read};

    use super::*;

    #[test]
    fn test_hexstring() {
        let hex: [u8; 4] = [0x13, 0x12, 0xde, 0xad];
        assert_eq!(hexstring(&hex), "1312dead");
    }

    #[test]
    fn test_blte_decode() {
        let test_file = std::fs::read("./test/test1.blte.out").unwrap();

        let buf = decode_blte(&test_file).unwrap();
        dbg!(buf);
    }
}
