use glium::glutin;

crate struct UniformsCons<'a, X, Xs> {
    crate name : &'a str,
    crate value : X,
    crate rest : Xs,
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



crate fn create_display(title : &str, event_loop: &glutin::event_loop::EventLoop<()>) -> glium::Display {
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
