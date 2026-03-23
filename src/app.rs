use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eframe::egui;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::data;
use crate::mtgjson::AllPrintings;
use crate::pack::{OwnedPackCard, PackGenerator};

enum Screen {
    Loading,
    Setup,
    Results {
        packs: Vec<Vec<Vec<OwnedPackCard>>>, // [player][pack][card]
        promos: Vec<OwnedPackCard>,          // one per player
    },
}

pub struct LimitedForgeApp {
    screen: Screen,
    load_rx: Option<mpsc::Receiver<Result<AllPrintings, String>>>,
    all_printings: Option<AllPrintings>,
    sets: Vec<(String, String)>, // (code, name) sorted by name
    data_path: String,

    // Setup form
    set_query: String,
    selected_set: Option<String>,
    predictions: Vec<(String, String)>,
    num_players: usize,
    packs_per_player: usize,

    // Loading animation
    tick: u64,

    error: Option<String>,
    export_status: Option<String>,
}

impl LimitedForgeApp {
    pub fn new() -> Self {
        let default_path = "src/AllPrintings.json".to_string();
        let rx = Self::start_load(&default_path);
        Self {
            screen: Screen::Loading,
            load_rx: Some(rx),
            all_printings: None,
            sets: Vec::new(),
            data_path: default_path,
            set_query: String::new(),
            selected_set: None,
            predictions: Vec::new(),
            num_players: 8,
            packs_per_player: 3,
            tick: 0,
            error: None,
            export_status: None,
        }
    }

    fn start_load(path: &str) -> mpsc::Receiver<Result<AllPrintings, String>> {
        let (tx, rx) = mpsc::channel();
        let path = path.to_string();
        thread::spawn(move || {
            let result = data::load(std::path::Path::new(&path)).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        rx
    }

    fn reload(&mut self, path: String) {
        self.data_path = path;
        self.all_printings = None;
        self.sets.clear();
        self.set_query.clear();
        self.selected_set = None;
        self.predictions.clear();
        self.error = None;
        self.export_status = None;
        self.tick = 0;
        self.load_rx = Some(Self::start_load(&self.data_path));
        self.screen = Screen::Loading;
    }

    fn update_predictions(&mut self) {
        let query = self.set_query.trim().to_lowercase();
        if query.is_empty() {
            self.predictions.clear();
            return;
        }

        let mut exact: Vec<&(String, String)> = Vec::new();
        let mut prefix: Vec<&(String, String)> = Vec::new();
        let mut substr: Vec<&(String, String)> = Vec::new();

        for entry in &self.sets {
            let code_lower = entry.0.to_lowercase();
            let name_lower = entry.1.to_lowercase();

            if code_lower == query {
                exact.push(entry);
            } else if code_lower.starts_with(&query) || name_lower.starts_with(&query) {
                prefix.push(entry);
            } else if code_lower.contains(&query) || name_lower.contains(&query) {
                substr.push(entry);
            }
        }

        self.predictions = exact
            .into_iter()
            .chain(prefix)
            .chain(substr)
            .take(7)
            .cloned()
            .collect();
    }

    fn show_loading(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.load_rx {
            match rx.try_recv() {
                Ok(Ok(data)) => {
                    let mut sets: Vec<(String, String)> = data
                        .data
                        .iter()
                        .filter(|(_, set)| {
                            set.booster
                                .as_ref()
                                .map(|b| b.contains_key("draft"))
                                .unwrap_or(false)
                        })
                        .map(|(code, set)| (code.clone(), set.name.clone()))
                        .collect();
                    sets.sort_by(|a, b| a.1.cmp(&b.1));
                    self.sets = sets;
                    self.all_printings = Some(data);
                    self.load_rx = None;
                    self.screen = Screen::Setup;
                    return;
                }
                Ok(Err(e)) => {
                    self.error = Some(e);
                    self.load_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Data loader thread disconnected.".into());
                    self.load_rx = None;
                }
            }
        }

        ctx.request_repaint_after(Duration::from_millis(100));
        self.tick = self.tick.wrapping_add(1);

        egui::CentralPanel::default().show(ctx, |ui| {
            title_bar(ui, "LimitedForge");
            ui.add_space(40.0);
            ui.centered_and_justified(|ui| {
                if let Some(err) = &self.error {
                    ui.label(
                        egui::RichText::new(format!("ERROR: {}", err))
                            .monospace()
                            .color(egui::Color32::RED),
                    );
                } else {
                    let bar_width = 20usize;
                    let filled = ((self.tick / 2) % (bar_width as u64 + 1)) as usize;
                    let bar = format!(
                        "[{}{}]",
                        "█".repeat(filled),
                        "░".repeat(bar_width - filled)
                    );
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Please wait...")
                                .monospace()
                                .size(14.0),
                        );
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Loading card data...").monospace().size(13.0));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(bar).monospace().size(14.0));
                    });
                }
            });
        });
    }

    fn show_setup(&mut self, ctx: &egui::Context) {
        let mut new_path: Option<String> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            title_bar(ui, "LimitedForge - Setup");
            ui.add_space(10.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("DATA SOURCE").monospace().strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("File:").monospace());
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&self.data_path)
                            .monospace()
                            .color(egui::Color32::from_rgb(0, 0, 128)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("[ BROWSE... ]").monospace())
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("JSON", &["json"])
                                .set_title("Select AllPrintings JSON file")
                                .pick_file()
                            {
                                new_path = Some(path.to_string_lossy().into_owned());
                            }
                        }
                    });
                });
            });

            ui.add_space(6.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("FORMAT").monospace().strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Format:").monospace());
                    ui.add_space(4.0);
                    let _ = ui.selectable_label(true, egui::RichText::new("Limited").monospace());
                });
            });

            ui.add_space(6.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("SET SELECTION").monospace().strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Set:").monospace());
                    ui.add_space(4.0);
                    let response = ui.text_edit_singleline(&mut self.set_query);
                    if response.changed() {
                        self.selected_set = None;
                        self.update_predictions();
                    }
                });

                if !self.predictions.is_empty() {
                    let predictions = self.predictions.clone();
                    let mut chosen: Option<(String, String)> = None;
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        for (code, name) in &predictions {
                            let label = format!("{} ({})", name, code);
                            if ui
                                .selectable_label(false, egui::RichText::new(&label).monospace())
                                .clicked()
                            {
                                chosen = Some((code.clone(), name.clone()));
                            }
                        }
                    });
                    if let Some((code, name)) = chosen {
                        self.set_query = name;
                        self.selected_set = Some(code);
                        self.predictions.clear();
                    }
                }
            });

            ui.add_space(6.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("PLAYERS & PACKS").monospace().strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Players:").monospace());
                    ui.add(egui::Slider::new(&mut self.num_players, 1..=16));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Packs:  ").monospace());
                    ui.add(egui::Slider::new(&mut self.packs_per_player, 1..=6));
                });
            });

            ui.add_space(10.0);

            let can_generate = self.selected_set.is_some();
            ui.add_enabled_ui(can_generate, |ui| {
                if ui
                    .button(egui::RichText::new("[ GENERATE PACKS ]").monospace().strong())
                    .clicked()
                {
                    self.generate_packs();
                }
            });

            if let Some(err) = &self.error {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("ERROR: {}", err))
                        .monospace()
                        .color(egui::Color32::RED),
                );
            }
        });

        if let Some(path) = new_path {
            self.reload(path);
        }
    }

    fn generate_packs(&mut self) {
        let set_code = match &self.selected_set {
            Some(c) => c.clone(),
            None => return,
        };
        let all_printings = match &self.all_printings {
            Some(ap) => ap,
            None => return,
        };

        let generator = match PackGenerator::new(&set_code, &all_printings.data) {
            Ok(g) => g,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };

        let rare_pool: Vec<OwnedPackCard> = all_printings
            .data
            .get(&set_code)
            .map(|set| {
                set.cards
                    .iter()
                    .filter(|c| c.rarity == "rare" || c.rarity == "mythic")
                    .map(|c| OwnedPackCard::from_card(c, false))
                    .collect()
            })
            .unwrap_or_default();

        let mut rng = StdRng::from_entropy();
        let num_players = self.num_players;
        let packs_per_player = self.packs_per_player;

        let mut player_packs: Vec<Vec<Vec<OwnedPackCard>>> = Vec::new();
        for _ in 0..num_players {
            let mut packs = Vec::new();
            for _ in 0..packs_per_player {
                let pack = generator.generate_pack(&mut rng);
                let owned: Vec<OwnedPackCard> = pack.iter().map(OwnedPackCard::from).collect();
                packs.push(owned);
            }
            player_packs.push(packs);
        }

        let promos: Vec<OwnedPackCard> = if rare_pool.is_empty() {
            Vec::new()
        } else {
            (0..num_players)
                .map(|_| rare_pool[rng.gen_range(0..rare_pool.len())].clone())
                .collect()
        };

        self.export_status = None;
        self.screen = Screen::Results {
            packs: player_packs,
            promos,
        };
    }

    fn show_results(&mut self, ctx: &egui::Context) {
        let set_code = self.selected_set.clone().unwrap_or_default();
        let num_players = self.num_players;
        let packs_per_player = self.packs_per_player;
        let export_status = self.export_status.clone();

        let mut go_back = false;
        let mut do_export = false;

        {
            let (player_packs, promos) = match &self.screen {
                Screen::Results { packs, promos } => (packs, promos),
                _ => return,
            };

            egui::CentralPanel::default().show(ctx, |ui| {
                title_bar(ui, "LimitedForge - Results");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("[< BACK]").monospace())
                        .clicked()
                    {
                        go_back = true;
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} | {} PLAYERS | {} PACKS",
                            set_code.to_uppercase(),
                            num_players,
                            packs_per_player
                        ))
                        .monospace()
                        .strong(),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button(
                                    egui::RichText::new("[EXPORT TO MOXFIELD]").monospace(),
                                )
                                .clicked()
                            {
                                do_export = true;
                            }
                        },
                    );
                });

                if let Some(status) = &export_status {
                    ui.add_space(4.0);
                    let color = if status.starts_with("Export failed") {
                        egui::Color32::RED
                    } else {
                        egui::Color32::from_rgb(0, 128, 0)
                    };
                    ui.label(egui::RichText::new(status).monospace().color(color));
                }

                ui.add_space(8.0);

                separator(ui);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (p_idx, packs) in player_packs.iter().enumerate() {
                        let player_label =
                            format!(">> PLAYER {}", p_idx + 1);
                        egui::CollapsingHeader::new(
                            egui::RichText::new(&player_label).monospace().strong(),
                        )
                        .default_open(p_idx == 0)
                        .show(ui, |ui| {
                            if let Some(promo) = promos.get(p_idx) {
                                egui::CollapsingHeader::new(
                                    egui::RichText::new("  [PROMO]").monospace(),
                                )
                                .default_open(p_idx == 0)
                                .show(ui, |ui| {
                                    card_row(ui, promo);
                                });
                            }

                            for (pk_idx, pack) in packs.iter().enumerate() {
                                let pack_label = format!("  [PACK {}]", pk_idx + 1);
                                egui::CollapsingHeader::new(
                                    egui::RichText::new(&pack_label).monospace(),
                                )
                                .default_open(p_idx == 0 && pk_idx == 0)
                                .show(ui, |ui| {
                                    for card in pack {
                                        card_row(ui, card);
                                    }
                                });
                            }
                        });
                    }
                });
            });
        }

        if go_back {
            self.screen = Screen::Setup;
        }
        if do_export {
            self.export_to_moxfield();
        }
    }

    fn export_to_moxfield(&mut self) {
        let player_lines: Vec<Vec<String>> = match &self.screen {
            Screen::Results { packs, promos } => packs
                .iter()
                .enumerate()
                .map(|(p_idx, player_packs)| {
                    let mut lines = Vec::new();
                    if let Some(promo) = promos.get(p_idx) {
                        lines.push(moxfield_line(promo));
                    }
                    for pack in player_packs {
                        for card in pack {
                            lines.push(moxfield_line(card));
                        }
                    }
                    lines
                })
                .collect(),
            _ => return,
        };

        let dir = std::path::Path::new("moxfield_export");
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.export_status = Some(format!("Export failed: {}", e));
            return;
        }

        let count = player_lines.len();
        for (p_idx, lines) in player_lines.iter().enumerate() {
            let path = dir.join(format!("player_{}.txt", p_idx + 1));
            if let Err(e) = std::fs::write(&path, lines.join("\n")) {
                self.export_status = Some(format!("Export failed: {}", e));
                return;
            }
        }

        self.export_status = Some(format!("Exported {} files to moxfield_export/", count));
    }
}

/// Windows 95-style title bar with navy background.
fn title_bar(ui: &mut egui::Ui, title: &str) {
    let navy = egui::Color32::from_rgb(0, 0, 128);
    egui::Frame::new()
        .fill(navy)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .monospace()
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("[X]")
                            .monospace()
                            .color(egui::Color32::WHITE),
                    );
                });
            });
        });
}

/// A retro raised-border group box.
fn retro_group(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(
            2.0,
            egui::Color32::from_rgb(128, 128, 128),
        ))
        .inner_margin(egui::Margin::same(8))
        .show(ui, add_contents);
}

/// A plain horizontal separator line.
fn separator(ui: &mut egui::Ui) {
    let gray = egui::Color32::from_rgb(128, 128, 128);
    ui.painter().hline(
        ui.available_rect_before_wrap().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(1.0, gray),
    );
    ui.add_space(4.0);
}

/// One card row: name in rarity color, optional [F] foil badge, rarity label right-aligned.
fn card_row(ui: &mut egui::Ui, card: &OwnedPackCard) {
    let (rarity_label, color) = rarity_color(&card.rarity);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&card.name).monospace().color(color));
        if card.foil {
            ui.label(
                egui::RichText::new("[F]")
                    .monospace()
                    .color(egui::Color32::from_rgb(100, 180, 255)),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(rarity_label.to_uppercase())
                    .monospace()
                    .color(color),
            );
        });
    });
}

fn moxfield_line(card: &OwnedPackCard) -> String {
    let foil = if card.foil { "*F* " } else { "" };
    format!(
        "1 {}{} ({}) {}",
        foil,
        card.name,
        card.set_code.to_uppercase(),
        card.number,
    )
}

fn rarity_color(rarity: &str) -> (&'static str, egui::Color32) {
    match rarity {
        "uncommon" => ("uncommon", egui::Color32::from_rgb(160, 160, 160)),
        "rare" => ("rare", egui::Color32::from_rgb(180, 140, 20)),
        "mythic" => ("mythic", egui::Color32::from_rgb(200, 80, 0)),
        _ => ("common", egui::Color32::from_rgb(80, 80, 80)),
    }
}

fn apply_retro_theme(ctx: &egui::Context) {
    let gray = egui::Color32::from_rgb(192, 192, 192);
    let dark_gray = egui::Color32::from_rgb(128, 128, 128);
    let darker_gray = egui::Color32::from_rgb(64, 64, 64);
    let navy = egui::Color32::from_rgb(0, 0, 128);
    let white = egui::Color32::WHITE;
    let black = egui::Color32::BLACK;
    let zero = egui::CornerRadius::ZERO;

    let mut visuals = egui::Visuals::light();

    visuals.panel_fill = gray;
    visuals.window_fill = gray;
    visuals.faint_bg_color = egui::Color32::from_rgb(212, 208, 200);
    visuals.extreme_bg_color = white;
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.window_corner_radius = zero;
    visuals.menu_corner_radius = zero;
    visuals.window_stroke = egui::Stroke::new(2.0, darker_gray);

    visuals.selection.bg_fill = navy;
    visuals.selection.stroke = egui::Stroke::new(1.0, white);

    // noninteractive
    visuals.widgets.noninteractive.bg_fill = gray;
    visuals.widgets.noninteractive.weak_bg_fill = gray;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, dark_gray);
    visuals.widgets.noninteractive.corner_radius = zero;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, black);

    // inactive (buttons at rest)
    visuals.widgets.inactive.bg_fill = gray;
    visuals.widgets.inactive.weak_bg_fill = gray;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(2.0, white);
    visuals.widgets.inactive.corner_radius = zero;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, black);

    // hovered
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(210, 210, 210);
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(210, 210, 210);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, dark_gray);
    visuals.widgets.hovered.corner_radius = zero;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, black);

    // active (pressed)
    visuals.widgets.active.bg_fill = dark_gray;
    visuals.widgets.active.weak_bg_fill = dark_gray;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, darker_gray);
    visuals.widgets.active.corner_radius = zero;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, black);

    // open (combo/collapsing)
    visuals.widgets.open.bg_fill = gray;
    visuals.widgets.open.weak_bg_fill = gray;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, dark_gray);
    visuals.widgets.open.corner_radius = zero;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, black);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::proportional(18.0),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::monospace(13.0),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::monospace(13.0),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::monospace(13.0),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::monospace(11.0),
        ),
    ]
    .into();
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    ctx.set_style(style);
}

impl eframe::App for LimitedForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_retro_theme(ctx);
        match &self.screen {
            Screen::Loading => self.show_loading(ctx),
            Screen::Setup => self.show_setup(ctx),
            Screen::Results { .. } => self.show_results(ctx),
        }
    }
}
