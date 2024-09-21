use egui::{
    emath::RectTransform, ColorImage, Context, Event, Image, ImageData, Pos2, Rect, Sense, Stroke,
    TextureHandle, TextureOptions, Ui, Vec2, Widget,
};
use image::{DynamicImage, EncodableLayout};
use std::future::Future;
use std::io::Cursor;
use std::sync::mpsc::{channel, Receiver, Sender};
use tiny_skia::{IntSize, Paint, PathBuilder, Pixmap, Transform};

// line width looks much thicker with skia as opposed to the painter, going to manually correct it until I figure out what I actually need to do.
const STROKE_RATIO: f32 = 0.7;

#[derive(serde::Serialize, serde::Deserialize, Eq, PartialEq, Clone, Copy)]
pub enum Mode {
    Select,
    Brush,
    Eraser,
}

pub const EDIT_MODES: &[Mode] = &[Mode::Select, Mode::Brush];

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
    // bytes of image format, not raw rgba
    img: Option<DynamicImage>,
    tex: Option<TextureHandle>,
    byte_channel: (Sender<Vec<u8>>, Receiver<Vec<u8>>),
    file_id: i32,
    changed: bool,
    rect: Option<Rect>,
    filename: String,
}

impl Default for Painter {
    fn default() -> Self {
        Self {
            lines: Default::default(),
            redo: Default::default(),
            active: false,
            img: None,
            tex: None,
            byte_channel: channel(),
            file_id: 0,
            changed: false,
            rect: None,
            filename: "image".to_string(),
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
        ui.horizontal(|ui| {
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

            if ui.button("(png) Save to: ").clicked() {
                if let Some(shot) = &self.img {
                    let task = rfd::AsyncFileDialog::new()
                        .set_file_name(self.filename.clone() + ".png")
                        .save_file();

                    let rgba = shot.to_rgba8();

                    let mut pixmap = Pixmap::from_vec(
                        rgba.as_bytes().to_vec(),
                        IntSize::from_wh(rgba.width(), rgba.height()).unwrap(),
                    )
                    .unwrap();

                    let image_rect = Rect::from_min_max(
                        Pos2::default(),
                        Pos2::new(pixmap.width() as f32, pixmap.height() as f32),
                    );

                    let transform =
                        RectTransform::from_to(Rect::from_min_size(Pos2::ZERO, self.rect.unwrap().square_proportions()), image_rect);

                    for line in &self.lines {
                        if let Some(p) = line.0.first() {
                            let mut pb = PathBuilder::new();

                            let p = transform * *p;
                            pb.move_to(p.x, p.y);

                            line.0.iter().for_each(|p| {
                                let p = transform * *p;
                                pb.line_to(p.x, p.y);
                            });

                            if let Some(path) = pb.finish() {
                                let color = line.1.color;
                                let mut paint = Paint::default();

                                paint.set_color_rgba8(color.r(), color.g(), color.b(), color.a());

                                let stroke = tiny_skia::Stroke {
                                    width: STROKE_RATIO * line.1.width,
                                    ..Default::default()
                                };

                                pixmap.stroke_path(
                                    &path,
                                    &paint,
                                    &stroke,
                                    Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }

                    let shot = image::RgbaImage::from_vec(
                        pixmap.width(),
                        pixmap.height(),
                        pixmap.data().to_vec(),
                    )
                    .unwrap();

                    let mut cursor = Cursor::new(Vec::new());
                    let _ = shot.write_to(&mut cursor, image::ImageFormat::Png);

                    execute(async move {
                        // retrieve inner vec after writing to it
                        let buf = cursor.into_inner();

                        let file = task.await;
                        if let Some(file) = file {
                            let _ = file.write(buf.as_bytes()).await;
                        }
                    });
                }
            }

            ui.text_edit_singleline(&mut self.filename);
        });
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

    pub fn ui_content(&mut self, ui: &mut Ui, ctx: &Context) {
        if let Ok(bytes) = self.byte_channel.1.try_recv() {
            let rgba = image::load_from_memory(bytes.clone().as_bytes()).unwrap();

            self.img = Some(rgba);

            let rgba = self.img.as_ref().unwrap();

            self.file_id += 1;
            self.lines = Default::default();

            let px = rgba.to_rgba8();

            let img = ImageData::from(ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                px.as_bytes(),
            ));

            self.tex = Some(ctx.load_texture(
                format!("image{}", self.file_id),
                img,
                TextureOptions::LINEAR,
            ));
        }

        let mut response;

        if let Some(tex) = &self.tex {
            response = Image::from_texture(tex)
                .maintain_aspect_ratio(true)
                .fit_to_exact_size(ui.available_size())
                .sense(Sense::click_and_drag())
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

        let to_screen = RectTransform::from_to(
            Rect::from_min_size(Pos2::ZERO, response.rect.square_proportions()),
            response.rect,
        );

        let from_screen = to_screen.inverse();

        self.rect = Some(rect);

        if self.lines.is_empty() {
            self.lines.push(Line::default());
        }

        let cur_line = self.lines.last_mut().unwrap();

        if self.active {
            if let Some(pos) = response.interact_pointer_pos() {
                let canvas_pos = from_screen * pos;

                if cur_line.0.last() != Some(&canvas_pos) {
                    // if cur_line.0.last().is_none() {
                    //     // hack for clicks
                    //     cur_line.0.push(canvas_pos + Vec2::new(0.0, 0.001 * cur_line.1.width));
                    // }

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
            .filter(|line| line.0.len() >= 1)
            .map(|line| to_shape(line, to_screen));

        painter.extend(shapes);
    }
}

fn to_shape(line: &Line, to_screen: RectTransform) -> egui::Shape {
    let points: Vec<Pos2> = line.0.iter().map(|p| to_screen * *p).collect();
    egui::Shape::line(points, line.1)
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
