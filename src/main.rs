#![allow(dead_code, unused_variables, unused_imports)]
use std::sync::{
    Arc,
    Mutex,
};

use std::fmt::Debug;

use quick_from::QuickFrom;
use async_trait::async_trait;
use tokio::runtime::Runtime;
use tokio::task::yield_now;
use egui::CtxRef;
use macroquad::prelude::*;

mod double_buffer;
use double_buffer::*;

const EFFECTS_VERTEX_SHADER: &'static str = include_str!("effects.vert");
const EFFECTS_FRAGMENT_SHADER: &'static str = include_str!("effects.frag");

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

fn main() {
    run_app::<Photos>();
}

fn run_app<A : App + 'static>() {
    macroquad::Window::new(A::name(), async move {
        let mut msgs = Vec::new();
        let (app, model) = A::init(&mut msgs).await;
        let app = Box::leak(Box::new(app));
        let bufbuf = Box::leak(Box::new(BufBuf::new(model)));
        let task_channel = TaskChannel::<A>::new(app, bufbuf.new_write());
        loop {
            egui_macroquad::ui(|ctx| {
                app.render(ctx, &mut bufbuf.lock(), &mut msgs);
            });
            egui_macroquad::draw();

            for msg in msgs.drain(..) {
                task_channel.send(msg);
            }

            bufbuf.swap(|old, new| {
                app.swap(old, new);
            });

            next_frame().await;
        }
    });
}

/// Loosely based on Elm architecure, App defines a communication protocol
/// between the render thread and worker threads. Strictly speaking, the
/// methods should not take a Self. However, a reference to self allows some
/// runtime dynamics which are helpful in implementing something like config
/// options.
#[async_trait]
pub trait App : Send + Sync + Sized {
    type Model : Debug + Send + 'static;
    type Msg : Debug + Send + 'static;
    type Error : Debug;

    // name and handle_error do not run on a specified thread, thus should not
    // block or make assumptions of the runtime.
    fn name() -> &'static str;
    fn handle_error(&self, err : Self::Error) {
        println!("error: {:?}", err);
    }

    // the following methods run on the macroquad thread
    async fn init(msgs : &mut Vec<Self::Msg>) -> (Self, Self::Model);
    fn render(&self, ctx : &CtxRef, model : &mut Self::Model, msgs : &mut Vec<Self::Msg>);
    // used for managing gpu resources
    fn swap(&self, old : &mut Self::Model, new : &mut Self::Model);

    // the following methods run in the tokio runtime
    async fn update(&self, model : &BufBufWrite<Self::Model>, msg : Self::Msg) -> Result<(), Self::Error>;
}


struct Photos{
    effects_material : Material,
}

#[derive(Debug, Clone)]
struct Effects {
    brightness : f32,
    contrast : f32,
    invert : u32,
    highlight : f32,
    shadow : f32,
    white_pt : f32,
    black_pt : f32,
    temperature : f32,
    original : u32,
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


enum PhotoData {
    GPU(Texture2D),
    CPU(Image),
}

impl PhotoData {
    fn get_texture(&mut self) -> Texture2D {
        match self {
            PhotoData::GPU(texture) => *texture,
            PhotoData::CPU(image) => {
                let texture = Texture2D::from_image(&image);
                *self = PhotoData::GPU(texture);
                texture
            },
        }
    }
}

struct Photo {
    id : String,
    data : PhotoData,
    effects : Effects,
}

impl Photo {
    async fn new(path : String) -> Result<Self, Error> {
        let byt = tokio::fs::read(&path).await?;
        let image = image::load_from_memory(&byt)?.to_rgba8();

        Ok(Photo{
            id : path,
            data : PhotoData::CPU(Image{
                height : image.height() as u16,
                width : image.width() as u16,
                bytes : image.into_raw(),
            }),
            effects : Default::default(),
        })
    }
}

impl std::fmt::Debug for Photo {
    fn fmt(&self, f : &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("id", &self.id)
            .field("effects", &self.effects)
            .finish_non_exhaustive()
    }
}

// like a Photo but, probably, lower resolution and the data
// might not be filled in yet.
struct Thumb {
    id : String,
    data : Option<PhotoData>,
}

struct Gallery {
    thumbs : Vec<Thumb>,
}


#[derive(Debug)]
enum Msg {
    // Rename to OpenPhoto/OpenSingle/OpenEditor
    Open{
        path : String,
    },
    // TODO: when the database is implemented
    // this should be an enum:
    //  enum PhotoSet {
    //      Folder(String), // folder not in database
    //      Album(u32), // an album in the database
    //      Selection(Vec<u32>), // a selection of images in the database
    //  }
    OpenSet {
        paths : Vec<String>,
    }
}

#[derive(Debug)]
enum Screen {
    Empty,
    Gallery(Vec<Photo>),
    Photo(Photo),
}

impl Screen {
    fn gallery_mut(&mut self) -> Option<&mut Vec<Photo>> {
        match self {
            Screen::Gallery(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Model {
    screen : Screen,
}

#[derive(Debug, QuickFrom)]
enum Error {
    #[quick_from]
    Io(std::io::Error),
    #[quick_from]
    Image(image::ImageError),
}

#[async_trait]
impl App for Photos {
    type Model = Model;
    type Msg = Msg;
    type Error = Error;

    fn name() -> &'static str {
        "photos"
    }

    async fn init(msgs : &mut Vec<Msg>) -> (Self, Self::Model) {
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
                    ("original".to_string(), UniformType::Int1),
                ],
                ..Default::default()
            },
        ).unwrap_or_else(|err| {
            println!("{}", err);
            std::process::exit(1);
        });

        msgs.push(Msg::Open{path : "test0.png".to_string()});

        let self_ = Photos {
            effects_material,
        };

        let model = Model {
            screen : Screen::Empty,
            // next_screen : None,
            // errors : Vec::new(),
            // effects_material,
        };

        (self_, model)
    }

    fn swap(&self, old : &mut Model, new : &mut Model) {
        match old.screen {
            Screen::Photo(Photo{data : PhotoData::GPU(texture), ..}) => {
                texture.delete();
            }
            _ => {},
        }
    }

    fn render(&self, ctx : &CtxRef, model : &mut Model, msgs : &mut Vec<Msg>) {
        clear_background(GRAY);

        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu(ui, "File", |ui| {
                    if ui.button("Open").clicked() {
                        println!("open!");
                        msgs.push(Msg::Open{path : "test1.jpg".to_string()});
                    }

                    if ui.button("Gallery").clicked() {
                        println!("gallery!");
                        msgs.push(Msg::OpenSet{
                            paths : vec![
                                "test0.png".to_string(),
                                "test1.jpg".to_string(),
                            ],
                        });
                    }
                });
            });
        });

        match &mut model.screen {
            Screen::Empty => {},
            Screen::Photo(photo) => {

                macro_rules! set_uniform {
                    ($($val:ident),*) => {
                        $(
                            self.effects_material
                                .set_uniform(stringify!($val), photo.effects.$val);
                        )*
                    };
                }

                set_uniform!(brightness, contrast, invert, original,
                             highlight, shadow, white_pt, black_pt,
                             temperature);

                gl_use_material(self.effects_material);

                draw_texture_ex(
                    photo.data.get_texture(),
                    0.0,
                    0.0,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(480.0, 720.0)),
                        ..Default::default()
                    },
                );

                gl_use_default_material();

                egui::SidePanel::right("effects").show(ctx, |ui| {
                    let effects = &mut photo.effects;

                    ui.label("brightness");
                    ui.add(egui::Slider::new(&mut effects.brightness, -0.5..=0.5));

                    ui.label("contrast");
                    ui.add(egui::Slider::new(&mut effects.contrast, 0.0..=1.0));

                    let mut invert = effects.invert > 0;
                    ui.checkbox(&mut invert, "invert");
                    effects.invert = if invert { 1 } else { 0 };

                    let mut original = effects.original > 0;
                    ui.checkbox(&mut original, "original");
                    effects.original = if original { 1 } else { 0 };

                    // ui.separator();

                    ui.label("highlight");
                    ui.add(egui::Slider::new(&mut effects.highlight, 0.0..=1.0));

                    ui.label("shadow");
                    ui.add(egui::Slider::new(&mut effects.shadow, 0.0..=1.0));

                    ui.label("white point");
                    ui.add(egui::Slider::new(&mut effects.white_pt, 0.0..=1.0));

                    ui.label("black point");
                    ui.add(egui::Slider::new(&mut effects.black_pt, 0.0..=1.0));

                    // ui.separator();

                    ui.label("temperature");
                    ui.add(egui::Slider::new(&mut effects.temperature, 4000.0..=9000.0));
                });
            }
            Screen::Gallery(photos) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for photo in photos.iter_mut() {
                        let texture = photo.data.get_texture();
                        let button = ui.add(egui::ImageButton::new(
                            egui::TextureId::User(
                                texture
                                    .raw_miniquad_texture_handle()
                                    .gl_internal_id()
                                    .into()
                            ),
                            egui::Vec2{
                                x : 100.0,
                                y : 100.0,
                            }
                        ));

                        if button.on_hover_text(&photo.id).clicked() {
                            println!("loading {}", photo.id);
                            msgs.push(Msg::Open{path : photo.id.to_string()});
                        }
                    }
                });
            },
        }
    }

    fn handle_error(&self, err : Error) {
        let s = format!("{:?}", err);
        println!("{:}", s);
        // model.errors.push(s);
    }

    async fn update(&self, model_buf : &BufBufWrite<Self::Model>, msg : Self::Msg) ->
        Result<(), Error> {

        dbg!(&msg);

        match msg {
            Msg::Open{path} => {
                let photo = Photo::new(path).await?;
                model_buf.set_next(Model{
                    screen : Screen::Photo(photo),
                });

                Ok(())
            },
            Msg::OpenSet{paths} => {
                let weak = model_buf.set_next(Model{
                    screen : Screen::Gallery(Vec::new())
                });

                // tokio::spawn(async move {
                    for path in paths {
                        let photo = match Photo::new(path).await {
                            Ok(photo) => photo,
                            Err(err) => {
                                self.handle_error(err);
                                break;
                            },
                        };

                        match weak.upgrade() {
                            None => break,
                            Some(arc) => {
                                arc
                                    .lock()
                                    .unwrap()
                                    .screen
                                    .gallery_mut()
                                    .unwrap()
                                    .push(photo);
                            },
                        }
                    }
                // });

                Ok(())
            }
        }
    }
}

