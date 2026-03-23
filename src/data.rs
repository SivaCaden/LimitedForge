use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::mtgjson::AllPrintings;

pub fn load(path: &Path) -> Result<AllPrintings, Box<dyn std::error::Error>> {
    eprintln!("Loading {}...", path.display());
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let data: AllPrintings = serde_json::from_reader(reader)?;
    eprintln!("Loaded {} sets.", data.data.len());
    Ok(data)
}

/// Returns (code, name) pairs for sets that have a "draft" booster config, sorted by name.
#[allow(dead_code)]
pub fn sets_with_draft_booster(data: &AllPrintings) -> Vec<(&str, &str)> {
    let mut sets: Vec<(&str, &str)> = data
        .data
        .iter()
        .filter(|(_, set)| {
            set.booster
                .as_ref()
                .map(|b| b.contains_key("draft"))
                .unwrap_or(false)
        })
        .map(|(code, set)| (code.as_str(), set.name.as_str()))
        .collect();
    sets.sort_by_key(|(_, name)| *name);
    sets
}
