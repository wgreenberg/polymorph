use std::collections::HashMap;

pub mod tact;
pub mod error;
pub mod util;
pub mod cdn;

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

#[cfg(test)]
mod tests {
    use crate::tact::{btle::decode_blte, common::{hexstring, hexunstring, EKey}, encoding::EncodingFile, root::RootFile};

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
