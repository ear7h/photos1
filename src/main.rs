use macroquad::prelude::*;

struct Effects {
    brightness : f32,
    contrast : f32,
    invert : u32,
    highlight : f32,
    shadow : f32,
    white_pt : f32,
    black_pt : f32,
}

impl Default for Effects {
    fn default() -> Effects {
        Effects {
            brightness: 0.,
            contrast: 0.5,
            invert : 0,
            highlight : 0.5,
            shadow : 0.5,
            white_pt : 0.5,
            black_pt : 0.5,
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
            ],
            ..Default::default()
        },
    ).unwrap();

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
                    ui.add(egui::Slider::new(&mut effects.brightness, -1.0..=1.0));

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
                });
        });

        egui_macroquad::draw();

        next_frame().await;
    }
}

const EFFECTS_VERTEX_SHADER: &'static str = r#"
#version 100
attribute vec3 position;
attribute vec2 texcoord;
// attribute vec4 color0;

varying lowp vec2 uv;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    uv = texcoord;
}
"#;

const EFFECTS_FRAGMENT_SHADER: &'static str = r#"
#version 100
varying lowp vec2 uv;

uniform sampler2D Texture;

uniform lowp float brightness;
uniform lowp float contrast;
uniform int invert;

uniform lowp float highlight;
uniform lowp float shadow;
uniform lowp float black_pt;
uniform lowp float white_pt;

// f : [0, 1] -> [0, inf)
lowp float unit2nnreal(lowp float x) {
    return x < .5 ?
        2. * x :
        1. / (2.0001 - 2. * x);
}

lowp float luminance(lowp vec4 color) {
    return 0.2126 * color.r + 0.7162 * color.g + 0.0722 * color.b;
}

void main() {
    lowp vec4 color = texture2D(Texture, uv);

    if (invert > 0) {
        color = 1. - color;
    }

    // contrast and brightness
    color = clamp(unit2nnreal(contrast) * (color - .5) + .5 + brightness, 0., 1.);

    lowp float lum0 = luminance(color); // current luminance
    lowp float lum = 2. * lum0 - 1.; // desired luminance
    if (lum0 > .5) {
        // highlight
        lum = pow(lum * unit2nnreal(white_pt), unit2nnreal(1. - highlight)) / 2. + .5;
    } else {
        // shadow
        lum = -pow(-lum * unit2nnreal(1. - black_pt), unit2nnreal(shadow)) / 2. + .5;
    }

    color *= clamp(lum, 0., 1.)/lum0;

    gl_FragColor = color;
}
"#;

