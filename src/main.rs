use macroquad::prelude::*;

const EFFECTS_VERTEX_SHADER: &'static str = include_str!("effects.vert");
const EFFECTS_FRAGMENT_SHADER: &'static str = include_str!("effects.frag");

struct Effects {
    brightness : f32,
    contrast : f32,
    invert : u32,
    highlight : f32,
    shadow : f32,
    white_pt : f32,
    black_pt : f32,
    temperature : f32,
}

impl Default for Effects {
    fn default() -> Effects {
        Effects {
            brightness: 0.,
            contrast: 0.5,
            invert : 0,
            highlight : 0.5,
            shadow : 0.5,
            white_pt : 1.0,
            black_pt : 0.0,
            temperature : 6500.,
        }
    }
}

#[macroquad::main("photos")]
async fn main() {
    let texture: Texture2D = load_texture("test.png").await.unwrap();
    texture.set_filter(FilterMode::Nearest);

    let effects_material = load_material(
        EFFECTS_VERTEX_SHADER,
        EFFECTS_FRAGMENT_SHADER,
        MaterialParams {
            uniforms: vec![
                ("brightness".to_string(), UniformType::Float1),
                ("contrast".to_string(), UniformType::Float1),
                ("invert".to_string(), UniformType::Int1),
                ("highlight".to_string(), UniformType::Float1),
                ("shadow".to_string(), UniformType::Float1),
                ("white_pt".to_string(), UniformType::Float1),
                ("black_pt".to_string(), UniformType::Float1),
                ("temperature".to_string(), UniformType::Float1),
            ],
            ..Default::default()
        },
    ).unwrap_or_else(|err| {
        println!("{}", err);
        std::process::exit(1);
    });

    let mut effects = Effects::default();

    loop {
        clear_background(GRAY);

        gl_use_material(effects_material);

        effects_material
            .set_uniform("brightness", effects.brightness);
        effects_material
            .set_uniform("contrast", effects.contrast);
        effects_material
            .set_uniform("invert", effects.invert);
        effects_material
            .set_uniform("highlight", effects.highlight);
        effects_material
            .set_uniform("shadow", effects.shadow);
        effects_material
            .set_uniform("white_pt", effects.white_pt);
        effects_material
            .set_uniform("black_pt", effects.black_pt);
        effects_material
            .set_uniform("temperature", effects.temperature);

        draw_texture_ex(
            texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(480.0, 720.0)),
                ..Default::default()
            },
        );
        gl_use_default_material();

        egui_macroquad::ui(|ctx| {
            egui::Window::new("window")
                .show(ctx, |ui| {
                    ui.label("brightness");
                    ui.add(egui::Slider::new(&mut effects.brightness, -0.5..=0.5));

                    ui.label("contrast");
                    ui.add(egui::Slider::new(&mut effects.contrast, 0.0..=1.0));

                    let mut invert = effects.invert > 0;
                    ui.checkbox(&mut invert, "invert");
                    effects.invert = if invert { 1 } else { 0 };

                    ui.separator();

                    ui.label("highlight");
                    ui.add(egui::Slider::new(&mut effects.highlight, 0.0..=1.0));

                    ui.label("shadow");
                    ui.add(egui::Slider::new(&mut effects.shadow, 0.0..=1.0));

                    ui.label("white point");
                    ui.add(egui::Slider::new(&mut effects.white_pt, 0.0..=1.0));

                    ui.label("black point");
                    ui.add(egui::Slider::new(&mut effects.black_pt, 0.0..=1.0));

                    ui.separator();

                    ui.label("temperature");
                    ui.add(egui::Slider::new(&mut effects.temperature, 4000.0..=9000.0));
                });
        });

        egui_macroquad::draw();

        next_frame().await;
    }
}

