use std::sync::Arc;
use std::fmt::Debug;

use quick_from::QuickFrom;
use async_trait::async_trait;
use tokio::runtime::Runtime;
use tokio::task::{yield_now, block_in_place};
use macroquad::prelude::*;
use parking_lot::{
    Mutex,
    MutexGuard,
};

const EFFECTS_VERTEX_SHADER: &'static str = include_str!("effects.vert");
const EFFECTS_FRAGMENT_SHADER: &'static str = include_str!("effects.frag");

async fn async_lock_mutex<'a, T>(m : &'a Mutex<T>) -> MutexGuard<'a, T> {
    loop {
        match m.try_lock() {
            Some(guard) => return guard,
            None => yield_now().await,
        }
    }
}

struct TaskChannel<A : App> {
    // TODO: unbounded sender or increase bound size
    sender : tokio::sync::mpsc::Sender<A::Msg>,
    _rt : Runtime,
}

impl <A : App> TaskChannel<A> {
    fn new(app : &'static A, model : Arc<Mutex<A::Model>>) -> Self {
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
                    app.handle_error(&mut model.lock(), err)
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
    run_app(&Photos{});
}

fn run_app<A : App>(app : &'static A) {
    macroquad::Window::new(app.name(), async move {
        let mut msgs = Vec::new();
        let model = Arc::new(Mutex::new(app.init(&mut msgs).await));
        let task_channel = TaskChannel::<A>::new(app, Arc::clone(&model));
        loop {
            {
                let mut model_locked = model.lock();
                app.render(&mut model_locked, &mut msgs).await;
                drop(model_locked);
            }

            let frame = next_frame();

            for msg in msgs.drain(..) {
                task_channel.send(msg);
            }

            frame.await;
        }
    });
}

/// Loosely based on Elm architecure, App defines a communication protocol
/// between the render thread and worker threads. Strictly speaking, the
/// methods should not take a Self. However, a reference to self allows some
/// runtime dynamics which are helpful in implementing something like config
/// options.
#[async_trait]
pub trait App : Send + Sync {
    type Model : Debug + Send + 'static;
    type Msg : Debug + Send + 'static;
    type Error : Debug;

    // name and handle_error do not run on a specified thread, thus should not
    // block or make assumptions of the runtime.
    fn name(&self) -> &'static str;
    fn handle_error(&self, _model : &mut Self::Model, err : Self::Error) {
        println!("error: {:?}", err);
    }


    // init and render are run on the render thread
    async fn init(&self, msgs : &mut Vec<Self::Msg>) -> Self::Model;
    async fn render(&self, model : &mut Self::Model, msgs : &mut Vec<Self::Msg>);

    // update is run in a normal async runtime
    async fn update(&self, model : &Mutex<Self::Model>, msg : Self::Msg) -> Result<(), Self::Error>;
}


struct Photos{
    // TODO: add some config options
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

struct Photo {
    id : String,
    data : PhotoData,
    effects : Effects,
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
    Open{
        path : String,
    },
}

#[derive(Debug)]
enum Screen {
    Empty,
    // TODO
    // Gallery,
    Photo(Photo),
}

#[derive(Debug)]
struct Model {
    effects_material : Material,
    next_screen : Option<Screen>,
    screen : Screen,
    errors : Vec<String>,
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

    fn name(&self) -> &'static str {
        "photos"
    }

    async fn init(&self, msgs : &mut Vec<Msg>) -> Self::Model {
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

        Model {
            screen : Screen::Empty,
            next_screen : None,
            errors : Vec::new(),
            effects_material,
        }
    }

    async fn render(&self, model : &mut Model, msgs : &mut Vec<Msg>) {
        clear_background(GRAY);


        // do any unloading
        if model.next_screen.is_some()  {
            match model.screen {
                Screen::Photo(Photo{data : PhotoData::GPU(texture), ..}) => {
                    texture.delete();
                }
                _ => {},
            }

            model.screen = model.next_screen.take().unwrap();
        }

        // do any loading
        match &mut model.screen {
            Screen::Photo(photo) => {
                let effects_material = model.effects_material;
                let effects = &mut photo.effects; // not copy

                let texture = match &photo.data {
                    PhotoData::GPU(texture) => *texture,
                    PhotoData::CPU(image) => {
                        println!("got cpu image");
                        let texture  = Texture2D::from_image(&image);
                        // texture.set_filter(FilterMode::Nearest);
                        photo.data = PhotoData::GPU(texture);
                        println!("loaded gpu texture");
                        texture
                    }
                };

                gl_use_material(model.effects_material);

                macro_rules! set_uniform {
                    ($($val:ident),*) => {
                        $(
                            effects_material
                                .set_uniform(stringify!($val), effects.$val);
                        )*
                    };
                }

                set_uniform!(brightness, contrast, invert, original,
                             highlight, shadow, white_pt, black_pt,
                             temperature);

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
                    egui::TopBottomPanel::top("menu bar")
                    .show(ctx, |ui| {
                        egui::menu::bar(ui, |ui| {
                            egui::menu::menu(ui, "File", |ui| {
                                if ui.button("Open").clicked() {
                                    println!("open!");

                                    msgs.push(Msg::Open{path : "test1.jpg".to_string()});
                                }
                            });
                        });
                    });

                    egui::SidePanel::right("effects")
                    .show(ctx, |ui| {

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
                });

                egui_macroquad::draw();
            }
            _ => {},
        }
    }

    fn handle_error(&self, model : &mut Model, err : Error) {
        let s = format!("{:?}", err);
        println!("{:}", s);
        model.errors.push(s);
    }

    async fn update(&self, model_mutex : &Mutex<Self::Model>, msg : Self::Msg) -> Result<(), Error> {
        dbg!(&msg);

        macro_rules! model_lock {
            () => {
                async_lock_mutex(model_mutex).await
            }
        }

        match msg {
            Msg::Open{path} => {
                let byt = tokio::fs::read(&path).await?;
                let image = image::load_from_memory(&byt)?.to_rgba8();

                model_lock!().next_screen = Some(Screen::Photo(Photo{
                    id : path,
                    data : PhotoData::CPU(Image{
                        height : image.height() as u16,
                        width : image.width() as u16,
                        bytes : image.into_raw(),
                    }),
                    effects : Default::default(),
                }));

                Ok(())
            }
        }
    }
}

