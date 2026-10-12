use crate::{
    avi::VideoFormat,
    config::{Config, Method, Overrides, Point, PrimaryEdge, SourceConfig},
    engine::{Engine, Snapshot, StartOptions, State},
    geometry::{self, Handle, move_handle},
    persistence::Autosaver,
    tracker::NAMES,
};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const COLORS: [Color32; 4] = [
    Color32::from_rgb(247, 93, 99),
    Color32::from_rgb(87, 216, 115),
    Color32::from_rgb(224, 107, 239),
    Color32::from_rgb(86, 216, 227),
];
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    offset: Point,
}
// Every visible handle is interactive, independent of the Settings selection.
fn handles(config: &Config) -> Vec<(Handle, Point, Color32)> {
    let mut points = Vec::new();
    if !config.sectors().iter().any(|s| s.enabled) {
        return points;
    }
    // These two points calibrate wing angles too; they are not body tracking ROIs.
    points.extend([
        (Handle::Head, config.head, COLORS[2]),
        (Handle::Tail, config.tail, COLORS[3]),
        (
            Handle::Body,
            geometry::center(config),
            Color32::from_gray(225),
        ),
    ]);
    for (i, s) in config
        .sectors()
        .into_iter()
        .enumerate()
        .filter(|(_, s)| s.enabled)
    {
        let middle = (s.start_deg + s.span_deg * 0.5).to_radians();
        points.extend([
            (Handle::Hinge(i), s.hinge, COLORS[i]),
            (
                Handle::Start(i),
                s.hinge.at(s.outer_radius, s.start_deg.to_radians()),
                COLORS[i],
            ),
            (
                Handle::End(i),
                s.hinge
                    .at(s.outer_radius, (s.start_deg + s.span_deg).to_radians()),
                COLORS[i],
            ),
            (
                Handle::Inner(i),
                s.hinge.at(s.inner_radius, middle),
                COLORS[i],
            ),
            (
                Handle::Outer(i),
                s.hinge.at(s.outer_radius, middle),
                COLORS[i],
            ),
        ]);
    }
    points
}
pub struct App {
    /// The effective configuration: the saved file plus this run's overrides.
    config: Config,
    /// The file's values for fields that `overrides` replaces.
    saved: Config,
    overrides: Overrides,
    path: PathBuf,
    engine: Option<Engine>,
    options: StartOptions,
    title: String,
    selected: usize,
    texture: Option<egui::TextureHandle>,
    latest: Option<Arc<Snapshot>>,
    last_sequence: Option<u64>,
    last_display: Option<(u64, bool, bool)>,
    history: VecDeque<(f32, Option<f32>, Option<f32>)>,
    drag: Option<Drag>,
    last_wing: usize,
    show_difference: bool,
    show_settings: bool,
    show_diagnostics: bool,
    dirty: bool,
    status: String,
    restart_pending: bool,
    cameras: Vec<String>,
    camera_query: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    started: Instant,
    autosaver: Option<Autosaver>,
    last_autosave: Config,
    shutdown: Arc<AtomicBool>,
}
fn app_icon() -> egui::IconData {
    let icon = image::load_from_memory(include_bytes!("../assets/rekke-64.png"))
        .expect("Bundled app icon")
        .into_rgba8();
    egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    }
}
/// Everything the GUI needs to start one instance.
pub struct Launch {
    /// The configuration as stored in `path`.
    pub saved: Config,
    /// This run's temporary settings (command line, rig manifest, video).
    pub overrides: Overrides,
    pub path: PathBuf,
    pub options: StartOptions,
    pub screenshot: Option<PathBuf>,
    /// Panels to open at start: "settings", "plots".
    pub panels: Vec<String>,
    pub shutdown: Arc<AtomicBool>,
}
fn window_title(config: &Config) -> String {
    let source = match &config.source {
        SourceConfig::Camera { device, .. } => format!("camera {device}"),
        SourceConfig::Video { path, .. } => std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "video".into()),
        SourceConfig::Network { .. } => "stream".into(),
        SourceConfig::Demo { .. } => "simulated fly".into(),
    };
    let port = if config.output.zmq_enabled {
        config
            .output
            .zmq_bind
            .rsplit_once(':')
            .map(|(_, p)| format!(" · ZMQ {p}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    format!("Rekke · {} · {source}{port}", config.instance)
}
pub fn launch(launch: Launch) -> eframe::Result<()> {
    let Launch {
        saved,
        overrides,
        path,
        options: start_options,
        screenshot,
        panels,
        shutdown,
    } = launch;
    let config = overrides.apply(&saved);
    let title = window_title(&config);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_app_id("rekke")
            .with_inner_size([1000.0, 820.0])
            .with_min_inner_size([720.0, 550.0])
            .with_drag_and_drop(true)
            .with_icon(app_icon()),
        ..Default::default()
    };
    let app_title = title.clone();
    eframe::run_native(
        &app_title,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.style_mut(|s| {
                s.spacing.item_spacing = Vec2::new(8.0, 6.0);
                s.spacing.button_padding = Vec2::new(8.0, 4.0);
                s.visuals.panel_fill = Color32::from_rgb(22, 25, 29);
                s.visuals.window_fill = Color32::from_rgb(27, 30, 35);
                s.visuals.selection.bg_fill = Color32::from_rgb(48, 65, 72);
            });
            let mut app = App {
                autosaver: Some(Autosaver::new(path.clone())),
                last_autosave: config.clone(),
                shutdown,
                config,
                saved,
                overrides,
                path,
                engine: None,
                options: start_options,
                title,
                selected: 0,
                texture: None,
                latest: None,
                last_sequence: None,
                last_display: None,
                history: VecDeque::new(),
                drag: None,
                last_wing: 0,
                show_difference: false,
                show_settings: panels.iter().any(|p| p == "settings"),
                show_diagnostics: panels.iter().any(|p| p == "plots"),
                dirty: false,
                status: String::new(),
                restart_pending: false,
                cameras: Vec::new(),
                camera_query: None,
                screenshot,
                screenshot_requested: false,
                started: Instant::now(),
            };
            app.start();
            Ok(Box::new(app))
        }),
    )
}
impl App {
    fn start(&mut self) {
        match Engine::start_with(self.config.clone(), self.path.clone(), self.options.clone()) {
            Ok(engine) => {
                self.engine = Some(engine);
                self.latest = None;
                self.texture = None;
                self.last_sequence = None;
                self.last_display = None;
                self.history.clear();
                self.status.clear();
            }
            Err(e) => self.status = format!("{e:#}"),
        }
    }
    fn apply(&mut self) {
        self.dirty = true;
        if let Some(e) = &self.engine
            && let Err(error) = e.update(&self.config)
        {
            self.status = format!("{error:#}");
        }
    }
    /// The configuration to write: GUI edits without this run's overrides.
    fn persistable(&self) -> Config {
        self.overrides.persistable(&self.config, &self.saved)
    }
    fn save(&mut self) {
        let config = self.persistable();
        if let Some(saver) = &self.autosaver {
            match config.validate() {
                Ok(()) => {
                    saver.save_now(config);
                    self.status = "Saving configuration…".into();
                }
                Err(error) => self.status = format!("{error:#}"),
            }
            return;
        }
        match config.save(&self.path) {
            Ok(()) => {
                self.dirty = false;
                self.status = "Configuration saved".into();
            }
            Err(e) => self.status = format!("{e:#}"),
        }
    }
    fn request_restart(&mut self) {
        if let Some(e) = &self.engine {
            e.stop();
            self.restart_pending = true;
            self.status = "Restarting…".into();
        } else {
            self.start();
        }
    }
    fn observe(&mut self, ctx: &egui::Context, state: &State) {
        let Some(s) = &state.latest else {
            return;
        };
        if self.last_sequence != Some(s.frame.sequence) {
            self.history.push_back((
                s.measurement.elapsed_ns as f32 / 1e9,
                s.measurement.left.angle_rad,
                s.measurement.right.angle_rad,
            ));
            while self.history.len() > 320 {
                self.history.pop_front();
            }
            self.last_sequence = Some(s.frame.sequence);
        }
        self.latest = Some(s.clone());
        let inverted = !self.config.tracking.dark_on_light;
        let display = (s.frame.sequence, self.show_difference, inverted);
        if self.last_display == Some(display) {
            return;
        }
        self.last_display = Some(display);
        let frame = if self.show_difference {
            s.foreground.as_ref().unwrap_or(&s.frame)
        } else {
            &s.frame
        };
        let pixels = if inverted && !self.show_difference {
            frame.pixels.iter().map(|x| 255 - x).collect::<Vec<_>>()
        } else {
            frame.pixels.clone()
        };
        let image =
            egui::ColorImage::from_gray([frame.width as usize, frame.height as usize], &pixels);
        if let Some(texture) = &mut self.texture {
            texture.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.texture = Some(ctx.load_texture("camera", image, egui::TextureOptions::LINEAR));
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context, state: &State) {
        let mut changed = false;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("rekke").strong().size(16.0));
                ui.separator();
                if ui.add_enabled(self.latest.is_some() && !state.finished, egui::Button::new("Save BG"))
                    .on_hover_text("Save the next camera frame as the background. Capture an empty scene first.").clicked()
                    && let Some(e) = &self.engine && let Err(error) = e.save_background() { self.status = format!("{error:#}"); }
                ui.label(egui::RichText::new("Track").small().color(Color32::GRAY));
                for (i, label) in [(2, "H"), (3, "A"), (0, "L"), (1, "R")] {
                    changed |= ui.checkbox(&mut self.config.sector_mut(i).enabled, egui::RichText::new(label).color(COLORS[i]))
                        .on_hover_text(format!("{} tracking and overlay", NAMES[i])).changed();
                }
                ui.separator();
                let mut asymmetric = self.config.tracking.asymmetric;
                if ui.checkbox(&mut asymmetric, "Asymmetric")
                    .on_hover_text("Off: mirror wing geometry and move regions with the body. On: edit independently. Turning off mirrors from the last edited wing.").changed() {
                    match geometry::set_asymmetric(&mut self.config, asymmetric, self.last_wing) {
                        Ok(()) => changed = true,
                        Err(e) => self.status = format!("{e:#}"),
                    }
                }
                changed |= ui.add_enabled(state.background_loaded, egui::Checkbox::new(&mut self.config.tracking.subtract_background, "Subtract"))
                    .on_hover_text("Track differences from the saved background").changed();
                let mut inverted = !self.config.tracking.dark_on_light;
                if ui.checkbox(&mut inverted, "Invert").on_hover_text("Bright insect on dark background; invert the preview too").changed() {
                    self.config.tracking.dark_on_light = !inverted; changed = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.show_settings, "Settings");
                    // Rigs started by the launcher are managed as a set; no extra windows there.
                    if self.options.fallback && ui.button("New window").on_hover_text("Open another Rekke window for the next rig config (its own camera and ports)").clicked() { self.new_window(); }
                    if ui.add_enabled(self.engine.as_ref().is_some_and(|e| !e.is_finished()), egui::Button::new(if state.recording.active { "Stop REC" } else { "Record" })).on_hover_text(format!("Record the camera image (no overlay) as {} on a dedicated writer. Settings → Video to change.", self.config.recording.format.label())).clicked() {
                        // A run-time toggle: never saved, so the next launch does not start recording by surprise.
                        if let Some(e) = &self.engine && let Err(error) = e.record(!state.recording.active) { self.status = format!("{error:#}"); }
                    }
                    if ui.button(if self.dirty { "Save •" } else { "Save" }).on_hover_text("Save this instance's configuration").clicked() { self.save(); }
                    let running = self.engine.as_ref().is_some_and(|e| !e.is_finished());
                    if ui.button(if running { "Stop" } else { "Start" }).clicked() {
                        if running { self.engine.as_ref().unwrap().stop(); self.status = "Stopping…".into(); }
                        else { self.engine.take(); self.start(); }
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.small("Threshold").on_hover_text("0 selects the strongest measurable edge, including faint signals. Raise to reject noise; changes apply live.");
                for (i, label) in [(0, "L"), (1, "R")] {
                    ui.colored_label(COLORS[i], label);
                    let s = self.config.sector_mut(i);
                    changed |= ui.add_enabled(s.enabled, egui::DragValue::new(&mut s.threshold).range(0.0..=1.0).speed(0.0001).fixed_decimals(4))
                        .on_hover_text("Drag to adjust, or click to type. 0 = any measurable edge.").changed();
                }
                ui.separator();
                let mut reject = self.config.tracking.left.method == Method::WingBlur && self.config.tracking.right.method == Method::WingBlur;
                if ui.checkbox(&mut reject, "Ignore legs").on_hover_text("Robust wing edges for both wings: legs, antennae and other thin dark features are ignored (recommended). Off: classic strongest edge. Tune in Settings.").changed() {
                    self.config.tracking.left.method = if reject { Method::WingBlur } else { Method::Edge };
                    self.config.tracking.right.method = if reject { Method::WingBlur } else { Method::Edge };
                    changed = true;
                }
                ui.add_enabled(state.background_loaded && self.config.tracking.subtract_background, egui::Checkbox::new(&mut self.show_difference, "Difference view"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small("Drag any dot to edit").on_hover_text("Drag the white center dot to move the setup. Head/tail drags rotate regions around the updated midline. Corner dots change angle and radius together.");
                });
            });
        });
        if changed {
            self.apply();
        }
    }
    fn settings(&mut self, ctx: &egui::Context, state: &State) {
        if !self.show_settings {
            return;
        }
        let mut open = self.show_settings;
        let mut changed = false;
        egui::Window::new("Settings").open(&mut open).default_width(360.0).default_pos([620.0, 95.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(620.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, name) in ["Left", "Right", "Head", "Abdomen"].into_iter().enumerate() {
                        ui.selectable_value(&mut self.selected, i, egui::RichText::new(name).color(COLORS[i]));
                    }
                });
                let old = self.config.sectors()[self.selected].clone();
                let mut edited = old.clone();
                let s = &mut edited;
                egui::ComboBox::from_label("Detector").selected_text(match s.method { Method::Edge => "Classic strongest edge", Method::WingBlur => "Robust · ignores legs & antennae", Method::Centroid => "Centroid" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.method, Method::WingBlur, "Robust · ignores legs & antennae");
                    ui.selectable_value(&mut s.method, Method::Edge, "Classic strongest edge");
                    ui.selectable_value(&mut s.method, Method::Centroid, "Centroid");
                });
                if s.method == Method::WingBlur {
                    ui.horizontal(|ui| { ui.label("Ignore features narrower than"); ui.add(egui::DragValue::new(&mut s.spike_width_deg).range(0.0..=45.0).speed(0.1).suffix("°")); });
                    ui.small("Thin dark features (legs, antennae) are removed per radial band, then the median edge across bands is used. Keep this narrower than the wing's blur.");
                    ui.horizontal(|ui| { ui.label("Contrast cap"); ui.add(egui::DragValue::new(&mut s.contrast_cap).range(0.001..=1.0).speed(0.001).fixed_decimals(3)); });
                }
                if s.method != Method::Centroid {
                    egui::ComboBox::from_label("Primary edge").selected_text(match s.primary_edge { PrimaryEdge::Strongest => "Strongest", PrimaryEdge::TowardHead => "Toward head", PrimaryEdge::TowardTail => "Toward tail" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.primary_edge, PrimaryEdge::Strongest, "Strongest");
                        ui.selectable_value(&mut s.primary_edge, PrimaryEdge::TowardHead, "Toward head");
                        ui.selectable_value(&mut s.primary_edge, PrimaryEdge::TowardTail, "Toward tail");
                    });
                }
                let (w, h, _) = self.config.source.dimensions();
                let s = &mut edited;
                egui::Grid::new("region_parameters").num_columns(2).show(ui, |ui| {
                    ui.label("Threshold"); ui.add(egui::DragValue::new(&mut s.threshold).range(0.0..=1.0).speed(0.0001).fixed_decimals(4)); ui.end_row();
                    ui.label("Hinge X / Y"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.hinge.x).range(0.0..=w as f32 - 1.0).speed(0.5)); ui.add(egui::DragValue::new(&mut s.hinge.y).range(0.0..=h as f32 - 1.0).speed(0.5)); }); ui.end_row();
                    ui.label("Start / span"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.start_deg).range(-3600.0..=3600.0).speed(0.5).suffix("°")); ui.add(egui::DragValue::new(&mut s.span_deg).range(5.0..=355.0).speed(0.5).suffix("°")); }); ui.end_row();
                    ui.label("Inner / outer"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.inner_radius).range(0.0..=s.outer_radius - 2.0).speed(0.5)); ui.add(egui::DragValue::new(&mut s.outer_radius).range(s.inner_radius + 2.0..=(w as f32).hypot(h as f32)).speed(0.5)); }); ui.end_row();
                    ui.label("Max edges"); ui.add(egui::DragValue::new(&mut s.max_edges).range(1..=8)); ui.end_row();
                });
                if *s != old {
                    match geometry::edit_sector(&mut self.config, self.selected, edited) {
                        Ok(geometry_changed) => {
                            if geometry_changed && self.selected < 2 { self.last_wing = self.selected; }
                            changed = true;
                        }
                        Err(e) => self.status = format!("{e:#}"),
                    }
                }
                ui.checkbox(&mut self.show_diagnostics, "Show signal diagnostics");
                if let Some(snapshot) = &self.latest {
                    let p = [&snapshot.measurement.left, &snapshot.measurement.right, &snapshot.measurement.head, &snapshot.measurement.abdomen][self.selected];
                    ui.small(format!("Peak {:.5} · selected {:.5} · coverage {:.0}%", p.peak_gradient, p.strength, p.coverage * 100.0));
                }
                ui.separator();
                egui::CollapsingHeader::new("Background").show(ui, |ui| {
                    ui.small(if state.background_loaded { "Reference loaded" } else { "No reference loaded" });
                    ui.small("Save BG captures the next frame. Use an empty scene before placing the insect.");
                    if ui.button("Load saved background").clicked() && let Some(e) = &self.engine && let Err(error) = e.load_background() { self.status = format!("{error:#}"); }
                    if matches!(self.config.source, SourceConfig::Demo { .. }) && ui.button("Use clean demo background").clicked() && let Some(e) = &self.engine && let Err(error) = e.demo_background() { self.status = format!("{error:#}"); }
                    self.dirty |= ui.text_edit_singleline(&mut self.config.tracking.background_path).changed();
                    ui.small("Restart to apply a changed background path.");
                });
                egui::CollapsingHeader::new("Camera / input").show(ui, |ui| { self.source_controls(ui); });
                egui::CollapsingHeader::new("Instance / output").show(ui, |ui| {
                    let o = &self.overrides;
                    let (label_free, zmq_free, udp_free) = (o.label.is_none(), o.zmq_port.is_none(), o.udp_port.is_none() && o.udp_target.is_none());
                    if !(label_free && zmq_free && udp_free) { ui.small("Greyed fields are set for this run by the rig launcher or command line."); }
                    ui.label("Instance label"); self.dirty |= ui.add_enabled(label_free, egui::TextEdit::singleline(&mut self.config.instance)).changed();
                    self.dirty |= ui.add_enabled(zmq_free, egui::Checkbox::new(&mut self.config.output.zmq_enabled, "ZMQ · Kinefly bridge format")).changed();
                    ui.label("ZMQ publisher bind"); self.dirty |= ui.add_enabled(zmq_free, egui::TextEdit::singleline(&mut self.config.output.zmq_bind)).changed();
                    self.dirty |= ui.add_enabled(udp_free, egui::Checkbox::new(&mut self.config.output.enabled, "UDP · full measurement JSON")).changed();
                    ui.label("Unique sender bind"); self.dirty |= ui.add_enabled(udp_free, egui::TextEdit::singleline(&mut self.config.output.bind)).changed();
                    ui.label("Receiver target"); self.dirty |= ui.add_enabled(udp_free, egui::TextEdit::singleline(&mut self.config.output.target)).changed();
                    ui.label("JSONL recording (empty = off)"); self.dirty |= ui.text_edit_singleline(&mut self.config.output.jsonl_path).changed();
                    ui.small("Restart to apply source, instance or output changes.");
                });
                egui::CollapsingHeader::new("Video / saved state").show(ui, |ui| {
                    self.dirty |= ui.checkbox(&mut self.config.autosave, "Automatically save configuration").changed();
                    ui.small("Marker edits are saved after 300 ms idle and on close. The saved background reloads automatically.");
                    ui.label("Recording directory"); changed |= ui.text_edit_singleline(&mut self.config.recording.directory).changed();
                    egui::ComboBox::from_label("Video format").selected_text(self.config.recording.format.label()).show_ui(ui, |ui| {
                        for format in [VideoFormat::Mjpeg, VideoFormat::Png, VideoFormat::Raw] {
                            changed |= ui.selectable_value(&mut self.config.recording.format, format, format.label()).changed();
                        }
                    });
                    if self.config.recording.format == VideoFormat::Mjpeg {
                        ui.horizontal(|ui| { ui.label("Quality"); changed |= ui.add(egui::Slider::new(&mut self.config.recording.quality, 50..=100)).changed(); });
                    }
                    ui.horizontal(|ui| { ui.label("Queue frames"); changed |= ui.add(egui::DragValue::new(&mut self.config.recording.queue_frames).range(1..=256)).changed(); });
                    ui.horizontal(|ui| { ui.label("Segment MiB"); changed |= ui.add(egui::DragValue::new(&mut self.config.recording.segment_mb).range(16..=1900)).changed(); });
                    ui.small("Grayscale AVI. MJPEG is 10–20× smaller than raw and plays in VLC and in Rekke; PNG and raw are bit-exact. Applies to the next recording. A full queue stops recording visibly.");
                    if let Some(path) = &state.recording.directory { ui.small(path.display().to_string()); }
                });
                egui::CollapsingHeader::new("Sampling").show(ui, |ui| {
                    ui.horizontal(|ui| { ui.label("Angular bins"); changed |= ui.add(egui::DragValue::new(&mut self.config.tracking.angle_bins).range(16..=2048)).changed(); });
                    ui.horizontal(|ui| { ui.label("Radial samples"); changed |= ui.add(egui::DragValue::new(&mut self.config.tracking.radial_samples).range(4..=1024)).changed(); });
                    ui.horizontal(|ui| { ui.label("Preview FPS"); changed |= ui.add(egui::DragValue::new(&mut self.config.preview_fps).range(1..=120)).changed(); });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Restart acquisition").clicked() { self.request_restart(); }
                    if ui.button("Reload config").clicked() {
                        match Config::load(&self.path) { Ok(c) => { self.config = self.overrides.apply(&c); self.saved = c; self.last_autosave = self.config.clone(); self.dirty = false; self.request_restart(); }, Err(e) => self.status = format!("{e:#}") }
                    }
                });
                if let Some(error) = &state.error { ui.colored_label(COLORS[0], error); }
                if !self.status.is_empty() { ui.small(&self.status); }
                ui.small(format!("Config: {}", self.path.display()));
                ui.small(format!("{} frames · {} local UDP drops", state.frames, state.udp_dropped));
            });
        });
        self.show_settings = open;
        if changed {
            self.apply();
        }
    }
    fn camera_view(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let (container, _) = ui.allocate_exact_size(available, egui::Sense::hover());
        ui.painter()
            .rect_filled(container, 0.0, Color32::from_gray(9));
        let (w, h, _) = self.config.source.dimensions();
        let scale = (available.x / w as f32).min(available.y / h as f32);
        let rect = Rect::from_center_size(
            container.center(),
            Vec2::new(w as f32 * scale, h as f32 * scale),
        );
        let Some(texture) = &self.texture else {
            let message = self.engine.as_ref().map(Engine::state).and_then(|s| s.error).unwrap_or_else(|| if self.status.is_empty() { "Opening camera…\n\nAllow camera access if your OS asks.\nSettings → Camera / input to select a device or demo.".into() } else { self.status.clone() });
            ui.put(
                Rect::from_center_size(rect.center(), Vec2::new(rect.width().min(650.0), 140.0)),
                egui::Label::new(message).wrap(),
            );
            return;
        };
        ui.painter().image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        let map = |p: Point| Pos2::new(rect.min.x + p.x * scale, rect.min.y + p.y * scale);
        let painter = ui.painter_at(rect);
        if self.config.sectors().iter().any(|s| s.enabled) {
            painter.line_segment(
                [map(self.config.head), map(self.config.tail)],
                Stroke::new(0.7_f32, Color32::from_white_alpha(120)),
            );
        }
        for (i, s) in self
            .config
            .sectors()
            .into_iter()
            .enumerate()
            .filter(|(_, s)| s.enabled)
        {
            let mut inner = Vec::new();
            let mut outer = Vec::new();
            for j in 0..=96 {
                let a = (s.start_deg + s.span_deg * j as f32 / 96.0).to_radians();
                inner.push(map(s.hinge.at(s.inner_radius, a)));
                outer.push(map(s.hinge.at(s.outer_radius, a)));
            }
            let stroke = Stroke::new(0.9_f32, COLORS[i].gamma_multiply(0.85));
            painter.line_segment([inner[0], outer[0]], stroke);
            painter.line_segment([inner[96], outer[96]], stroke);
            painter.add(egui::Shape::line(inner, stroke));
            painter.add(egui::Shape::line(outer, stroke));
            if let Some(snapshot) = &self.latest {
                let part = [
                    &snapshot.measurement.left,
                    &snapshot.measurement.right,
                    &snapshot.measurement.head,
                    &snapshot.measurement.abdomen,
                ][i];
                for &a in &part.image_angles {
                    let primary = part.image_angle_rad.is_some_and(|v| (v - a).abs() < 1e-5);
                    painter.line_segment(
                        [
                            map(s.hinge.at(s.inner_radius, a)),
                            map(s.hinge.at(s.outer_radius, a)),
                        ],
                        Stroke::new(
                            if primary { 1.1_f32 } else { 0.6_f32 },
                            if primary {
                                COLORS[i]
                            } else {
                                COLORS[i].gamma_multiply(0.4)
                            },
                        ),
                    );
                }
            }
        }
        let points = handles(&self.config);
        let response = ui.interact(
            rect,
            ui.id().with("camera_canvas"),
            egui::Sense::click_and_drag(),
        );
        let hovered = response.hover_pos().and_then(|pointer| {
            points
                .iter()
                .filter(|(_, p, _)| map(*p).distance(pointer) <= 13.0)
                .min_by(|a, b| {
                    map(a.1)
                        .distance(pointer)
                        .total_cmp(&map(b.1).distance(pointer))
                })
                .map(|v| v.0)
        });
        for (handle, p, color) in &points {
            let active = hovered == Some(*handle) || self.drag.map(|d| d.handle) == Some(*handle);
            painter.circle_filled(map(*p), if active { 4.5 } else { 3.0 }, *color);
            if active {
                painter.circle_stroke(map(*p), 7.5, Stroke::new(0.8_f32, *color));
            }
        }
        if let Some(handle) = hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            response.clone().on_hover_text(handle.label());
        }
        // Capture the press itself, before egui's drag threshold. OS events may
        // contain a complete fast drag in one frame, clearing press_origin.
        let press = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        if let Some(pointer) = press
            && rect.contains(pointer)
            && ui.ctx().layer_id_at(pointer) == Some(response.layer_id)
        {
            self.drag = points
                .iter()
                .filter(|(_, p, _)| map(*p).distance(pointer) <= 15.0)
                .min_by(|a, b| {
                    map(a.1)
                        .distance(pointer)
                        .total_cmp(&map(b.1).distance(pointer))
                })
                .map(|v| Drag {
                    handle: v.0,
                    offset: Point {
                        x: v.1.x - (pointer.x - rect.min.x) / scale,
                        y: v.1.y - (pointer.y - rect.min.y) / scale,
                    },
                });
        }
        let (down, released, pointer) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer.latest_pos(),
            )
        });
        if (down || released)
            && let (Some(drag), Some(pointer)) = (self.drag, pointer)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let before = self.config.clone();
            match move_handle(
                &mut self.config,
                drag.handle,
                Point {
                    x: (pointer.x - rect.min.x) / scale + drag.offset.x,
                    y: (pointer.y - rect.min.y) / scale + drag.offset.y,
                },
            ) {
                Ok(()) if self.config != before => {
                    if let Some(wing) = drag.handle.wing() {
                        self.last_wing = wing;
                    }
                    self.status.clear();
                    self.apply();
                }
                Ok(()) => {}
                Err(e) => self.status = format!("{e:#}"),
            }
        }
        if released || !down {
            self.drag = None;
        }
        self.overlay_readouts(&painter, rect);
    }
    fn overlay_readouts(&self, painter: &egui::Painter, rect: Rect) {
        let Some(snapshot) = &self.latest else {
            return;
        };
        let m = &snapshot.measurement;
        let states = [&m.left, &m.right, &m.head, &m.abdomen];
        let mut lines = Vec::new();
        for (i, label) in ["L", "R", "H", "A"].into_iter().enumerate() {
            if self.config.sectors()[i].enabled {
                lines.push((
                    format!(
                        "{label}: {}",
                        states[i]
                            .angle_rad
                            .map(|a| format!("{:+.4} rad", a))
                            .unwrap_or_else(|| "— no edge".into())
                    ),
                    COLORS[i],
                ));
            }
        }
        if self.config.tracking.left.enabled && self.config.tracking.right.enabled {
            lines.push((
                format!(
                    "L−R: {}",
                    m.left_minus_right_rad
                        .map(|a| format!("{:+.4} rad", a))
                        .unwrap_or_else(|| "—".into())
                ),
                Color32::from_gray(215),
            ));
        }
        if lines.is_empty() {
            return;
        }
        let height = lines.len() as f32 * 18.0 + 12.0;
        let panel = Rect::from_min_size(
            Pos2::new(rect.left() + 10.0, rect.bottom() - height - 10.0),
            Vec2::new(173.0, height),
        );
        painter.rect_filled(panel, 5.0, Color32::from_black_alpha(145));
        for (i, (text, color)) in lines.into_iter().enumerate() {
            painter.text(
                panel.min + Vec2::new(8.0, 6.0 + i as f32 * 18.0),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::monospace(12.0),
                color,
            );
        }
    }
    fn source_controls(&mut self, ui: &mut egui::Ui) {
        if self.overrides.source.is_some() {
            ui.small(format!(
                "This run uses a temporary source: {}. Your saved source is unchanged.",
                self.overrides.note.as_deref().unwrap_or("set at launch")
            ));
            ui.horizontal(|ui| {
                if ui.button("Back to saved source").clicked() {
                    self.use_source(None, None);
                }
                if ui
                    .button("Save as this config's source")
                    .on_hover_text("Make this source permanent for this config")
                    .clicked()
                {
                    self.saved.source = self.config.source.clone();
                    self.overrides.source = None;
                    self.overrides.note = None;
                    self.dirty = true;
                    self.last_autosave.source = SourceConfig::Demo {
                        width: 0,
                        height: 0,
                        fps: 0,
                        beat_hz: 0.0,
                    };
                }
            });
            return;
        }
        let (mut w, mut h, mut fps) = self.config.source.dimensions();
        let mut kind = match self.config.source {
            SourceConfig::Demo { .. } => 0,
            SourceConfig::Camera { .. } => 1,
            SourceConfig::Video { .. } => 2,
            SourceConfig::Network { .. } => 3,
        };
        let old_kind = kind;
        egui::ComboBox::from_id_salt("source_kind")
            .selected_text(self.config.source.label())
            .show_ui(ui, |ui| {
                for (i, name) in [
                    "Simulated fly",
                    "USB camera",
                    "Video file",
                    "Network stream (FFmpeg)",
                ]
                .into_iter()
                .enumerate()
                {
                    ui.selectable_value(&mut kind, i, name);
                }
            });
        if old_kind != kind {
            self.config.source = match kind {
                1 => SourceConfig::Camera {
                    device: "auto".into(),
                    width: w,
                    height: h,
                    fps: 30,
                },
                2 => SourceConfig::Video {
                    path: String::new(),
                    width: w,
                    height: h,
                    fps: 30,
                    repeat: true,
                    realtime: true,
                },
                3 => SourceConfig::Network {
                    url: String::new(),
                    width: w,
                    height: h,
                    fps: 30,
                },
                _ => SourceConfig::Demo {
                    width: w,
                    height: h,
                    fps: 60,
                    beat_hz: 0.8,
                },
            };
            self.dirty = true;
            fps = self.config.source.dimensions().2;
        }
        match &mut self.config.source {
            SourceConfig::Camera { device, .. } => {
                ui.label(
                    "Camera: auto, a number from the list below, a device path or part of its name",
                );
                self.dirty |= ui.text_edit_singleline(device).changed();
            }
            SourceConfig::Video {
                path,
                repeat,
                realtime,
                ..
            } => {
                ui.label("Video path (relative to config). Tip: drop a video file on the window.");
                self.dirty |= ui.text_edit_singleline(path).changed();
                self.dirty |= ui.checkbox(repeat, "Loop video").changed();
                self.dirty |= ui
                    .checkbox(realtime, "Play at recorded speed")
                    .on_hover_text(
                        "Off: process every frame as fast as possible (offline analysis)",
                    )
                    .changed();
            }
            SourceConfig::Network { url, .. } => {
                ui.label("Stream URL");
                self.dirty |= ui.text_edit_singleline(url).changed();
            }
            SourceConfig::Demo { beat_hz, .. } => {
                ui.horizontal(|ui| {
                    ui.label("Demo motion Hz");
                    self.dirty |= ui
                        .add(egui::DragValue::new(beat_hz).range(0.1..=200.0))
                        .changed();
                });
            }
        }
        let mut dimensions_changed = false;
        ui.horizontal(|ui| {
            ui.label("W / H");
            dimensions_changed |= ui
                .add(egui::DragValue::new(&mut w).range(32..=8192))
                .changed();
            dimensions_changed |= ui
                .add(egui::DragValue::new(&mut h).range(32..=8192))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Requested fps");
            dimensions_changed |= ui
                .add(egui::DragValue::new(&mut fps).range(1..=10000))
                .changed();
        });
        if dimensions_changed {
            match &mut self.config.source {
                SourceConfig::Demo {
                    width,
                    height,
                    fps: f,
                    ..
                }
                | SourceConfig::Camera {
                    width,
                    height,
                    fps: f,
                    ..
                }
                | SourceConfig::Video {
                    width,
                    height,
                    fps: f,
                    ..
                }
                | SourceConfig::Network {
                    width,
                    height,
                    fps: f,
                    ..
                } => {
                    *width = w;
                    *height = h;
                    *f = fps;
                }
            }
            self.dirty = true;
        }
        if ui
            .add_enabled(
                self.camera_query.is_none(),
                egui::Button::new("List camera devices"),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            self.camera_query = Some(rx);
            std::thread::spawn(move || {
                let result = crate::source::list_cameras().map_err(|e| format!("{e:#}"));
                let _ = tx.send(result);
            });
        }
        let mut chosen = None;
        for c in &self.cameras {
            if ui
                .small_button(c)
                .on_hover_text("Use this camera (then Restart acquisition)")
                .clicked()
            {
                chosen = c.split(':').next().map(str::to_owned);
            }
        }
        if let Some(number) = chosen
            && let SourceConfig::Camera { device, .. } = &mut self.config.source
        {
            *device = number;
            self.dirty = true;
        }
        ui.small("Restart to apply camera changes. Coordinates are image pixels; re-position regions if resolution changes.");
    }
    /// Switch this run to another source without touching the saved config.
    fn use_source(&mut self, source: Option<SourceConfig>, note: Option<String>) {
        self.overrides.source = source;
        self.overrides.note = note;
        self.config.source = self
            .overrides
            .source
            .clone()
            .unwrap_or_else(|| self.saved.source.clone());
        self.request_restart();
    }
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            let (width, height, fps) = self.config.source.dimensions();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.use_source(
                Some(SourceConfig::Video {
                    path: path.to_string_lossy().into_owned(),
                    width,
                    height,
                    fps,
                    repeat: true,
                    realtime: true,
                }),
                Some(format!("Tracking {name} (dropped)")),
            );
        }
    }
    /// Start another window for the next free rig config.
    fn new_window(&mut self) {
        match std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe)
                .stdin(std::process::Stdio::null())
                .spawn()
        }) {
            Ok(_) => self.status = "Opening another window for the next rig…".into(),
            Err(e) => self.status = format!("New window: {e}"),
        }
    }
    fn banner(&mut self, ui: &mut egui::Ui, state: &State) {
        let (text, color) = if let Some(reason) = &state.fallback {
            (
                format!(
                    "No camera — showing a {}. {}",
                    if state.source.starts_with("Sample") {
                        "sample video"
                    } else {
                        "simulated fly"
                    },
                    reason
                ),
                Color32::from_rgb(240, 180, 80),
            )
        } else if let Some(note) = &self.overrides.note {
            (
                format!("{note} · not saved to the config"),
                Color32::from_rgb(120, 200, 230),
            )
        } else {
            return;
        };
        egui::Frame::new()
            .fill(Color32::from_black_alpha(190))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(text).color(color).small()).wrap());
                    if state.fallback.is_some() && ui.small_button("Retry camera").clicked() {
                        self.request_restart();
                    }
                    if self.overrides.source.is_some()
                        && ui.small_button("Back to saved source").clicked()
                    {
                        self.use_source(None, None);
                    }
                });
            });
    }
    fn plots(&self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            columns[0].label(
                egui::RichText::new("Wing angle · last 8 seconds")
                    .size(12.0)
                    .strong(),
            );
            let (rect, _) = columns[0].allocate_exact_size(
                Vec2::new(columns[0].available_width(), 110.0),
                egui::Sense::hover(),
            );
            plot_grid(columns[0].painter(), rect);
            if let Some(&(time, _, _)) = self.history.back() {
                for (wing, &color) in COLORS.iter().take(2).enumerate() {
                    let mut segment = Vec::new();
                    for &(t, l, r) in &self.history {
                        if time - t > 8.0 {
                            continue;
                        }
                        let value = if wing == 0 { l } else { r };
                        if let Some(value) = value {
                            segment.push(Pos2::new(
                                rect.max.x - (time - t) / 8.0 * rect.width(),
                                rect.center().y - value.to_degrees() / 90.0 * rect.height() / 2.0,
                            ));
                        } else if segment.len() > 1 {
                            columns[0].painter().add(egui::Shape::line(
                                std::mem::take(&mut segment),
                                Stroke::new(1.5_f32, color),
                            ));
                        } else {
                            segment.clear();
                        }
                    }
                    if segment.len() > 1 {
                        columns[0]
                            .painter()
                            .add(egui::Shape::line(segment, Stroke::new(1.5_f32, color)));
                    }
                }
            }
            columns[1].label(
                egui::RichText::new(format!(
                    "{} · angular intensity profile",
                    NAMES[self.selected]
                ))
                .size(12.0)
                .strong(),
            );
            let (rect, _) = columns[1].allocate_exact_size(
                Vec2::new(columns[1].available_width(), 110.0),
                egui::Sense::hover(),
            );
            plot_grid(columns[1].painter(), rect);
            if let Some(snapshot) = &self.latest {
                let state = [
                    &snapshot.measurement.left,
                    &snapshot.measurement.right,
                    &snapshot.measurement.head,
                    &snapshot.measurement.abdomen,
                ][self.selected];
                if state.profile.len() > 1 {
                    // Auto-scale: faint wings change the profile by only a few percent.
                    let lo = state.profile.iter().copied().fold(f32::INFINITY, f32::min);
                    let hi = state
                        .profile
                        .iter()
                        .copied()
                        .fold(f32::NEG_INFINITY, f32::max);
                    let range = (hi - lo).max(1e-4);
                    let n = state.profile.len() - 1;
                    let points = state
                        .profile
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            Pos2::new(
                                rect.min.x + i as f32 / n as f32 * rect.width(),
                                rect.max.y - 4.0 - (p - lo) / range * (rect.height() - 8.0),
                            )
                        })
                        .collect();
                    columns[1].painter().add(egui::Shape::line(
                        points,
                        Stroke::new(1.5_f32, COLORS[self.selected]),
                    ));
                    // Mark detected edges; the primary edge is brighter.
                    let s = self.config.sectors()[self.selected];
                    for &a in &state.image_angles {
                        let offset = crate::tracker::wrap_angle(a - s.start_deg.to_radians())
                            .rem_euclid(std::f32::consts::TAU);
                        let f = offset / s.span_deg.to_radians();
                        if (0.0..=1.0).contains(&f) {
                            let x = rect.min.x + f * rect.width();
                            let primary =
                                state.image_angle_rad.is_some_and(|p| (p - a).abs() < 1e-5);
                            columns[1].painter().line_segment(
                                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                                Stroke::new(
                                    1.0_f32,
                                    Color32::from_white_alpha(if primary { 200 } else { 70 }),
                                ),
                            );
                        }
                    }
                }
                columns[1].small(format!(
                    "Peak {:.5} · ROI coverage {:.0}%",
                    state.peak_gradient,
                    state.coverage * 100.0
                ));
            }
        });
    }
}
fn plot_grid(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, 3.0, Color32::from_rgb(13, 18, 26));
    for j in 1..4 {
        let y = rect.min.y + rect.height() * j as f32 / 4.0;
        p.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(0.5_f32, Color32::from_gray(48)),
        );
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if self.config.autosave
            && self.config != self.last_autosave
            && let Some(saver) = &self.autosaver
        {
            saver.submit(self.persistable());
        }
        // Finish accepted video frames and finalize AVI indexes before exit.
        if let Some(engine) = self.engine.take() {
            engine.wait();
        }
        self.autosaver.take();
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.shutdown.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(saver) = &self.autosaver {
            for result in saver.results.try_iter() {
                match result {
                    Ok(saved) => {
                        if saved == self.persistable() {
                            self.dirty = false;
                            if self.status == "Saving configuration…" {
                                self.status = "Configuration saved".into();
                            }
                        }
                    }
                    Err(error) => self.status = error,
                }
            }
        }
        if let Some(query) = &self.camera_query
            && let Ok(result) = query.try_recv()
        {
            match result {
                Ok(cameras) => self.cameras = cameras,
                Err(e) => self.status = e,
            }
            self.camera_query = None;
        }
        if self.restart_pending && self.engine.as_ref().is_none_or(|e| e.is_finished()) {
            self.engine.take();
            self.restart_pending = false;
            self.start();
        }
        let state = self.engine.as_ref().map(Engine::state).unwrap_or_default();
        self.observe(ctx, &state);
        self.toolbar(ctx, &state);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let running = self.engine.as_ref().is_some_and(|e| !e.is_finished());
                let live = running && state.latest.is_some();
                ui.colored_label(
                    if live { COLORS[1] } else { Color32::GRAY },
                    if live {
                        "Live"
                    } else if running {
                        "Opening"
                    } else if state.message == "End of video" {
                        "End of video · press Start to replay"
                    } else {
                        "Stopped"
                    },
                );
                ui.small(&self.config.instance);
                if state.recording.active {
                    ui.colored_label(
                        COLORS[0],
                        format!(
                            "REC {} · {:.0} MB",
                            state.recording.written,
                            state.recording.bytes as f64 / 1e6
                        ),
                    );
                }
                if let Some(error) = &state.recording.error {
                    ui.colored_label(COLORS[0], "REC error")
                        .on_hover_text(error);
                }
                ui.separator();
                if let Some(s) = &self.latest {
                    ui.small(format!(
                        "{:.1} fps · {:.0} µs",
                        s.measurement.capture_fps, s.measurement.processing_us
                    ));
                }
                if let Some(error) = &state.output_error {
                    ui.colored_label(COLORS[0], "Outputs off")
                        .on_hover_text(error);
                }
                if let Some(error) = &state.error {
                    ui.colored_label(COLORS[0], "Camera error")
                        .on_hover_text(error);
                } else if !["Live", "Acquisition stopped", "End of video"]
                    .contains(&state.message.as_str())
                {
                    ui.small(&state.message);
                }
                if !self.status.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&self.status).small()).truncate());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.config.output.zmq_enabled {
                        format!("ZMQ {}", self.config.output.zmq_bind)
                    } else if !self.config.output.enabled {
                        "Output off".into()
                    } else if live {
                        format!("UDP {}", self.config.output.target)
                    } else {
                        "UDP waiting".into()
                    };
                    ui.small(label).on_hover_text(format!(
                        "UDP sender {} · {} UDP drops · {} ZMQ queue drops",
                        self.config.output.bind, state.udp_dropped, state.zmq_dropped
                    ));
                });
            });
        });
        if self.show_diagnostics {
            egui::TopBottomPanel::bottom("diagnostics").show(ctx, |ui| {
                self.plots(ui);
            });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let full = ui.max_rect();
                self.camera_view(ui);
                let mut top = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_size(full.min, Vec2::new(full.width(), 40.0))),
                );
                self.banner(&mut top, &state);
            });
        self.settings(ctx, &state);
        if self.config != self.last_autosave {
            if self.config.autosave
                && let Some(saver) = &self.autosaver
            {
                saver.submit(self.persistable());
            }
            self.last_autosave = self.config.clone();
        }
        let title = window_title(&self.config);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        self.dropped_files(ctx);
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed() > Duration::from_secs(2)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_requested = true;
        }
        if let Some(path) = &self.screenshot {
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(
                    path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => eprintln!("Screenshot: {e}"),
                }
            }
        }
        ctx.request_repaint_after(Duration::from_secs_f64(
            1.0 / self.config.preview_fps as f64,
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_wings_expose_all_handles_without_selection() {
        let mut config = Config::default();
        let h = handles(&config);
        assert_eq!(h.len(), 13); // Head/tail/center plus five handles per wing.
        for i in 0..2 {
            for handle in [
                Handle::Hinge(i),
                Handle::Start(i),
                Handle::End(i),
                Handle::Inner(i),
                Handle::Outer(i),
            ] {
                assert!(h.iter().any(|p| p.0 == handle));
            }
        }
        config.tracking.right.enabled = false;
        assert_eq!(handles(&config).len(), 8);
        assert!(handles(&config).iter().all(|(h, _, _)| !matches!(
            h,
            Handle::Hinge(1)
                | Handle::Start(1)
                | Handle::End(1)
                | Handle::Inner(1)
                | Handle::Outer(1)
        )));
        config.tracking.left.enabled = false;
        assert!(handles(&config).is_empty());
    }
    #[test]
    fn dragging_angles_across_zero_preserves_unwrapped_sector() {
        let mut c = Config::default();
        c.tracking.right.start_deg = 300.0;
        c.tracking.right.span_deg = 120.0;
        let p = c.tracking.right.hinge.at(150.0, 65.0_f32.to_radians());
        move_handle(&mut c, Handle::End(1), p).unwrap();
        assert!((c.tracking.right.span_deg - 125.0).abs() < 0.001);
        let p = c.tracking.right.hinge.at(150.0, 310.0_f32.to_radians());
        move_handle(&mut c, Handle::Start(1), p).unwrap();
        assert!((c.tracking.right.span_deg - 115.0).abs() < 0.001);
        c.validate().unwrap();
    }
    #[test]
    fn a_complete_fast_drag_in_one_frame_edits_the_unselected_wing() {
        for corner in [false, true] {
            let ctx = egui::Context::default();
            let mut app = App {
                autosaver: None,
                last_autosave: Config::default(),
                shutdown: Arc::new(AtomicBool::new(false)),
                config: Config::default(),
                saved: Config::default(),
                overrides: Overrides::default(),
                options: StartOptions::default(),
                title: String::new(),
                path: PathBuf::from("unused.toml"),
                engine: None,
                selected: 0,
                texture: Some(ctx.load_texture(
                    "test",
                    egui::ColorImage::new([640, 480], Color32::GRAY),
                    egui::TextureOptions::LINEAR,
                )),
                latest: None,
                last_sequence: None,
                last_display: None,
                history: VecDeque::new(),
                drag: None,
                last_wing: 0,
                show_difference: false,
                show_settings: false,
                show_diagnostics: false,
                dirty: false,
                status: String::new(),
                restart_pending: false,
                cameras: Vec::new(),
                camera_query: None,
                screenshot: None,
                screenshot_requested: false,
                started: Instant::now(),
            };
            let raw = |events| egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
                events,
                ..Default::default()
            };
            let draw = |ctx: &egui::Context, app: &mut App| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| app.camera_view(ui));
            };
            let _ = ctx.run(raw(vec![]), |ctx| draw(ctx, &mut app));
            // Settings still selects Left; directly drag Right's arc or corner.
            let sector = &app.config.tracking.right;
            let (start_deg, span_deg) = (sector.start_deg, sector.span_deg);
            let angle =
                (sector.start_deg + if corner { 0.0 } else { sector.span_deg * 0.5 }).to_radians();
            let start = sector.hinge.at(sector.outer_radius, angle);
            let end_angle = angle - if corner { 10.0_f32.to_radians() } else { 0.0 };
            let end = sector.hinge.at(160.0, end_angle);
            let start = Pos2::new(start.x, start.y);
            let end = Pos2::new(end.x, end.y);
            let events = vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ];
            let _ = ctx.run(raw(events), |ctx| draw(ctx, &mut app));
            assert!((app.config.tracking.right.outer_radius - 160.0).abs() < 0.01);
            assert!((app.config.tracking.left.outer_radius - 160.0).abs() < 0.01);
            if corner {
                let right = &app.config.tracking.right;
                assert!((right.start_deg - (start_deg - 10.0)).abs() < 0.01);
                assert!((right.span_deg - (span_deg + 10.0)).abs() < 0.01);
                assert!((app.config.tracking.left.span_deg - (span_deg + 10.0)).abs() < 0.01);
            }
            assert!(app.dirty);
            assert!(app.drag.is_none());
        }
    }
}
