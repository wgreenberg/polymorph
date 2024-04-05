use std::collections::HashSet;

use polymorph::error::Error;
use polymorph::sheepfile::reader::SheepfileReader;

fn make_name_variants(name: &str) -> Vec<String> {
    let mut variants = HashSet::new();
    variants.insert(name.trim().to_ascii_uppercase().replace('/', "\\"));
    variants.insert(name.trim().to_ascii_uppercase().replace('/', "\\\\"));
    variants.insert(name.to_string());
    variants.insert(name.to_string().replace('/', "\\"));
    variants.insert(name.to_string().replace('/', "\\\\"));
    variants.insert(name.to_ascii_uppercase());
    variants.insert(name.to_ascii_lowercase());
    variants.insert(name.to_ascii_uppercase().replace('/', "\\"));
    variants.insert(name.to_ascii_lowercase().replace('/', "\\"));
    variants.insert(name.to_ascii_uppercase().replace('/', "\\\\"));
    variants.insert(name.to_ascii_lowercase().replace('/', "\\\\"));
    variants.into_iter().collect()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let path = "../noclip.website/data/sheep/index.shp";
    // let name = "world/maps/deepruntram/deepruntram.wdt";
    // let file_id = 780788;
    // let name = "XTextures\\ocean\\ocean_h.1.blp";
    // let file_id = 219855;
    let name = "WORLD\\AZEROTH\\REDRIDGE\\PASSIVEDOODADS\\DOCKPIECES\\REDRIDGEDOCKPLANK02.BLP";
    let file_id = 190086;
    let sheepfile = SheepfileReader::parse(&tokio::fs::read(path).await?)?;
    let Some(entry) = sheepfile.get_entry_for_file_id(file_id) else {
        println!("No entry for file id {}", file_id);
        return Ok(())
    };
    let target = entry.name_hash;
    println!("looking for hash value {} / {:X}...", target, target);
    for variant in make_name_variants(name) {
        let mut hash = hashers::jenkins::lookup3(&variant.as_bytes());
        let high = hash & 0xffffffff00000000;
        let low = hash & 0x00000000ffffffff;
        hash = high >> 32 | low << 32;
        if hash == target {
            println!("\n\n!!!! SUCCESS: \"{}\" !!!!", variant);
            return Ok(());
        } else {
            println!("failure: \"{}\" == {} / {:X}", variant, hash, hash);
        }
    }
    Ok(())
}
