#![allow(dead_code)]

use serde::Serialize;

/// Scryfall image URL for a card given its scryfall_id.
pub fn scryfall_image_url(scryfall_id: &str) -> String {
    let c1 = &scryfall_id[0..1];
    let c2 = &scryfall_id[1..2];
    format!(
        "https://cards.scryfall.io/normal/front/{}/{}/{}.jpg",
        c1, c2, scryfall_id
    )
}

/// Top-level TTS SavedObject.
#[derive(Serialize)]
pub struct SavedObject {
    #[serde(rename = "ObjectStates")]
    pub object_states: Vec<DeckCustom>,
}

/// A TTS custom deck object representing one player's card pool.
#[derive(Serialize)]
pub struct DeckCustom {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DeckIDs")]
    pub deck_ids: Vec<u32>,
    #[serde(rename = "CustomDeck")]
    pub custom_deck: std::collections::HashMap<String, CustomDeckEntry>,
    #[serde(rename = "ContainedObjects")]
    pub contained_objects: Vec<CardObject>,
}

#[derive(Serialize)]
pub struct CustomDeckEntry {
    #[serde(rename = "FaceURL")]
    pub face_url: String,
    #[serde(rename = "BackURL")]
    pub back_url: String,
    #[serde(rename = "NumWidth")]
    pub num_width: u32,
    #[serde(rename = "NumHeight")]
    pub num_height: u32,
    #[serde(rename = "BackIsHidden")]
    pub back_is_hidden: bool,
    #[serde(rename = "UniqueBack")]
    pub unique_back: bool,
}

#[derive(Serialize)]
pub struct CardObject {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Nickname")]
    pub nickname: String,
    #[serde(rename = "CardID")]
    pub card_id: u32,
}

/// Stub: writes a simple text file listing card names per player pack.
pub fn write_text_output(
    path: &std::path::Path,
    player_packs: &[Vec<Vec<crate::pack::PackCard<'_>>>],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as FmtWrite;
    let mut output = String::new();
    for (i, packs) in player_packs.iter().enumerate() {
        writeln!(output, "=== Player {} ===", i + 1)?;
        for (j, pack) in packs.iter().enumerate() {
            writeln!(output, "  Pack {}:", j + 1)?;
            for pc in pack {
                let foil_marker = if pc.foil { " [FOIL]" } else { "" };
                writeln!(
                    output,
                    "    {} ({} {}){}",
                    pc.card.name, pc.card.set_code, pc.card.rarity, foil_marker
                )?;
            }
        }
        writeln!(output)?;
    }
    std::fs::write(path, output)?;
    Ok(())
}
