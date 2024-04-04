use std::{collections::HashSet, path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};

use clap::{arg, Parser, Subcommand};
use log::{error, info};
use polymorph::{cdn::CDNFetcher, error::Error, sheepfile::SheepfileReader};
use axum::{extract::{Path, State}, http::StatusCode, routing::get, Router};
use tokio::{fs, sync::Mutex, task::JoinSet};

const PATCH_SERVER: &str = "http://us.patch.battle.net:1119";
const PRODUCT: &str = "wow_classic";
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

        #[arg(short, long, default_value_t = false)]
        no_fetch: bool,
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
    List,
    Create {
        #[arg(short, long, value_name = "FILE")]
        cache_path: PathBuf,
    },
}

#[derive(Clone)]
struct ServerState {
    fetcher: CDNFetcher,
    no_fetch: bool,
}

async fn get_file_id(state: State<ServerState>, Path(file_id): Path<u32>) -> Result<Vec<u8>, (StatusCode, String)> {
    todo!()
}

async fn get_file_name(state: State<ServerState>, Path(file_name): Path<String>) -> Result<Vec<u8>, (StatusCode, String)> {
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { port, no_fetch } => {
            let app = Router::new()
                .route("/file-id/:file_id", get(get_file_id))
                .route("/file-name/:file_name", get(get_file_name));
            // let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port)).await.unwrap();
            // axum::serve(listener, app).await.unwrap()
        },
        Commands::GetId { file_id, out_path } => {
            let sheepfile = SheepfileReader::new(&cli.sheepfile_path).await?;
            let entry = sheepfile.get_entry_for_file_id(file_id)
                .ok_or(Error::MissingFileId(file_id))?;
            let data = sheepfile.get_entry_data(&cli.sheepfile_path, entry).await?;
            fs::write(out_path, &data).await?;
        },
        Commands::GetName { name, out_path } => {
            let sheepfile = SheepfileReader::new(&cli.sheepfile_path).await?;
            let entry = sheepfile.get_entry_for_name(&name)
                .ok_or(Error::MissingFileName(name))?;
            let data = sheepfile.get_entry_data(&cli.sheepfile_path, entry).await?;
            fs::write(out_path, &data).await?;
        },
        Commands::List => {
            let sheepfile = SheepfileReader::new(cli.sheepfile_path).await?;
            for (i, entry) in sheepfile.entries.iter().enumerate() {
                println!("{} - FileID {}, Size {} bytes", i+1, entry.file_id, entry.size_bytes);
            }
        },
        Commands::Create { cache_path } => {
            let fetcher = CDNFetcher::init(cache_path, PATCH_SERVER, PRODUCT, REGION).await?;
            fetcher.save_sheepfile(cli.sheepfile_path).await?;
        },
    }
    Ok(())
}
