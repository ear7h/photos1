#![feature(crate_visibility_modifier)]

mod double_buffer;
pub use double_buffer::*;

mod task_channel;
use task_channel::TaskChannel;

mod shaders;
pub use shaders::{
    Effects,
    EffectsShader,
};

mod input;
use input::Input;

mod color;
pub use color::*;

mod utils;
use utils::{
    UniformsCons,
    create_display,
};

use std::fmt::Debug;

use quick_from::QuickFrom;
use async_trait::async_trait;

use glium::{
    implement_vertex,
    GlObject,
};

use glium::glutin;
use glium::Surface;

use glam::f32::{
    Mat4,
    Vec3
};


pub type Result<T> = std::result::Result<T, Error>;

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
                    msg : Self::Msg) -> std::result::Result<(), Self::Error>;
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




#[derive(Clone, Copy)]
struct Vertex {
    position : [f32; 2],
    texcoord : [f32; 2],
}

implement_vertex!(Vertex, position, texcoord);


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
    ) -> Result<()>
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


