#![allow(dead_code)]

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct AllPrintings {
    pub meta: Meta,
    pub data: HashMap<String, MtgSet>,
}

#[derive(Deserialize)]
pub struct Meta {
    pub version: String,
    pub date: String,
}

#[derive(Deserialize)]
pub struct MtgSet {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub booster: Option<HashMap<String, BoosterConfig>>,
    #[serde(default)]
    pub cards: Vec<Card>,
}

#[derive(Deserialize)]
pub struct BoosterConfig {
    #[serde(default)]
    pub name: String,
    pub boosters: Vec<BoosterVariant>,
    #[serde(rename = "boostersTotalWeight")]
    pub boosters_total_weight: u64,
    pub sheets: HashMap<String, Sheet>,
    #[serde(rename = "sourceSetCodes", default)]
    pub source_set_codes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct BoosterVariant {
    pub contents: HashMap<String, u32>,
    pub weight: u64,
}

#[derive(Deserialize)]
pub struct Sheet {
    pub cards: HashMap<String, u64>,
    pub foil: bool,
    #[serde(rename = "totalWeight")]
    pub total_weight: u64,
}

#[derive(Deserialize)]
pub struct Card {
    pub uuid: String,
    pub name: String,
    pub number: String,
    #[serde(rename = "setCode")]
    pub set_code: String,
    pub rarity: String,
    pub identifiers: CardIdentifiers,
}

#[derive(Deserialize)]
pub struct CardIdentifiers {
    #[serde(rename = "scryfallId")]
    pub scryfall_id: Option<String>,
}
