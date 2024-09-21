# Paint Jr.

Janky picture editor for when you want to draw lines on things and then download them. Built using [eframe](https://github.com/emilk/eframe_template) and `egui`.

## Known Issues
- Line rendering:
    - `epaint` is used to render on the canvas, but `tiny_skia` is used to download the image. The line widths are different.
    - I am approximating them by multiplying the width f32 with a constant for now...