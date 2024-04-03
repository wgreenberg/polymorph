use polymorph::CDNFetcher;

#[tokio::main]
async fn main() {
    let cache_path = "./cache";
    let fetcher = CDNFetcher::init(cache_path).await.unwrap();
    for archive in &fetcher.archive_index {
        println!("fetching archive {}", archive.key);
        fetcher.fetch_archive(archive).await.unwrap();
    }
}
