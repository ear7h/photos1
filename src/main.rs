#![allow(dead_code, unused_variables, unused_imports)]
#![allow(unused_macros)]

use std::path::{
    PathBuf,
};

use glam::f32::{
    Quat,
    Mat4,
    Vec3,
};

use async_trait::async_trait;
use egui::CtxRef;

use photos1::*;
use photos1::double_buffer::BufBufWrite;
/*
    App,
    run_app,
    Error,
    double_buffer::BufBufWrite,
};
*/

const EFFECTS_VERTEX_SHADER: &'static str = include_str!("effects.vert");
const EFFECTS_FRAGMENT_SHADER: &'static str = include_str!("effects.frag");


macro_rules! res_unwrap_or {
    ($e:expr, $id:ident, $b:block) => {
        match $e {
            Ok(x) => x,
            Err($id) => $b,
        }
    };
    ($e:expr, $b:block) => {
        match $e {
            Ok(x) => x,
            Err(_) => $b,
        }
    };
}

macro_rules! opt_unwrap_or {
    ($e:expr, $b:block) => {
        match $e {
            Some(x) => x,
            None => $b,
        }
    };
}

macro_rules! spawn_err {
    ($handler:ident, $b:tt) => {
        tokio::spawn(async move {
            let res = (async move $b).await;
            match res {
                Err(err) => $handler.handle_error(err),
                Ok(_) => {},
            }
        })
    }
}

fn main() {
    run_app::<Photos>();
    //run_app::<photos1::TestApp>();
}

struct Photos{ }




enum PhotoData {
    GPU(ImageId),
    CPU(image::RgbaImage),
}

impl PhotoData {
    fn get_image_id(&mut self, ctx : &mut RenderCtx) -> ImageId {
        match self {
            PhotoData::GPU(img_id) => *img_id,
            PhotoData::CPU(img) => {
                let img_id = ctx.add_image(img.clone());
                *self = PhotoData::GPU(img_id);
                img_id
            },
        }
    }
}

struct Photo {
    id : PathBuf,
    height : u16,
    width : u16,
    data : PhotoData,
    effects : Effects,
}

impl Photo {
    async fn new(path : PathBuf) -> Result<Self, Error> {
        let byt = tokio::fs::read(&path).await?;
        let image = image::load_from_memory(&byt)?.to_rgba8();

        Ok(Photo{
            id : path,
            height : image.height() as u16,
            width : image.width() as u16,
            data : PhotoData::CPU(image),
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
    id : PathBuf,
    data : PhotoData,
}

impl Thumb {
    async fn new<P>(path : P, size : f32) -> Result<Self, Error>
    where P : Into<PathBuf>
    {
        let path : PathBuf = path.into();
        let byt = tokio::fs::read(&path).await?;
        let image = image::load_from_memory(&byt)?
            .thumbnail(size as u32, size as u32)
            .into_rgba8();

        Ok(Thumb{
            id : path,
            data : PhotoData::CPU(image),
        })

    }
}

impl std::fmt::Debug for Thumb {
    fn fmt(&self, f : &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumb")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Gallery {
    thumbs : Vec<Thumb>,
}


#[derive(Debug)]
enum PhotoSet {
    Folder(String),
    List(Vec<String>),
}

#[derive(Debug)]
enum Msg {
    // Rename to OpenPhoto/OpenSingle/OpenEditor
    Open{
        path : PathBuf,
    },
    // TODO: when the database is implemented
    // this should be an enum:
    //  enum PhotoSet {
    //      Folder(String), // folder not in database
    //      Album(u32), // an album in the database
    //      Selection(Vec<u32>), // a selection of images in the database
    //  }
    OpenSet(PhotoSet),
        //paths : Vec<String>,
    //}
}

#[derive(Debug)]
struct PhotoScreen {
    photo : Photo,
    offx : f32,
    offy : f32,
    zoom : f32,
}

impl PhotoScreen {
    fn new(photo : Photo) -> Self {
        PhotoScreen {
            photo,
            offx : 0.,
            offy : 0.,
            zoom : 0.25,
        }
    }
}


#[derive(Debug)]
enum Screen {
    Empty,
    Gallery(Gallery),
    Photo(PhotoScreen),
}

impl Screen {
    fn gallery_mut(&mut self) -> Option<&mut Gallery> {
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


#[derive(Debug)]
struct LocalModel {
    effects_render : EffectsRender,
    view_mat : Mat4,
    open_dialog : bool,
    open_dialog_input : String,
}

impl LocalModel {
    fn new(effects_render : EffectsRender) -> Self {
        LocalModel {
            effects_render,
            view_mat : Mat4::IDENTITY,
            open_dialog : false,
            open_dialog_input : "/Users/julio/Pictures/wallpapers/".to_string(),
        }
    }

    fn update_view(&mut self, ctx : &mut RenderCtx<'_>) -> Mat4 {
        let scale = self.view_mat.transform_vector3(Vec3::new(1.0, 0.0, 0.0)).length();
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
                    self.view_mat = pan.mul_mat4(&self.view_mat);
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

            self.view_mat = fro
                .mul_mat4(&Mat4::from_scale(Vec3::ONE * (new_scale / scale)))
                .mul_mat4(&to)
                .mul_mat4(&self.view_mat);
        }

        let drag_delta = ctx
            .background_input()
            .map(|i| i.drag_delta())
            .flatten();

        match drag_delta {
            Some((dx, dy, released)) => {
                let pan = Mat4::from_scale_rotation_translation(
                    Vec3::ONE,
                    Quat::from_rotation_z(0.0),
                    Vec3::new(dx, dy, 0.0)
                );

                if released {
                    self.view_mat = pan.mul_mat4(&self.view_mat);
                    self.view_mat
                } else {
                    pan.mul_mat4(&self.view_mat)
                }
            },
            _ => self.view_mat,
        }
    }
}


#[async_trait]
impl App for Photos {
    type LocalModel = LocalModel;
    type Model = Model;
    type Msg = Msg;
    type Error = Error;

    fn name() -> &'static str {
        "photos"
    }

    fn init(ctx : &mut InitCtx, msgs : &mut Vec<Msg>) -> (Self, Self::LocalModel, Self::Model) {
        let effects_render = EffectsRender::new(ctx.display);

        msgs.push(Msg::OpenSet(PhotoSet::Folder("/Users/julio/Pictures/wallpapers".into())));

        let self_ = Photos {};

        let model = Model {
            screen : Screen::Empty,
            // errors : Vec::new(),
        };


        (self_, LocalModel::new(effects_render), model)
    }

    fn swap(&self, ctx : &mut SwapCtx, old : &mut Model, _new : &mut Model) {
        // TODO: reuse textures from old? allocate textures for new?
        match old.screen {
            Screen::Photo(PhotoScreen{photo: Photo{data : PhotoData::GPU(img_id), ..}, ..}) => {
                ctx.delete_image(img_id);
            }
            _ => {},
        }
    }

    fn render(&self,
              ctx : &mut RenderCtx,
              local_model : &mut LocalModel,
              model : &mut Model,
              msgs : &mut Vec<Msg>)
    {
        ctx.clear_color(GRAY);

        egui::TopBottomPanel::top("menu bar").show(ctx.egui, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu(ui, "File", |ui| {
                    local_model.open_dialog |= ui.button("Open").clicked();

                    if ui.button("Gallery").clicked() {
                        println!("gallery!");
                        msgs.push(Msg::OpenSet(PhotoSet::List(
                            vec![
                                "test0.png".to_string(),
                                "test1.jpg".to_string(),
                            ],
                        )));
                    }
                });
            });
        });

        {
            // TODO: native file open dialog?
            let LocalModel{
                open_dialog,
                open_dialog_input,
                ..
            } = local_model;

            let mut submitted = false;

            egui::Window::new("Open File")
                .collapsible(false)
                .resizable(false)
                .open(open_dialog)
                .show(ctx.egui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Folder name: ");
                        ui.text_edit_singleline(open_dialog_input);
                    });

                    if ui.button("open").clicked() {
                        println!("opening: {}", open_dialog_input);
                        let dir = std::mem::replace(open_dialog_input, String::new());
                        msgs.push(Msg::OpenSet(PhotoSet::Folder(dir)));
                        submitted = true;
                    }
                });

            if submitted {
                *open_dialog = false;
            }
        }

        match &mut model.screen {
            Screen::Empty => {},
            Screen::Photo(photo_screen) => {
                let view_mat = local_model.update_view(ctx);

                let photo = &mut photo_screen.photo;
                let img_id = photo.data.get_image_id(ctx);
                local_model.effects_render.draw_image_screen(
                    ctx,
                    img_id,
                    &view_mat,
                    &photo.effects
                ).unwrap();

                egui::SidePanel::right("effects").resizable(false).show(ctx.egui, |ui| {
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

                /*
                let resp = background(ctx, egui::Sense::drag());
                println!("drag delta: {:?}", resp.drag_delta());
                let scroll_delta = ctx.input().scroll_delta;
                println!("scroll delta: {:?}", scroll_delta);
                */

            }
            Screen::Gallery(gallery) => {
                egui::CentralPanel::default().show(ctx.egui, |ui| {
                    let ncols = 4; //(ui.available_width() / 100.0) as usize + 1;
                    // println!("ncols: {}", ncols);
                    let nrows = gallery.thumbs.len() / ncols;

                    // TODO: just make the rows manually
                    egui::ScrollArea::auto_sized().show_rows(ui, 100.0, nrows, |ui, rng| {

                        let start = rng.start * ncols;
                        let end = rng.end * ncols;
                        for row in gallery.thumbs[start..end].chunks_mut(ncols) {
                            ui.horizontal(|ui| {
                                for photo in row {
                                    let egui_id = photo.data.get_image_id(ctx).egui_id();
                                    let button = ui.add(egui::ImageButton::new(
                                        egui_id,
                                        egui::Vec2{
                                            x : 100.0,
                                            y : 100.0,
                                        }
                                    ));

                                    if button.on_hover_text(photo.id.display()).clicked() {
                                        println!("loading {}", photo.id.display());
                                        msgs.push(Msg::Open{path : photo.id.clone()});
                                    }
                                }
                            });
                        }
                    })
                });
            },
        }
    }

    fn handle_error(&self, err : Error) {
        let s = format!("{:?}", err);
        println!("{:}", s);
        // model.errors.push(s);
    }

    async fn update(&'static self, model_buf : &BufBufWrite<Self::Model>, msg : Self::Msg) ->
        Result<(), Error> {

        dbg!(&msg);

        match msg {
            Msg::Open{path} => {
                let photo = Photo::new(path).await?;
                model_buf.set_next(Model{
                    screen : Screen::Photo(PhotoScreen::new(photo)),
                });

                Ok(())
            },
            Msg::OpenSet(photo_set) => {
                let weak = model_buf.set_next(Model{
                    screen : Screen::Gallery(Gallery{
                        thumbs : Vec::new()
                    })
                });

                spawn_err!(self, {
                    match photo_set {
                        PhotoSet::Folder(path) => {
                            let mut entries = tokio::fs::read_dir(path).await?;

                            while let Some(entry) = entries.next_entry().await? {
                                println!("{:?}", entry);
                                let thumb = Thumb::new(entry.path(), 100.0).await?;

                                let model = opt_unwrap_or!(weak.upgrade(), {
                                    // the screen was dropped
                                    break;
                                });

                                model
                                    .lock().unwrap()
                                    .screen
                                    .gallery_mut().unwrap()
                                    .thumbs
                                    .push(thumb);
                            }

                            Ok(())
                        },
                        PhotoSet::List(paths) => {
                            for path in paths {
                                let thumb = Thumb::new(path, 100.0).await?;

                                let model = opt_unwrap_or!(weak.upgrade(), {
                                    // the screen was dropped
                                    break;
                                });

                                model
                                    .lock().unwrap()
                                    .screen
                                    .gallery_mut().unwrap()
                                    .thumbs
                                    .push(thumb);
                            }

                            Ok(())
                        }
                    }
                });

                Ok(())
            }
        }
    }
}
