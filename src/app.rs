use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eframe::egui;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::data;
use crate::mtgjson::AllPrintings;
use crate::pack::{OwnedPackCard, PackGenerator};

const MTGJSON_URL: &str = "https://mtgjson.com/api/v5/AllPrintings.json";

#[derive(PartialEq, Clone, Copy)]
enum Format {
    Limited,
    PreRelease,
}

enum DownloadMsg {
    Progress { downloaded: u64, total: u64 },
    Done(String),
    Err(String),
}

enum Screen {
    DataSource,
    Downloading,
    Loading,
    Setup,
    Results {
        packs: Vec<Vec<Vec<OwnedPackCard>>>, // [player][slot][card]
        promos: Vec<OwnedPackCard>,          // one per player
        slot_names: Vec<String>,             // set name per slot
    },
}

pub struct LimitedForgeApp {
    screen: Screen,
    load_rx: Option<mpsc::Receiver<Result<AllPrintings, String>>>,
    download_rx: Option<mpsc::Receiver<DownloadMsg>>,
    download_progress: (u64, u64), // (downloaded, total)
    all_printings: Option<AllPrintings>,
    sets: Vec<(String, String)>, // (code, name) sorted by name
    data_path: String,

    // Setup form
    format: Format,
    set_query: String,
    selected_sets: Vec<(String, String, usize)>, // pack slots: (code, name, count)
    predictions: Vec<(String, String)>,
    num_players: usize,

    // Loading animation
    tick: u64,

    error: Option<String>,
    export_status: Option<String>,
}

/// Returns the path to look for AllPrintings.json at startup.
/// Checks next to the executable first (distribution mode), then falls back to the dev path.
fn default_data_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("AllPrintings.json");
            if p.exists() {
                return p.to_string_lossy().into_owned();
            }
        }
    }
    "src/AllPrintings.json".to_string()
}

/// Where a downloaded AllPrintings.json will be saved (next to the executable).
fn download_save_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("AllPrintings.json")))
        .unwrap_or_else(|| std::path::PathBuf::from("AllPrintings.json"))
}

impl LimitedForgeApp {
    pub fn new() -> Self {
        let default_path = default_data_path();
        let file_exists = std::path::Path::new(&default_path).exists();
        let (screen, load_rx) = if file_exists {
            (Screen::Loading, Some(Self::start_load(&default_path)))
        } else {
            (Screen::DataSource, None)
        };
        Self {
            screen,
            load_rx,
            download_rx: None,
            download_progress: (0, 0),
            all_printings: None,
            sets: Vec::new(),
            data_path: default_path,
            format: Format::Limited,
            set_query: String::new(),
            selected_sets: Vec::new(),
            predictions: Vec::new(),
            num_players: 8,
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

    fn start_download() -> mpsc::Receiver<DownloadMsg> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let resp = match ureq::get(MTGJSON_URL).call() {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(DownloadMsg::Err(e.to_string()));
                    return;
                }
            };
            let total: u64 = resp
                .header("Content-Length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let save_path = download_save_path();
            let mut file = match std::fs::File::create(&save_path) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(DownloadMsg::Err(format!(
                        "Cannot create file {}: {}",
                        save_path.display(),
                        e
                    )));
                    return;
                }
            };
            let mut reader = resp.into_reader();
            let mut buf = [0u8; 65536];
            let mut downloaded = 0u64;
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => {
                        let _ = tx.send(DownloadMsg::Err(e.to_string()));
                        return;
                    }
                };
                if let Err(e) = file.write_all(&buf[..n]) {
                    let _ = tx.send(DownloadMsg::Err(e.to_string()));
                    return;
                }
                downloaded += n as u64;
                let _ = tx.send(DownloadMsg::Progress { downloaded, total });
            }
            let _ = tx.send(DownloadMsg::Done(save_path.to_string_lossy().into_owned()));
        });
        rx
    }

    fn reload(&mut self, path: String) {
        self.data_path = path;
        self.all_printings = None;
        self.sets.clear();
        self.set_query.clear();
        self.selected_sets.clear();
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

    fn show_data_source(&mut self, ctx: &egui::Context) {
        let mut new_path: Option<String> = None;
        let mut start_download = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            title_bar(ui, "LimitedForge - Setup");
            ui.add_space(10.0);

            retro_group(ui, |ui| {
                ui.label(
                    egui::RichText::new("! NO CARD DATA FOUND")
                        .monospace()
                        .strong()
                        .color(egui::Color32::RED),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "AllPrintings.json was not found. Please download it\n\
                         from MTGJSON or provide a local copy.",
                    )
                    .monospace(),
                );
            });

            ui.add_space(10.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("DATA SOURCE").monospace().strong());
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("[ DOWNLOAD FROM MTGJSON ]")
                                .monospace()
                                .strong(),
                        )
                        .clicked()
                    {
                        start_download = true;
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("~500 MB — downloads AllPrintings.json")
                            .monospace()
                            .weak(),
                    );
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("[ BROWSE LOCAL FILE... ]").monospace())
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .set_title("Select AllPrintings.json")
                            .pick_file()
                        {
                            new_path = Some(path.to_string_lossy().into_owned());
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Use an existing local AllPrintings.json")
                            .monospace()
                            .weak(),
                    );
                });
            });

            if let Some(err) = &self.error {
                ui.add_space(8.0);
                retro_group(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("ERROR: {}", err))
                            .monospace()
                            .color(egui::Color32::RED),
                    );
                });
            }
        });

        if start_download {
            self.error = None;
            self.download_progress = (0, 0);
            self.download_rx = Some(Self::start_download());
            self.screen = Screen::Downloading;
        }
        if let Some(path) = new_path {
            self.reload(path);
        }
    }

    fn show_downloading(&mut self, ctx: &egui::Context) {
        // Poll the download channel
        let mut done_path: Option<String> = None;
        if let Some(rx) = &self.download_rx {
            loop {
                match rx.try_recv() {
                    Ok(DownloadMsg::Progress { downloaded, total }) => {
                        self.download_progress = (downloaded, total);
                    }
                    Ok(DownloadMsg::Done(path)) => {
                        done_path = Some(path);
                        break;
                    }
                    Ok(DownloadMsg::Err(e)) => {
                        self.error = Some(e);
                        self.download_rx = None;
                        self.screen = Screen::DataSource;
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.error = Some("Download thread disconnected.".into());
                        self.download_rx = None;
                        self.screen = Screen::DataSource;
                        return;
                    }
                }
            }
        }

        ctx.request_repaint_after(Duration::from_millis(100));
        self.tick = self.tick.wrapping_add(1);

        let (downloaded, total) = self.download_progress;

        egui::CentralPanel::default().show(ctx, |ui| {
            title_bar(ui, "LimitedForge - Downloading");
            ui.add_space(40.0);
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Downloading AllPrintings.json...")
                            .monospace()
                            .size(14.0),
                    );
                    ui.add_space(8.0);

                    let progress_text = if total > 0 {
                        let pct = (downloaded as f64 / total as f64 * 100.0) as u64;
                        let bar_width = 20usize;
                        let filled = (downloaded as f64 / total as f64 * bar_width as f64) as usize;
                        format!(
                            "[{}{}] {}%  ({:.1} / {:.1} MB)",
                            "█".repeat(filled),
                            "░".repeat(bar_width - filled),
                            pct,
                            downloaded as f64 / 1_048_576.0,
                            total as f64 / 1_048_576.0,
                        )
                    } else {
                        let bar_width = 20usize;
                        let filled = ((self.tick / 2) % (bar_width as u64 + 1)) as usize;
                        format!(
                            "[{}{}]  {:.1} MB received",
                            "█".repeat(filled),
                            "░".repeat(bar_width - filled),
                            downloaded as f64 / 1_048_576.0,
                        )
                    };

                    ui.label(egui::RichText::new(progress_text).monospace().size(13.0));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Source: mtgjson.com")
                            .monospace()
                            .size(11.0)
                            .weak(),
                    );
                });
            });
        });

        if let Some(path) = done_path {
            self.download_rx = None;
            self.reload(path);
        }
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
        // Slot to add/increment when user clicks a prediction
        let mut add_slot: Option<(String, String)> = None;
        let mut decrement_idx: Option<usize> = None;
        let mut increment_idx: Option<usize> = None;

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
                    if ui
                        .selectable_label(
                            self.format == Format::Limited,
                            egui::RichText::new("Limited").monospace(),
                        )
                        .clicked()
                    {
                        self.format = Format::Limited;
                    }
                    ui.scope(|ui| {
                        let red = egui::Color32::from_rgb(180, 0, 0);
                        let dark_red = egui::Color32::from_rgb(120, 0, 0);
                        ui.visuals_mut().widgets.inactive.bg_fill = red;
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = red;
                        ui.visuals_mut().widgets.hovered.bg_fill = dark_red;
                        ui.visuals_mut().widgets.hovered.weak_bg_fill = dark_red;
                        ui.visuals_mut().widgets.active.bg_fill = dark_red;
                        ui.visuals_mut().widgets.active.weak_bg_fill = dark_red;
                        ui.visuals_mut().selection.bg_fill = dark_red;
                        if ui
                            .selectable_label(
                                self.format == Format::PreRelease,
                                egui::RichText::new("Pre-Release").monospace(),
                            )
                            .clicked()
                        {
                            self.format = Format::PreRelease;
                            self.num_players = 1;
                        }
                    });
                });
            });

            ui.add_space(6.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("SET SLOTS").monospace().strong());
                ui.add_space(4.0);

                // Search input
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Set:").monospace());
                    ui.add_space(4.0);
                    let response = ui.text_edit_singleline(&mut self.set_query);
                    if response.changed() {
                        self.update_predictions();
                    }
                });

                // Autocomplete dropdown — clicking a prediction adds it as a slot
                if !self.predictions.is_empty() {
                    let predictions = self.predictions.clone();
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        for (code, name) in &predictions {
                            let label = format!("{} ({})", name, code);
                            if ui
                                .selectable_label(false, egui::RichText::new(&label).monospace())
                                .clicked()
                            {
                                add_slot = Some((code.clone(), name.clone()));
                            }
                        }
                    });
                }

                // Slot list
                if !self.selected_sets.is_empty() {
                    ui.add_space(4.0);
                    let slots: Vec<(usize, String, String, usize)> = self
                        .selected_sets
                        .iter()
                        .enumerate()
                        .map(|(i, (c, n, cnt))| (i, c.clone(), n.clone(), *cnt))
                        .collect();
                    for (idx, code, name, count) in &slots {
                        ui.horizontal(|ui| {
                            if ui
                                .button(egui::RichText::new("[ - ]").monospace())
                                .clicked()
                            {
                                decrement_idx = Some(*idx);
                            }
                            if ui
                                .button(egui::RichText::new("[ + ]").monospace())
                                .clicked()
                            {
                                increment_idx = Some(*idx);
                            }
                            ui.label(
                                egui::RichText::new(format!("{}  {} ({})", count, name, code))
                                    .monospace(),
                            );
                        });
                    }
                } else {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "No slots added. Search for a set and click it to add.",
                        )
                        .monospace()
                        .weak(),
                    );
                }
            });

            ui.add_space(6.0);

            retro_group(ui, |ui| {
                ui.label(egui::RichText::new("PLAYERS").monospace().strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Players:").monospace());
                    if self.format == Format::PreRelease {
                        ui.label(egui::RichText::new("1  (Pre-Release)").monospace().weak());
                    } else {
                        ui.add(egui::Slider::new(&mut self.num_players, 1..=16));
                    }
                });
            });

            ui.add_space(10.0);

            let can_generate = !self.selected_sets.is_empty();
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

        let pack_cap = if self.format == Format::PreRelease { 6 } else { usize::MAX };
        let total_packs: usize = self.selected_sets.iter().map(|(_, _, c)| c).sum();

        if let Some((code, name)) = add_slot {
            if total_packs < pack_cap {
                if let Some(entry) = self.selected_sets.iter_mut().find(|(c, _, _)| *c == code) {
                    entry.2 += 1;
                } else {
                    self.selected_sets.push((code, name, 1));
                }
            }
            self.set_query.clear();
            self.predictions.clear();
        }
        if let Some(idx) = decrement_idx {
            self.selected_sets[idx].2 = self.selected_sets[idx].2.saturating_sub(1);
            if self.selected_sets[idx].2 == 0 {
                self.selected_sets.remove(idx);
            }
        }
        if let Some(idx) = increment_idx {
            if total_packs < pack_cap {
                self.selected_sets[idx].2 += 1;
            }
        }
        if let Some(path) = new_path {
            self.reload(path);
        }
    }

    fn generate_packs(&mut self) {
        if self.selected_sets.is_empty() {
            return;
        }
        let all_printings = match &self.all_printings {
            Some(ap) => ap,
            None => return,
        };

        // Build one PackGenerator per unique set code
        let mut generators: std::collections::HashMap<String, PackGenerator<'_>> =
            std::collections::HashMap::new();
        for (code, _name, _count) in &self.selected_sets {
            if !generators.contains_key(code) {
                match PackGenerator::new(code, &all_printings.data) {
                    Ok(g) => {
                        generators.insert(code.clone(), g);
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                }
            }
        }

        let mut rng = StdRng::from_entropy();
        let num_players = self.num_players;

        // Expand (code, name, count) into flat (code, name) slot list
        let slots: Vec<(String, String)> = self
            .selected_sets
            .iter()
            .flat_map(|(code, name, count)| {
                std::iter::repeat((code.clone(), name.clone())).take(*count)
            })
            .collect();
        let slot_names: Vec<String> = slots.iter().map(|(_, name)| name.clone()).collect();

        let mut player_packs: Vec<Vec<Vec<OwnedPackCard>>> = Vec::new();
        for _ in 0..num_players {
            let mut packs = Vec::new();
            for (code, _name) in &slots {
                if let Some(generator) = generators.get(code) {
                    let pack = generator.generate_pack(&mut rng);
                    let owned: Vec<OwnedPackCard> = pack.iter().map(OwnedPackCard::from).collect();
                    packs.push(owned);
                }
            }
            player_packs.push(packs);
        }

        self.export_status = None;
        self.screen = Screen::Results {
            packs: player_packs,
            promos: Vec::new(),
            slot_names,
        };
    }

    fn show_results(&mut self, ctx: &egui::Context) {
        let num_players = self.num_players;
        let export_status = self.export_status.clone();

        let mut go_back = false;
        let mut export_dir: Option<std::path::PathBuf> = None;

        {
            let (player_packs, promos, slot_names) = match &self.screen {
                Screen::Results { packs, promos, slot_names } => (packs, promos, slot_names),
                _ => return,
            };
            let slot_count = slot_names.len();

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
                            "{} PLAYERS | {} PACKS",
                            num_players, slot_count,
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
                                if let Some(dir) = rfd::FileDialog::new()
                                    .set_title("Choose export folder")
                                    .pick_folder()
                                {
                                    export_dir = Some(dir);
                                }
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
                        let player_label = format!(">> PLAYER {}", p_idx + 1);
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
                                let set_label = slot_names
                                    .get(pk_idx)
                                    .map(|s| format!(" — {}", s))
                                    .unwrap_or_default();
                                let pack_label = format!("  [PACK {}{}]", pk_idx + 1, set_label);
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
        if let Some(dir) = export_dir {
            self.export_to_moxfield(dir);
        }
    }

    fn export_to_moxfield(&mut self, dir: std::path::PathBuf) {
        let player_lines: Vec<Vec<String>> = match &self.screen {
            Screen::Results { packs, promos, .. } => packs
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

        let count = player_lines.len();
        for (p_idx, lines) in player_lines.iter().enumerate() {
            let path = dir.join(format!("player_{}.txt", p_idx + 1));
            if let Err(e) = std::fs::write(&path, lines.join("\n")) {
                self.export_status = Some(format!("Export failed: {}", e));
                return;
            }
        }

        self.export_status = Some(format!(
            "Exported {} files to {}",
            count,
            dir.display()
        ));
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
            Screen::DataSource => self.show_data_source(ctx),
            Screen::Downloading => self.show_downloading(ctx),
            Screen::Loading => self.show_loading(ctx),
            Screen::Setup => self.show_setup(ctx),
            Screen::Results { .. } => self.show_results(ctx),
        }
    }
}
