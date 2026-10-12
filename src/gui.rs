use crate::{
    config::{Config, Method, Point, PrimaryEdge, SourceConfig},
    engine::{Engine, Snapshot, State},
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
    config: Config,
    path: PathBuf,
    engine: Option<Engine>,
    limit: Option<u64>,
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
pub fn launch(
    config: Config,
    path: PathBuf,
    limit: Option<u64>,
    screenshot: Option<PathBuf>,
    shutdown: Arc<AtomicBool>,
) -> eframe::Result<()> {
    let title = format!("Rekke · {}", config.instance);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 820.0])
            .with_min_inner_size([720.0, 550.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        &title,
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
                path,
                engine: None,
                limit,
                selected: 0,
                texture: None,
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
        match Engine::start(self.config.clone(), self.path.clone(), self.limit) {
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
    fn save(&mut self) {
        if let Some(saver) = &self.autosaver {
            match self.config.validate() {
                Ok(()) => {
                    saver.save_now(self.config.clone());
                    self.status = "Saving configuration…".into();
                }
                Err(error) => self.status = format!("{error:#}"),
            }
            return;
        }
        match self.config.save(&self.path) {
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
                    if ui.add_enabled(self.engine.as_ref().is_some_and(|e| !e.is_finished()), egui::Button::new(if state.recording.active { "Stop REC" } else { "Record" })).on_hover_text("Lossless grayscale video; no overlay, dedicated disk writer").clicked() {
                        let record = !state.recording.active;
                        if let Some(e) = &self.engine { match e.record(record) { Ok(()) => { self.config.recording.enabled = record; changed = true; }, Err(error) => self.status = format!("{error:#}") } }
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
                if ui.checkbox(&mut reject, "Reject spikes").on_hover_text("Both wings: ignore thin bright pulses and track a broad faint shoulder. Tune width in Settings.").changed() {
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
                egui::ComboBox::from_label("Detector").selected_text(match s.method { Method::Edge => "Strongest edge", Method::WingBlur => "Wing blur · experimental", Method::Centroid => "Centroid" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.method, Method::Edge, "Strongest edge");
                    ui.selectable_value(&mut s.method, Method::WingBlur, "Wing blur · experimental");
                    ui.selectable_value(&mut s.method, Method::Centroid, "Centroid");
                });
                if s.method == Method::WingBlur {
                    ui.horizontal(|ui| { ui.label("Reject pulses under"); ui.add(egui::DragValue::new(&mut s.spike_width_deg).range(0.0..=45.0).speed(0.1).suffix("°")); });
                    ui.small("Remove narrow bright pulses, then find the median edge across radial bands. Keep width smaller than the wing shoulder; broad obstructions can still win.");
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
                    ui.label("Instance"); self.dirty |= ui.text_edit_singleline(&mut self.config.instance).changed();
                    self.dirty |= ui.checkbox(&mut self.config.output.zmq_enabled, "ZMQ · Kinefly bridge format").changed();
                    ui.label("ZMQ publisher bind"); self.dirty |= ui.text_edit_singleline(&mut self.config.output.zmq_bind).changed();
                    self.dirty |= ui.checkbox(&mut self.config.output.enabled, "UDP · full measurement JSON").changed();
                    ui.label("Unique sender bind"); self.dirty |= ui.text_edit_singleline(&mut self.config.output.bind).changed();
                    ui.label("Receiver target"); self.dirty |= ui.text_edit_singleline(&mut self.config.output.target).changed();
                    ui.label("JSONL recording (empty = off)"); self.dirty |= ui.text_edit_singleline(&mut self.config.output.jsonl_path).changed();
                    ui.small("Restart to apply source, instance or output changes.");
                });
                egui::CollapsingHeader::new("Video / saved state").show(ui, |ui| {
                    self.dirty |= ui.checkbox(&mut self.config.autosave, "Automatically save configuration").changed();
                    ui.small("Marker edits are saved after 300 ms idle and on close. The saved background reloads automatically.");
                    ui.label("Recording directory"); changed |= ui.text_edit_singleline(&mut self.config.recording.directory).changed();
                    ui.horizontal(|ui| { ui.label("Queue frames"); changed |= ui.add(egui::DragValue::new(&mut self.config.recording.queue_frames).range(1..=256)).changed(); });
                    ui.horizontal(|ui| { ui.label("Segment MiB"); changed |= ui.add(egui::DragValue::new(&mut self.config.recording.segment_mb).range(16..=1900)).changed(); });
                    ui.small("Lossless grayscale AVI. Settings apply to the next recording session. Fast storage required; queue overflow stops recording visibly.");
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
                        match Config::load(&self.path) { Ok(c) => { self.config = c; self.dirty = false; self.request_restart(); }, Err(e) => self.status = format!("{e:#}") }
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
                    "Simulated camera",
                    "USB camera",
                    "Video replay (FFmpeg)",
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
                    repeat: false,
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
                    fps: 120,
                    beat_hz: 8.0,
                },
            };
            self.dirty = true;
            fps = self.config.source.dimensions().2;
        }
        match &mut self.config.source {
            SourceConfig::Camera { device, .. } => {
                ui.label("Camera: auto, index or device path");
                self.dirty |= ui.text_edit_singleline(device).changed();
            }
            SourceConfig::Video { path, repeat, .. } => {
                ui.label("Video path (relative to config)");
                self.dirty |= ui.text_edit_singleline(path).changed();
                self.dirty |= ui.checkbox(repeat, "Loop video").changed();
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
        for c in &self.cameras {
            ui.small(c);
        }
        ui.small("Restart to apply camera changes. Coordinates are image pixels; re-position regions if resolution changes.");
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
                    "{} · radial intensity profile",
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
                    let points = state
                        .profile
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            Pos2::new(
                                rect.min.x
                                    + i as f32 / (state.profile.len() - 1) as f32 * rect.width(),
                                rect.max.y - p * rect.height(),
                            )
                        })
                        .collect();
                    columns[1].painter().add(egui::Shape::line(
                        points,
                        Stroke::new(1.5_f32, COLORS[self.selected]),
                    ));
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
            saver.submit(self.config.clone());
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
                        if saved == self.config {
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
                    } else {
                        "Stopped"
                    },
                );
                ui.small(&self.config.instance);
                if state.recording.active {
                    ui.colored_label(COLORS[0], format!("REC {}", state.recording.written));
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
                if let Some(error) = &state.error {
                    ui.colored_label(COLORS[0], "Camera error")
                        .on_hover_text(error);
                } else if state.message != "Live" && state.message != "Acquisition stopped" {
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
                self.camera_view(ui);
            });
        self.settings(ctx, &state);
        if self.config != self.last_autosave {
            if self.config.autosave
                && let Some(saver) = &self.autosaver
            {
                saver.submit(self.config.clone());
            }
            self.last_autosave = self.config.clone();
        }
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
                path: PathBuf::from("unused.toml"),
                engine: None,
                limit: None,
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
                assert!((app.config.tracking.right.start_deg + 35.0).abs() < 0.01);
                assert!((app.config.tracking.right.span_deg - 100.0).abs() < 0.01);
                assert!((app.config.tracking.left.span_deg - 100.0).abs() < 0.01);
            }
            assert!(app.dirty);
            assert!(app.drag.is_none());
        }
    }
}
