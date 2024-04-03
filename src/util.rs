use std::{io::SeekFrom, path::Path};

use tokio::{fs, io::{AsyncReadExt, AsyncSeekExt}};

use crate::error::Error;

pub async fn fetch_data_fragment<P: AsRef<Path>>(path: P, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset as u64)).await?;
    let mut buf = vec![0; size];
    file.read(&mut buf).await?;
    Ok(buf)
}
