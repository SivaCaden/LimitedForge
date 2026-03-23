# LimitedForge

MTG limited format tool written in Rust.


## Commands

```bash
# Build
cargo build

# Run
cargo run

# Test
cargo test

# Lint
cargo clippy

# Format
cargo fmt
```

## Data

`src/AllPrintings.json` — 518MB MTGJSON v5.2.2 dataset containing all MTG card printings across all sets. This is the primary data source for card and set information. Do not commit this file; it should be listed in `.gitignore`.

Source: https://mtgjson.com/

## Architecture

Simulates MTG booster pack generation for limited formats (Draft, Sealed) and exports the results as Tabletop Simulator (TTS) deck objects, so players can run a full draft or sealed pool in TTS.

Pipeline:
1. Parse `AllPrintings.json` → build a set's card list, organized by rarity
2. Simulate booster pack generation per set rules (commons/uncommons/rares/mythics ratios, foil slots, etc.)
3. Fetch card image URLs from Scryfall (MTGJSON does not include images)
4. Output a TTS `SavedObject` JSON with `DeckCustom` objects — one per player's pack pool or draft seat

- `src/main.rs` — entry point
