use async_trait::async_trait;

use glam::f32::{
    Quat,
    Mat4,
    Vec3,
};

use photos1::*;

fn main() {
    run_app::<TestApp>();
}

pub struct TestApp();

#[derive(Debug)]
pub struct TestAppLocal {
    effects_shader : EffectsShader,
    trans : Mat4,
    image_id : ImageId,
}

#[async_trait]
impl App for TestApp {
    type Model = ();
    type LocalModel = TestAppLocal;
    type Msg = ();
    type Error = Error;

    fn name() -> &'static str {
        "test app!"
    }

    fn init(ctx : &mut InitCtx, _msgs : &mut Vec<()>) -> (TestApp, TestAppLocal, ()) {
        let image = image::load(std::io::Cursor::new(&include_bytes!("../test0.png")[..]),
            image::ImageFormat::Png).unwrap().to_rgba8();

        let image_id = ctx.add_image(image);
        let effects_shader = EffectsShader::new(ctx.display);
        let trans = Mat4::IDENTITY;

        (TestApp(), TestAppLocal{effects_shader, trans, image_id}, ())

    }

    fn render(&self,
              ctx : &mut RenderCtx,
              local_model : &mut TestAppLocal,
              _model : &mut (),
              _msgs : &mut Vec<Self::Msg>)
    {
        let TestAppLocal{
            effects_shader,
            trans,
            image_id,
        } = local_model;

            let scale = trans.transform_vector3(Vec3::new(1.0, 0.0, 0.0)).length();
            let mut new_scale = scale;

            match ctx.background_input().map(|i| (i.modifiers, i.scroll_delta)) {
                Some((modifiers, (dx, dy))) => {

                    if modifiers.shift() {
                        // zoom
                        new_scale *= 1.0 - dy.clamp(-10.0, 10.0) / 30.0;
                    } else {
                        // pan
                        let pan = Mat4::from_scale_rotation_translation(
                            Vec3::ONE,
                            Quat::from_rotation_z(0.0),
                            Vec3::new(dx, dy, 0.0)
                        );
                        *trans = pan.mul_mat4(&trans);
                    }

                },
                _ => {},
            }

            new_scale = new_scale.clamp(0.125, 8.0);
            if scale != new_scale {
                let (origin_x, origin_y) = ctx.background_input()
                    .map_or((0.0, 0.0), |i| {
                        let (dim_x, dim_y) = ctx.dimensions();
                        let (px, py) = i.pointer;
                        (dim_x/2.0 - px, py - dim_y/2.0)
                    });

                let to = Mat4::from_scale_rotation_translation(
                    Vec3::ONE,
                    Quat::from_rotation_z(0.0),
                    Vec3::new(origin_x, origin_y, 0.0)
                );

                let fro = Mat4::from_scale_rotation_translation(
                    Vec3::ONE,
                    Quat::from_rotation_z(0.0),
                    Vec3::new(-origin_x, -origin_y, 0.0)
                );

                *trans = fro
                    .mul_mat4(&Mat4::from_scale(Vec3::ONE * (new_scale / scale)))
                    .mul_mat4(&to)
                    .mul_mat4(&trans);
            }

            let drag_delta = ctx
                .background_input()
                .map(|i| i.drag_delta())
                .flatten();

            let trans = match drag_delta {
                Some((dx, dy, released)) => {
                    let pan = Mat4::from_scale_rotation_translation(
                        Vec3::ONE,
                        Quat::from_rotation_z(0.0),
                        Vec3::new(dx, dy, 0.0)
                    );

                    if released {
                        *trans = pan.mul_mat4(&trans);
                        *trans
                    } else {
                        pan.mul_mat4(&trans)
                    }
                },
                _ => *trans,
            };


            egui::SidePanel::left("my_side_panel").show(ctx.egui, |ui| {

                ui.heading("Hello!");

                ui
                    .button("Quit")
                    .clicked()
                    .then(|| ctx.quit());

            });


            ctx.clear_color(GRAY);

            effects_shader.draw_image_screen(ctx, *image_id, &trans, &Default::default()).unwrap();
    }

    fn swap(&self, _ctx : &mut SwapCtx, _old : &mut (), _new : &mut ()) {}
    async fn update(&'static self, _model : &BufBufWrite<()>, _msg : ()) -> Result<()> { Ok(()) }
}

