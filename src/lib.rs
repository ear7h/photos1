#![allow(dead_code, unused_variables, unused_imports)]

use std::fmt::Debug;
use egui::CtxRef;

use tokio::runtime::Runtime;
use quick_from::QuickFrom;
use async_trait::async_trait;

use glium::{
    implement_vertex,
    program,
    GlObject,
};

use glium::glutin;
use glium::Surface;

use glam::f32::{
    Quat,
    Mat4,
    Vec3,
};

pub mod double_buffer;
use double_buffer::*;

#[derive(Debug, QuickFrom)]
pub enum Error {
    #[quick_from]
    Draw(glium::DrawError),
    #[quick_from]
    Io(std::io::Error),
    #[quick_from]
    Image(image::ImageError),
}


/// Loosely based on Elm architecure, App defines a communication protocol
/// between the render thread and worker threads. Strictly speaking, the
/// methods should not take a Self. However, a reference to self allows some
/// runtime dynamics which are helpful in implementing something like config
/// options.
///
/// The app state is split into two, where Model has to be Send, that is it
/// needs to be capable of being shared between the render and worker threads.
/// The LocalModel lives only in the render thread so it does not have to be
/// Send. The Model will have data the requires blocking operations, like
/// image files, while the LocalModel will have ui state from the immediate
/// mode gui.
#[async_trait]
pub trait App : Send + Sync + Sized {
    /// Sent between render and worker threads
    type Model : Debug + Send + 'static;
    /// Only lives on the render thread
    type LocalModel : Debug + 'static;
    type Msg : Debug + Send + 'static;
    type Error : Debug;

    // name and handle_error do not run on a specified thread, thus should not
    // block or make assumptions of the runtime.
    fn name() -> &'static str;
    fn handle_error(&self, err : Self::Error) {
        println!("error: {:?}", err);
    }

    /// initialize the app state, runs on the render thread
    fn init(ctx : &mut InitCtx, msgs : &mut Vec<Self::Msg>) -> (Self, Self::LocalModel, Self::Model);

    /// render the app to the screen
    fn render(&self,
              ctx : &mut RenderCtx,
              local_model : &mut Self::LocalModel,
              model : &mut Self::Model,
              msgs : &mut Vec<Self::Msg>);

    /// used for managing gpu resources
    fn swap(&self, ctx : &mut SwapCtx, old : &mut Self::Model, new : &mut Self::Model);

    /// the following methods run in the tokio runtime
    async fn update(&'static self,
                    model : &BufBufWrite<Self::Model>,
                    msg : Self::Msg) -> Result<(), Self::Error>;
}


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


#[derive(Clone, Copy)]
struct Vertex {
    position : [f32; 2],
    texcoord : [f32; 2],
}

implement_vertex!(Vertex, position, texcoord);

#[derive(Debug)]
pub struct EffectsRender {
    program : glium::Program,
}

impl EffectsRender {
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

#[derive(Clone, Copy, Debug)]
pub struct ImageId {
    gl_id : std::os::raw::c_uint,
    egui_id : egui::TextureId,
    ctx_id : usize,
}

impl ImageId {
    pub fn gl_id(&self) -> u64 {
        self.gl_id as u64
    }

    pub fn egui_id(&self) -> egui::TextureId {
        self.egui_id
    }
}


pub struct GraphicsCtx {
    // TODO: rename to image_*_buffer
    vertex_buffer : glium::VertexBuffer<Vertex>,
    index_buffer : glium::IndexBuffer<u16>,
    images : Vec<Option<glium::texture::SrgbTexture2d>>,
}

impl GraphicsCtx {
    fn new(display : &glium::Display) -> Self {
        let vertex_buffer = {
            glium::VertexBuffer::new(display,
                &[
                    Vertex { position: [-1.0,  1.0], texcoord: [0.0, 0.0] },
                    Vertex { position: [-1.0, -1.0], texcoord: [0.0, 1.0] },
                    Vertex { position: [ 1.0, -1.0], texcoord: [1.0, 1.0] },
                    Vertex { position: [ 1.0,  1.0], texcoord: [1.0, 0.0] }
                ]
            ).unwrap()
        };

        let index_buffer = glium::IndexBuffer::new(
            display,
            glium::index::PrimitiveType::TriangleStrip,
            &[1 as u16, 2, 0, 3]
        ).unwrap();

        let program = program!(display,
            100 => {
                vertex : include_str!("effects.vert"),
                fragment : include_str!("effects.frag"),
            }
        ).unwrap();

        Self{
            vertex_buffer,
            index_buffer,
            images : Vec::new(),
        }
    }

    fn add_image(
        &mut self,
        display : &glium::Display,
        egui : &mut egui_glium::Painter,
        img : image::RgbaImage) -> ImageId
    {
        let dim = img.dimensions();

        let img = glium::texture::RawImage2d::from_raw_rgba(img.into_raw(), dim);
        let tex = glium::texture::SrgbTexture2d::with_format(
            display,
            img,
            glium::texture::SrgbFormat::U8U8U8,
            glium::texture::MipmapsOption::NoMipmap,
        ).unwrap();

        let gl_id = tex.get_id();

        let non_owned = unsafe {
            glium::texture::SrgbTexture2d::from_id(
                display,
                glium::texture::SrgbFormat::U8U8U8,
                gl_id,
                false,
                glium::texture::MipmapsOption::NoMipmap,
                glium::texture::Dimensions::Texture2d{
                    width: dim.0,
                    height: dim.1,
                }
            )
        };

        let egui_id = egui.register_glium_texture(non_owned);

        for (idx, tex_opt) in self.images.iter_mut().enumerate() {
            if tex_opt.is_none() {
                *tex_opt = Some(tex);
                return ImageId {
                    ctx_id : idx,
                    egui_id,
                    gl_id,
                }
            }
        }

        let idx = self.images.len();
        self.images.push(Some(tex));
        ImageId{
            ctx_id : idx,
            egui_id,
            gl_id
        }
    }

    pub fn delete_image(&mut self, egui : &mut egui_glium::Painter, img_id : ImageId) {
        match self.images.get_mut(img_id.ctx_id) {
            Some(x) => {
                x.take();
            },
            _ => {},
        }

        egui.free_user_texture(img_id.egui_id);
    }

    fn get_image_texture(&self, img_id : ImageId) -> Option<&glium::texture::SrgbTexture2d> {
        match self.images.get(img_id.ctx_id) {
            Some(Some(x)) => Some(x),
            _ => None,
        }
    }

}


fn create_display(title : &str, event_loop: &glutin::event_loop::EventLoop<()>) -> glium::Display {
    let window_builder = glutin::window::WindowBuilder::new()
        .with_resizable(true)
        .with_inner_size(glutin::dpi::LogicalSize {
            width: 800.0,
            height: 600.0,
        })
        .with_title(title);

    let context_builder = glutin::ContextBuilder::new()
        .with_depth_buffer(0)
        .with_srgb(true)
        .with_stencil_buffer(0)
        .with_vsync(true);

    glium::Display::new(window_builder, context_builder, event_loop).unwrap()
}

struct UniformsCons<'a, X, Xs> {
    name : &'a str,
    value : X,
    rest : Xs,
}

impl<'a, X, Xs> glium::uniforms::Uniforms for UniformsCons<'a, X, Xs>
where
    X : glium::uniforms::AsUniformValue,
    Xs : glium::uniforms::Uniforms,
{
    fn visit_values<'b, F : FnMut(&str, glium::uniforms::UniformValue<'b>)>(&'b self, mut visitor : F) {
        visitor(self.name, self.value.as_uniform_value());
        self.rest.visit_values(visitor);
    }
}

pub type InitCtx<'a> = UnrenderCtx<'a>;
pub type SwapCtx<'a> = UnrenderCtx<'a>;

pub struct UnrenderCtx<'a> {
    pub display : &'a glium::Display,
    egui_glium : &'a mut egui_glium::Painter,
    gfx : &'a mut GraphicsCtx,
}

impl UnrenderCtx<'_> {
    pub fn add_image(&mut self, img : image::RgbaImage) -> ImageId {
        self.gfx.add_image(self.display, self.egui_glium, img)
    }

    pub fn delete_image(&mut self, img_id : ImageId) {
        self.gfx.delete_image(self.egui_glium, img_id)
    }
}


pub struct RenderCtx<'a> {
    pub egui : &'a egui::CtxRef,
    pub display : &'a glium::Display,
    gfx : &'a mut GraphicsCtx,
    egui_glium : &'a mut egui_glium::Painter,
    frame : &'a mut glium::Frame,
    background_input : Option<&'a Input>,
    quit : &'a mut bool,
}

impl RenderCtx<'_> {
    pub fn clear_color(&mut self, color : Color) {
        self.frame.clear_color_srgb(color[0], color[1], color[2], color[3]);
    }

    pub fn background_input(&self) -> Option<&Input> {
        self.background_input
    }

    pub fn dimensions(&self) -> (f32, f32) {
        let (x, y) = self.frame.get_dimensions();
        (x as f32, y as f32)
    }

    pub fn add_image(&mut self, img : image::RgbaImage) -> ImageId {
        self.gfx.add_image(self.display, self.egui_glium, img)
    }

    pub fn delete_image(&mut self, img_id : ImageId) {
        self.gfx.delete_image(self.egui_glium, img_id)
    }

    pub fn draw_image_screen<U>(
        &mut self,
        img_id : ImageId,
        trans : &Mat4,
        program : &glium::Program,
        uniforms : U
    ) -> Result<(), Error>
    where
        U : glium::uniforms::Uniforms
    {
        let texture = self.gfx.get_image_texture(img_id).unwrap();

        let tex_width = texture.get_width() as f32;
        let tex_height = texture.get_height().unwrap() as f32;


        let (win_width, win_height) = self.dimensions();

        // modify the translation matrix for gl_coords
        let trans = Mat4::from_scale(Vec3::new(2. / win_width, 2. / win_height, 1.0))
            .mul_mat4(&trans)
            .mul_mat4(&Mat4::from_scale(Vec3::new(win_width / 2., win_height / 2., 1.0)));

        let window_scale = Mat4::from_scale(
            Vec3::new(tex_width / win_width, tex_height / win_height, 1.0),
        );

        let uniforms = UniformsCons {
            name : "matrix",
            value : trans.mul_mat4(&window_scale).to_cols_array_2d(),
            rest : uniforms,
        };

        let uniforms = UniformsCons{
            name : "texture",
            value : texture,
            rest : uniforms,
        };

        Ok(self.frame.draw(
            &self.gfx.vertex_buffer,
            &self.gfx.index_buffer,
            &program,
            &uniforms,
            &Default::default(),
        )?)
    }

    pub fn quit(&mut self) {
        *self.quit = true;
    }
}



#[derive(Debug, Default)]
pub struct Input {
    /// Some if currently in a drag action, the start x, start y, and if
    /// the drag was released since the last frame
    /// TODO: add start time and input button to sense clicks/taps
    pub pointer_drag : Option<(f32, f32, bool)>,
    pub pointer : (f32, f32),
    pub scroll_delta : (f32, f32),
    pub modifiers : glutin::event::ModifiersState,
}

impl Input {
    fn frame_reset(&mut self) {
        if matches!(self.pointer_drag, Some((_, _, true))) {
            self.pointer_drag = None;
        }

        self.scroll_delta = (0.0, 0.0);
    }

    fn update(&mut self, evt : glutin::event::WindowEvent<'_>) {
        use glutin::event::WindowEvent::*;
        use glutin::event::ElementState;
        use glutin::event::MouseScrollDelta;

        use glutin::dpi::PhysicalPosition;

        match evt {
            CursorMoved{position, ..} => {
                self.pointer = (position.x as f32, position.y as f32);
            },
            MouseInput{state, ..} => {
                match state {
                    ElementState::Pressed => {
                        self.pointer_drag = Some((
                            self.pointer.0,
                            self.pointer.1,
                            false,
                        ));
                    },
                    ElementState::Released => {
                        self.pointer_drag.iter_mut().for_each(|(_, _, released)| {
                            *released = true;
                        })
                    }
                }
            },
            ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            },
            MouseWheel{ delta, ..} => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        // TODO: test this code path
                        self.scroll_delta.0 += x;
                        self.scroll_delta.1 += y;
                    },
                    MouseScrollDelta::PixelDelta(PhysicalPosition{x, y}) => {
                        self.scroll_delta.0 -= x as f32;
                        self.scroll_delta.1 -= y as f32;
                    }
                }
            }
            _ => {},
        }
    }

    pub fn drag_delta(&self) -> Option<(f32, f32, bool)> {
        let (x1, y1) = self.pointer;

        self.pointer_drag.map(|(x0,y0, released)| {
            (x1 - x0, y0 - y1, released)
        })
    }
}


type Color = [f32;4];

pub const GRAY : Color = [0.51, 0.51, 0.51, 1.00];

pub struct TestApp();

#[derive(Debug)]
pub struct TestAppLocal {
    effects_render : EffectsRender,
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
        let image = image::load(std::io::Cursor::new(&include_bytes!("./test0.png")[..]),
            image::ImageFormat::Png).unwrap().to_rgba8();

        let image_id = ctx.add_image(image);
        let effects_render = EffectsRender::new(ctx.display);
        let trans = Mat4::IDENTITY;

        (TestApp(), TestAppLocal{effects_render, trans, image_id}, ())

    }

    fn render(&self,
              ctx : &mut RenderCtx,
              local_model : &mut TestAppLocal,
              _model : &mut (),
              _msgs : &mut Vec<Self::Msg>)
    {
        let TestAppLocal{
            effects_render,
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

            effects_render.draw_image_screen(ctx, *image_id, &trans, &Default::default()).unwrap();
    }

    fn swap(&self, _ctx : &mut SwapCtx, _old : &mut (), _new : &mut ()) {}
    async fn update(&'static self, _model : &BufBufWrite<()>, msg : ()) -> Result<(), Error> { Ok(()) }
}

pub fn run_app<A : App + 'static >() {
    let event_loop = glutin::event_loop::EventLoop::with_user_event();
    let display = create_display(A::name(), &event_loop);

    let mut egui_gl = egui_glium::EguiGlium::new(&display);

    let mut gfx = GraphicsCtx::new(&display);
    let mut background_input : Option<Input> = None;


    let mut msgs = Vec::new();

    let mut init_ctx = InitCtx{
        gfx : &mut gfx,
        display : &display,
        egui_glium: egui_gl.ctx_and_painter_mut().1,
    };

    let (app, mut local_model, model) = A::init(&mut init_ctx, &mut msgs);
    let app : &'static A = Box::leak(Box::new(app));
    let app_ref : &'static &'static A = Box::leak(Box::new(app));
    let bufbuf = Box::leak(Box::new(BufBuf::new(model)));
    let task_channel = TaskChannel::<A>::new(app, bufbuf.new_write());

    event_loop.run(move |event, _, control_flow| {

        let next = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next);

        use glutin::event::Event::*;
        use glutin::event::StartCause;

        match (cfg!(windows), event) {
            (true, RedrawEventsCleared) |
            (false, | RedrawRequested(_)) |
            (_, Resumed) => {
                egui_gl.begin_frame(&display);

                let egui_ctx = egui_gl.ctx();
                let mut frame = display.draw();
                let mut quit = false;
                let (egui_ctx, egui_painter) = egui_gl.ctx_and_painter_mut();

                let mut render_ctx = RenderCtx {
                    egui : egui_ctx,
                    egui_glium: egui_painter,
                    gfx : &mut gfx,
                    display : &display,
                    frame : &mut frame,
                    quit : &mut quit,
                    background_input : background_input.as_ref(),
                };


                app_ref.render(&mut render_ctx, &mut local_model, &mut bufbuf.lock(), &mut msgs);

                if let Some(input) = background_input.as_mut() {
                    input.frame_reset();
                }

                let (needs_repaint, shapes) = egui_gl.end_frame(&display);

                if quit {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                } else if needs_repaint {
                    // TODO: force repaint in the ctx
                    *control_flow = glutin::event_loop::ControlFlow::Poll;
                }

                egui_gl.paint(&display, &mut frame, shapes);
                frame.finish().unwrap();
            },
            (_, WindowEvent{ event, .. }) => {
                if egui_gl.is_quit_event(&event) {
                    *control_flow = glium::glutin::event_loop::ControlFlow::Exit;
                    return
                }

                egui_gl.on_event(&event);

                if !egui_gl.ctx().wants_pointer_input() {
                    if background_input.is_none() {
                        background_input = Some(Default::default());
                    }

                    background_input.as_mut().unwrap().update(event);
                } else {
                    background_input = None;
                }

                display.gl_window().window().request_redraw();
            },
            (_, NewEvents(StartCause::ResumeTimeReached{..})) => {
                display.gl_window().window().request_redraw();
            },
            _ => {},
        }

        for msg in msgs.drain(..) {
            task_channel.send(msg);
        }

        bufbuf.swap(|old, new| {
            let mut swap_ctx = SwapCtx{
                gfx : &mut gfx,
                display : &display,
                egui_glium: egui_gl.ctx_and_painter_mut().1,
            };
            app.swap(&mut swap_ctx, old, new)
        });
    });
}


struct TaskChannel<A : App> {
    // TODO: unbounded sender or increase bound size
    sender : tokio::sync::mpsc::Sender<A::Msg>,
    _rt : Runtime,
}

impl <A : App> TaskChannel<A> {
    fn new(app : &'static A, model : BufBufWrite<A::Model>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("photos-workers")
            .build()
            .unwrap();

        let (sender, mut recv) = tokio::sync::mpsc::channel(1);

        rt.spawn(async move {
            loop {
                println!("waiting for message");
                let msg = if let Some(msg) = recv.recv().await {
                    msg
                } else {
                    break
                };

                println!("got msg : {:?}", msg);

                if let Err(err) = app.update(&model, msg).await {
                    app.handle_error(err)
                }
            }
        });

        Self{sender, _rt : rt}
    }

    fn send(&self, msg : A::Msg) {
        println!("sending msg : {:?}", msg);
        self.sender
            .blocking_send(msg)
            .unwrap();
    }
}
