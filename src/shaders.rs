use crate::{
    Error,
    ImageId,
    RenderCtx,
};

use glium::{
    program,
};

use glam::f32::{
    Mat4,
};

#[derive(Debug, Clone)]
pub struct Effects {
    pub brightness : f32,
    pub contrast : f32,
    pub invert : i32,
    pub highlight : f32,
    pub shadow : f32,
    pub white_pt : f32,
    pub black_pt : f32,
    pub temperature : f32,
    pub original : i32,
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
            original : 0,
        }
    }
}


#[derive(Debug)]
pub struct EffectsShader {
    program : glium::Program,
}

impl EffectsShader {
    pub fn new(display : &glium::Display) -> Self {
        let program = program!(display,
            100 => {
                vertex : include_str!("effects.vert"),
                fragment : include_str!("effects.frag"),
            }
        ).unwrap();

        Self{ program }
    }

    pub fn draw_image_screen(
        &self,
        ctx : &mut RenderCtx,
        img_id : ImageId,
        trans : &Mat4,
        effects : &Effects
    ) -> Result<(), Error> {

        macro_rules! effects_uniforms {
            ($val0:ident,$($val:ident),*,) => {
                {
                    let uniforms = glium::uniforms::UniformsStorage::new(
                        stringify!($val0),
                        effects.$val0
                    );

                    $(
                        let uniforms = uniforms.add(stringify!($val), effects.$val);
                    )*

                    uniforms
                }
            };
        }

        let uniforms = effects_uniforms!(
            brightness, contrast, invert, original,
            highlight, shadow, white_pt, black_pt, temperature,
        );


        ctx.draw_image_screen(img_id, trans, &self.program, uniforms)
    }
}
