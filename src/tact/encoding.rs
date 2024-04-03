use std::collections::HashMap;

use deku::{DekuRead, DekuContainerRead};

use crate::error::Error;
use crate::tact::{btle::decode_blte, common::{CKey, EKey}};

#[derive(Debug)]
pub struct EncodingFile {
    pub ckey_to_ekey: HashMap<CKey, EKey>,
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
