use std::collections::HashMap;

use deku::{DekuRead, DekuContainerRead};

use crate::error::Error;
use crate::tact::{btle::decode_blte, common::{CKey, EKey}};

#[derive(Clone, Debug)]
pub struct EncodingFile {
    pub ckey_to_ekey: HashMap<CKey, EKey>,
}

#[derive(DekuRead, Debug)]
struct EncodingFilePage {
    pub ekey_count: u8,
    #[deku(pad_bytes_before = "1", endian = "big")]
    pub _size: u32, // Technically this is a 40-bit size value. We chop off the first byte here... hope it doesn't matter!
    pub ckey: CKey,
    #[deku(count = "ekey_count")]
    pub ekeys: Vec<EKey>,
}

#[derive(DekuRead, Debug)]
#[deku(magic = b"EN", endian = "big")]
struct EncodingFileHeader {
    pub _version: u8,
    pub hash_size_ckey: u8,
    pub _hash_size_ekey: u8,
    pub page_size_ckey: u16,
    pub _page_size_ekey: u16,
    pub page_count_ckey: u32,
    pub _page_count_ekey: u32,
    #[deku(assert_eq = "0")]
    _pad1: u8,
    pub espec_page_size: u32,
}

impl EncodingFile {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let decode = decode_blte(data)?;
        let ((rest, _), header) = EncodingFileHeader::from_bytes((&decode, 0))?;

        let mut ckey_to_ekey = HashMap::new();
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

                ckey_to_ekey.insert(page.ckey, page.ekeys[0].clone());
            }
        }

        Ok(EncodingFile {
            ckey_to_ekey,
        })
    }

    pub fn get_ekey_for_ckey(&self, ckey: &CKey) -> Option<&EKey> {
        self.ckey_to_ekey.get(ckey)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_file() {
        let test_file = std::fs::read("./test/encoding.out").unwrap();

        let file = EncodingFile::parse(&test_file).unwrap();
        dbg!(file);
    }
}
