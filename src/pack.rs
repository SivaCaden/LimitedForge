use std::collections::HashMap;

use rand::Rng;

use crate::mtgjson::{BoosterConfig, Card, MtgSet};

pub struct PackCard<'a> {
    pub card: &'a Card,
    pub foil: bool,
}

#[derive(Clone)]
pub struct OwnedPackCard {
    pub name: String,
    pub set_code: String,
    pub rarity: String,
    pub foil: bool,
    pub number: String,
}

impl OwnedPackCard {
    pub fn from_card(card: &Card, foil: bool) -> Self {
        Self {
            name: card.name.clone(),
            set_code: card.set_code.clone(),
            rarity: card.rarity.clone(),
            foil,
            number: card.number.clone(),
        }
    }
}

impl From<&PackCard<'_>> for OwnedPackCard {
    fn from(pc: &PackCard<'_>) -> Self {
        Self::from_card(pc.card, pc.foil)
    }
}

pub struct PackGenerator<'a> {
    booster_config: &'a BoosterConfig,
    /// uuid → Card, across all sourceSetCodes
    card_pool: HashMap<&'a str, &'a Card>,
}

impl<'a> PackGenerator<'a> {
    pub fn new(
        set_code: &str,
        all_sets: &'a HashMap<String, MtgSet>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let set = all_sets
            .get(set_code)
            .ok_or_else(|| format!("Set '{}' not found", set_code))?;
        let booster_map = set
            .booster
            .as_ref()
            .ok_or_else(|| format!("Set '{}' has no booster data", set_code))?;
        let booster_config = booster_map
            .get("draft")
            .ok_or_else(|| format!("Set '{}' has no draft booster config", set_code))?;

        // Collect cards from all source sets (e.g. STX + STA for Strixhaven)
        let source_codes: Vec<&str> = booster_config
            .source_set_codes
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(String::as_str)
            .collect();

        let mut card_pool: HashMap<&'a str, &'a Card> = HashMap::new();

        // Always include the primary set's cards
        for card in &set.cards {
            card_pool.insert(card.uuid.as_str(), card);
        }

        // Include source set cards (may overlap with primary set)
        for code in &source_codes {
            if let Some(src_set) = all_sets.get(*code) {
                for card in &src_set.cards {
                    card_pool.insert(card.uuid.as_str(), card);
                }
            }
        }

        Ok(Self {
            booster_config,
            card_pool,
        })
    }

    pub fn generate_pack(&self, rng: &mut impl Rng) -> Vec<PackCard<'a>> {
        let booster_items: Vec<_> = self
            .booster_config
            .boosters
            .iter()
            .map(|b| (b, b.weight))
            .collect();
        let variant = weighted_choice(
            &booster_items,
            self.booster_config.boosters_total_weight,
            rng,
        );

        let mut pack = Vec::new();

        for (sheet_name, &count) in &variant.contents {
            let sheet = match self.booster_config.sheets.get(sheet_name) {
                Some(s) => s,
                None => continue,
            };

            let card_entries: Vec<(&str, u64)> = sheet
                .cards
                .iter()
                .map(|(uuid, &w)| (uuid.as_str(), w))
                .collect();

            for _ in 0..count {
                let uuid = weighted_choice(&card_entries, sheet.total_weight, rng);
                if let Some(&card) = self.card_pool.get(uuid) {
                    pack.push(PackCard {
                        card,
                        foil: sheet.foil,
                    });
                }
            }
        }

        pack
    }
}

fn weighted_choice<'a, T>(items: &'a [(T, u64)], total: u64, rng: &mut impl Rng) -> &'a T {
    let mut roll = rng.gen_range(0..total);
    for (item, weight) in items {
        if roll < *weight {
            return item;
        }
        roll -= weight;
    }
    // Fallback to last element (handles rounding edge cases)
    &items[items.len() - 1].0
}
