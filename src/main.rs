use std::path::PathBuf;

use clap::{arg, Parser, Subcommand};
use log::{error, info};
use polymorph::{cdn::CDNFetcher, error::Error};
use axum::{extract::{Path, State}, http::StatusCode, routing::get, Router};

const PATCH_SERVER: &str = "http://us.patch.battle.net:1119";
const PRODUCT: &str = "wow_classic";
const REGION: &str = "us";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    cache_path: PathBuf,

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
    Init,
    Save {
        #[arg(short, long, value_name = "FILE")]
        out_path: PathBuf,
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
    let fetcher = CDNFetcher::init(cli.cache_path, PATCH_SERVER, PRODUCT, REGION).await?;
    match cli.command {
        Commands::Serve { port, no_fetch } => {
            let state = ServerState { fetcher, no_fetch };
            let app = Router::new()
                .with_state(state)
                .route("/file-id/:file_id", get(get_file_id))
                .route("/file-name/:file_name", get(get_file_name));
            // let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port)).await.unwrap();
            // axum::serve(listener, app).await.unwrap()
        },
        Commands::GetId { file_id, out_path } => {
            let data = fetcher.fetch_file_id(file_id).await?;
            tokio::fs::write(out_path, &data).await?;
        },
        Commands::GetName { name, out_path } => {
            let data = fetcher.fetch_file_name(&name).await?;
            tokio::fs::write(out_path, &data).await?;
        },
        Commands::Init => {
            let arc_fetcher = std::sync::Arc::new(fetcher);
            let mut set = tokio::task::JoinSet::new();
            let indices = arc_fetcher.archive_index.clone();
            let num_archives = indices.len();
            info!("fetching {} archives...", num_archives);
            for archive in indices {
                let fetcher_clone = arc_fetcher.clone();
                let archive_clone = archive.clone();
                set.spawn(async move {
                    fetcher_clone.fetch_archive(&archive_clone).await
                });
            }

            let mut i = 0;
            while let Some(result) = set.join_next().await {
                match result.unwrap() {
                    Ok(bytes) => info!("[{}/{}] SUCCESS: {} bytes", i, num_archives, bytes.len()),
                    Err(err) => error!("[{}/{}] ERR: {:?}", i, num_archives, err),
                }
                i += 1;
            }
        },
        Commands::Save { out_path } => {
            let db = fetcher.build_file_db();
            let info = &db.file_infos[*db.file_id_to_file_info_index.get(&780788).unwrap()];
            dbg!(info);
            db.write_to_file(out_path).await?;
        },
    }
    Ok(())
}
