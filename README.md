# LimitedForge

Ahoy, ye scallywag! LimitedForge be a tool for simulatin' MTG booster packs for limited formats such as Draft and Limited. Once yer packs have been plundered, the loot can be exported to Moxfield so ye and yer crew can sail into Tabletop Simulator and do battle on the high servers!

---

## Just Want the Binary?

If ye have no interest in compiling the beast yerself, pre-built binaries for **Windows** and **Linux** can be plundered from the Releases page:

**[https://github.com/SivaCaden/LimitedForge/releases](https://github.com/SivaCaden/LimitedForge/releases)**

Download the binary for yer platform, drop it somewhere convenient, and set sail.

---

## What Ye Be Needin' to Build From Source

- **Rust** -- hoist it from [rustup.rs](https://rustup.rs) if ye haven't already
- **AllPrintings.json** -- the great tome of all card knowledge, courtesy of [mtgjson.com](https://mtgjson.com). The app can fetch this for ye on first launch, or ye can supply yer own copy

On Linux ye may also need the following plunder for the window to render proper-like:

```
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

---

## How to Build the Beast

Clone the repository to yer local port and run the following in yer terminal:

```bash
cargo build --release
```

The compiled binary will be buried at:

```
target/release/LimitedForge
```

To run the vessel directly without diggin' for the binary, use:

```bash
cargo run
```

---

## Gettin' the Card Data Aboard

LimitedForge requires the **AllPrintings.json** dataset from MTGJSON to know what cards be hiding in each set's booster packs. This file weighs in at roughly 500 MB, so stow it somewhere ye won't forget.

On first launch, if no data file is found, the app will present ye with two choices:

- **Download from MTGJSON** -- the app will fetch the file straight from `mtgjson.com` and save it next to the executable. A progress bar will keep ye informed so ye don't think yer ship has run aground.
- **Browse Local File** -- if ye already have a copy of AllPrintings.json stowed somewhere on yer vessel, use this to point the app at it.

Once the data is aboard, LimitedForge will remember its location for future voyages.

---

## How to Sail the App

Once the card data be loaded, ye'll arrive at the Setup screen. Choose yer format -- **Draft** for a standard multi-player draft, or **Limited** for a Pre-Release-style sealed pool -- then search for the sets ye want to crack packs from and add 'em to yer list. Set the number of players with the slider, then press **GENERATE PACKS** to let the dice roll.

The Results screen shows the full haul organized by player and pack, with rarity and foil markers on each card. When yer satisfied with the loot, press **EXPORT TO MOXFIELD** to write one card list per player to a folder of yer choosin'. These files import directly into Moxfield as decks or collections.

Press **BACK** at any time to return to Setup and run another draft.

---

## Notes for the Ship's Navigator

- AllPrintings.json is not included in this repository and must never be committed to it. It be listed in `.gitignore` already, so ye need not worry.
- Any set that has booster data of any kind will appear in the set search. Sets with a proper draft configuration be marked **[DRAFT]** in the dropdown so ye can tell them apart from sets with only set or collector booster data.
- The Chaos Draft works best with sets from the last few years. Ancient sets from before the MTGJSON booster sheet era have no booster data at all and will not appear in the list.
- Collector boosters contain mostly foils and specialty cards. If ye select **[COLL]** for a set that has no collector booster config, the app will fall back to the draft config without complaint.
