use std::{collections::HashMap, io::SeekFrom, path::Path};

use tokio::{fs, io::{AsyncReadExt, AsyncSeekExt}};

use crate::error::Error;

pub async fn fetch_data_fragment<P: AsRef<Path>>(path: P, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset as u64)).await?;
    let mut buf = vec![0; size];
    file.read_exact(&mut buf).await?;
    Ok(buf)
}

pub fn parse_config(data: &str) -> HashMap<String, Vec<String>> {
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
