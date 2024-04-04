use std::{path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};

use clap::{arg, Parser, Subcommand};
use log::{error, info};
use polymorph::{cdn::CDNFetcher, error::Error};
use axum::{extract::{Path, State}, http::StatusCode, routing::get, Router};
use tokio::{sync::Mutex, task::JoinSet};

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
            let arc_fetcher = Arc::new(fetcher);
            let n_archives = arc_fetcher.archive_index.len();
            info!("fetching {} archives...", n_archives);
            let n_complete = Arc::new(AtomicUsize::new(0));
            let mut set = JoinSet::new();
            let mut archives = arc_fetcher.archive_index.clone();
            for _ in 0..10 {
                let fetcher_clone = arc_fetcher.clone();
                let archives_batch = archives.split_off((n_archives / 10).min(archives.len()));
                let n_complete_clone = n_complete.clone();
                set.spawn(async move {
                    for archive in archives_batch {
                        let res = fetcher_clone.fetch_archive(&archive).await;
                        let n = n_complete_clone.fetch_add(1, Ordering::Relaxed);
                        match res {
                            Ok(bytes) => info!("[{}/{}] {} SUCCESS: {} bytes", n, n_archives, archive.key, bytes.len()),
                            Err(err) => error!("[{}/{}] {} ERR: {:?}", n, n_archives, archive.key, err),
                        }
                    }
                });
            }

            while let Some(_) = set.join_next().await {
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
