use egui::{Color32, Stroke};

use crate::edit;

const MAX_BRUSH: f32 = 40.0;
const MIN_BRUSH: f32 = 1.0;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateApp {
    // Example stuff:
    label: String,
    value: f32,
    state: edit::Mode,
    brush_width: f32,
    color: Color32,

    #[serde(skip)]
    painter: edit::Painter,
}

impl<'a> Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            state: edit::Mode::Select,
            brush_width: 8.0,
            color: Color32::from_rgb(12, 50, 200),
            painter: edit::Painter::default(),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

impl<'a> eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Selected: {}", self.state));

                if self.state != edit::Mode::Select {
                    ui.label(format!("Width: {:.2}", self.brush_width));
                }

                ui.add_sized(ui.available_size(), |ui: &mut egui::Ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        ui.horizontal(|ui| {
                            egui::warn_if_debug_build(ui);
                        });
                    })
                    .response
                });
            });
        });

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.painter.ui_files(ui);
            });

            ui.separator();

            ui.horizontal(|ui| {
                self.painter.ui_undo(ui);
                self.painter.ui_redo(ui);
            });

            ui.separator();

            ui.horizontal(|ui| {
                edit::EDIT_MODES.iter().for_each(|mode| {
                    if mode == &edit::Mode::Brush && self.state == edit::Mode::Brush {
                        ui.color_edit_button_srgba(&mut self.color);

                        return;
                    }

                    let button = ui.button(mode.to_string());
                    if button.clicked() {
                        self.state = *mode;
                    }

                    if &self.state == mode {
                        button.highlight();
                    }
                });
            });

            ui.horizontal(|ui| {
                if ui.button("-").clicked() {
                    self.brush_width = f32::max(MIN_BRUSH, self.brush_width - 1.0);
                }

                if ui.button("+").clicked() {
                    self.brush_width = f32::min(MAX_BRUSH, self.brush_width + 1.0);
                }

                ui.add(egui::Slider::new(&mut self.brush_width, MIN_BRUSH..=MAX_BRUSH).text("width"));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.painter.set_active(self.state == edit::Mode::Brush);
            self.painter.set_stroke(Stroke::new(
                self.brush_width,
                self.color,
            ));

            self.painter.ui_content(ui, ctx);
        });
    }
}
