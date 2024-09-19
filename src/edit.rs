use egui::{emath, Image, Pos2, Rect, Sense, Stroke, Ui, Widget};
use std::future::Future;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(serde::Serialize, serde::Deserialize, Eq, PartialEq, Clone, Copy)]
pub enum Mode {
    Select,
    Brush,
    Eraser,
}

pub const EDIT_MODES: &[Mode] = &[Mode::Select, Mode::Brush, Mode::Eraser];

// TODO: use symbols
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Select => write!(f, "select"),
            Mode::Brush => write!(f, "brush"),
            Mode::Eraser => write!(f, "eraser"),
        }
    }
}

struct Line(Vec<Pos2>, Stroke);

impl Default for Line {
    fn default() -> Self {
        Self(vec![], Stroke::default())
    }
}

pub struct Painter {
    lines: Vec<Line>,
    redo: Vec<Line>,
    active: bool,
    img: Option<Vec<u8>>,
    byte_channel: (Sender<Vec<u8>>, Receiver<Vec<u8>>),
    file_id: i32,
    changed: bool,
}

impl Default for Painter {
    fn default() -> Self {
        Self {
            lines: Default::default(),
            redo: Default::default(),
            active: false,
            img: None,
            byte_channel: channel(),
            file_id: 0,
            changed: false,
        }
    }
}

/// Painter represents the painted layer on top of an image. Largely taken from https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/painting.rs.
impl Painter {
    pub fn set_stroke(&mut self, stroke: Stroke) {
        self.lines.last_mut().map(|line| line.1 = stroke);
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn ui_files(&mut self, ui: &mut Ui) {
        // a simple button opening the dialog
        if ui.button("Open Image").clicked() {
            let sender = self.byte_channel.0.clone();
            let task = rfd::AsyncFileDialog::new().pick_file();
            // Context is wrapped in an Arc so it's cheap to clone as per:
            // > Context is cheap to clone, and any clones refers to the same mutable data (Context uses refcounting internally).
            // Taken from https://docs.rs/egui/0.24.1/egui/struct.Context.html
            let ctx = ui.ctx().clone();
            execute(async move {
                let file = task.await;
                if let Some(file) = file {
                    let bytes = file.read().await;
                    let _ = sender.send(bytes);
                    ctx.request_repaint();
                }
            });
        }
    }

    pub fn ui_undo(&mut self, ui: &mut Ui) {
        if ui.button("Undo").clicked() {
            if self.lines.len() >= 2 {
                let pop_idx = self.lines.len() - 2;
                self.redo.push(self.lines.remove(pop_idx));
                self.changed = true;
            }
        }
    }

    pub fn ui_redo(&mut self, ui: &mut Ui) {
        if ui.button("Redo").clicked() {
            self.redo.pop().map(|l| {
                let push_idx = self.lines.len() - 1;
                self.lines.insert(push_idx, l);
                self.changed = true;
            });
        }
    }

    pub fn ui_content(&mut self, ui: &mut Ui) {
        if let Ok(bytes) = self.byte_channel.1.try_recv() {
            self.img = Some(bytes);
            self.file_id += 1;
            self.lines = Default::default();
        }

        let mut response;

        if let Some(img) = &self.img {
            response = Image::from_bytes(format!("{}", self.file_id), img.clone())
                .maintain_aspect_ratio(true)
                .fit_to_exact_size(ui.available_size())
                .sense(Sense::drag())
                .ui(ui);
        } else {
            return;
        }

        let rect = ui.clip_rect().intersect(response.rect);
        let painter = ui.painter_at(rect);

        if self.changed {
            response.mark_changed();
            self.changed = false;
        }

        let to_screen = emath::RectTransform::from_to(
            Rect::from_min_size(Pos2::ZERO, response.rect.square_proportions()),
            response.rect,
        );

        let from_screen = to_screen.inverse();

        if self.lines.is_empty() {
            self.lines.push(Line::default());
        }

        let cur_line = self.lines.last_mut().unwrap();

        if self.active {
            if let Some(pos) = response.interact_pointer_pos() {
                let canvas_pos = from_screen * pos;

                if cur_line.0.last() != Some(&canvas_pos) {
                    cur_line.0.push(canvas_pos);
                    response.mark_changed();
                }
            } else if !cur_line.0.is_empty() {
                self.lines.push(Line::default());
                self.redo = Vec::new();
                response.mark_changed();
            }
        }

        let shapes = self
            .lines
            .iter()
            .filter(|line| line.0.len() >= 2)
            .map(|line| {
                let points: Vec<Pos2> = line.0.iter().map(|p| to_screen * *p).collect();
                egui::Shape::line(points, line.1)
            });

        painter.extend(shapes);
    }
}

// `execute` (and buttons) taken from` https://github.com/woelper/egui_pick_file/blob/main/src/app.rs

#[cfg(not(target_arch = "wasm32"))]
fn execute<F: Future<Output = ()> + Send + 'static>(f: F) {
    // this is stupid... use any executor of your choice instead
    std::thread::spawn(move || futures::executor::block_on(f));
}

#[cfg(target_arch = "wasm32")]
fn execute<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}
