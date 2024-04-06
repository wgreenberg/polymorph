use std::{io::SeekFrom, path::PathBuf};

use clap::{arg, Parser, Subcommand};
use log::info;
use polymorph::{cdn::CDNFetcher, error::Error, sheepfile::{get_data_filename, reader::SheepfileReader, writer::SheepfileWriter, Entry, INDEX_FILENAME}};
use tokio::{fs, io::{AsyncReadExt, AsyncSeekExt}};

const PATCH_SERVER: &str = "http://us.patch.battle.net:1119";
const REGION: &str = "us";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    sheepfile_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Serve {
        #[arg(short, long, default_value_t = 8081)]
        port: u16,
    },
    GetId {
        file_id: u32,

        #[arg(short, long, value_name = "FILE")]
        out_path: PathBuf,
    },
    GetName {
        name: String,

        #[arg(short, long, value_name = "FILE")]
        out_path: PathBuf,
    },
    Create {
        #[arg(short, long, value_name = "FILE")]
        cache_path: PathBuf,
    },
}

async fn get_entry_data<P: AsRef<std::path::Path>>(path: P, entry: &Entry) -> Result<Vec<u8>, Error> {
    let file_path = path.as_ref().join(get_data_filename(entry.data_file_index as usize));
    let mut file = fs::File::open(file_path).await?;
    file.seek(SeekFrom::Start(entry.start_bytes as u64)).await?;
    let mut buf = vec![0; entry.size_bytes as usize];
    file.read_exact(&mut buf).await?;
    return Ok(buf)
}

async fn new_sheepfile<P: AsRef<std::path::Path>>(path: P) -> Result<SheepfileReader, Error> {
    SheepfileReader::parse(&fs::read(path.as_ref().join(INDEX_FILENAME)).await?)
}


#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { .. } => todo!(),
        Commands::GetId { file_id, out_path } => {
            let sheepfile = new_sheepfile(&cli.sheepfile_path).await?;
            let entry = sheepfile.get_entry_for_file_id(file_id)
                .ok_or(Error::MissingFileId(file_id))?;
            let data = get_entry_data(&cli.sheepfile_path, entry).await?;
            fs::write(&out_path, &data).await?;
            dbg!(&entry);
            println!("Found {} (name hash {}), wrote {} bytes to {:?}", entry.file_id, entry.name_hash, data.len(), &out_path);
        },
        Commands::GetName { name, out_path } => {
            let sheepfile = new_sheepfile(&cli.sheepfile_path).await?;
            let entry = sheepfile.get_entry_for_name(&name)
                .ok_or(Error::MissingFileName(name))?;
            let data = get_entry_data(&cli.sheepfile_path, entry).await?;
            fs::write(&out_path, &data).await?;
            println!("Found {} (name hash {}), wrote {} bytes to {:?}", entry.file_id, entry.name_hash, data.len(), &out_path);
        },
        Commands::Create { cache_path } => {
            info!("creating wow_classic CDNFetcher...");
            let mut classic_fetcher = CDNFetcher::init(&cache_path, PATCH_SERVER, "wow_classic", REGION).await?;
            info!("creating wow_classic_era CDNFetcher...");
            let mut era_fetcher = CDNFetcher::init(&cache_path, PATCH_SERVER, "wow_classic_era", REGION).await?;
            info!("creating sheepfile at {:?}", &cli.sheepfile_path);
            let sheepfile = SheepfileWriter::new(cli.sheepfile_path).await?;
            info!("writing sheepfile contents from fetchers...");
            sheepfile.write_cdn_files(&[&mut classic_fetcher, &mut era_fetcher]).await?;
        },
    }
    Ok(())
}
