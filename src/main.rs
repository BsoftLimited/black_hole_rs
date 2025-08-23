use std::f64::consts::PI;
use std::ffi::CString;
use std::io::Read;
use std::{mem, ptr};
use std::os::raw::c_void;
use gl::types::{GLchar, GLfloat, GLint, GLsizei, GLsizeiptr};
use glfw::{Action, Context, Key};
use glm::{vec3, Vec3};

// VARS
static mut lastPrintTime: f64 = 0.0;
static  mut framesCount: i64   = 0;
static c: f64 = 299792458.0;
static G:f64 = 6.67430e-11;
struct Ray;
static mut Gravity: bool = false;

fn read(file: &str)->String{
    let  mut content = String::new();
    match &mut std::fs::File::open(file){
        Ok(file) =>{
            if let Result::Err(error) = file.read_to_string(&mut content){
                panic!("{}", error);
            }
        },
        Err(error)=>{ panic!("{}", error); }
    }
    content
}

struct Camera {
    target: Vec3, radius: f64, min_radius: f64, max_radius: f64,
    azimuth: f64, elevation: f64,
    orbit_speed: f64, pan_speed: f64, zoom_speed: f64,
    dragging: bool, panning: bool, moving: bool, last_x: f64, last_y: f64,
}

impl Camera {
    fn new() -> Self {
        Camera{
            target: vec3(0.0, 0.0, 0.0), radius: 6.34194e10, min_radius: 1e10, max_radius: 1e12,
            azimuth: 0.0, elevation: PI / 2.0,
            orbit_speed: 0.01, pan_speed: 0.01, zoom_speed: 25e9,
            dragging: false, panning: false, moving: false, last_x: 0.0, last_y: 0.0
        }
    }

    // Calculate camera position in world space
    fn position(&self) -> Vec3 {
        let clamped_elevation = glm::clamp(self.elevation, 0.01, PI - 0.01);
        // Orbit around (0,0,0) always
        return vec3(
            (self.radius * f64::sin(clamped_elevation) * f64::cos(self.azimuth)) as f32,
            (self.radius * f64::cos(clamped_elevation)) as f32,
            (self.radius * f64::sin(clamped_elevation) * f64::sin(self.azimuth)) as f32);
    }

    fn update(&mut self) {
        // Always keep target at black hole center
        self.target = vec3(0.0, 0.0, 0.0);
        if self.dragging || self.panning {
            self.moving = true;
        } else {
            self.moving = false;
        }
    }

    fn process_mouse_move(&mut self, x: f64, y: f64) {
        let dx = x - self.last_x;
        let dy = y - self.last_y;

        if self.dragging && self.panning {
            // Pan: Shift + Left or Middle Mouse
            // Disable panning to keep camera centered on black hole
        } else if self.dragging && !self.panning {
            // Orbit: Left mouse only
            self.azimuth   += dx * self.orbit_speed;
            self.elevation -= dy * self.orbit_speed;
            self.elevation = glm::clamp(self.elevation, 0.01, PI - 0.01);
        }

        self.last_x = x;
        self.last_y = y;
        self.update();
    }

    fn process_mouse_button(&mut self, button: glfw::MouseButton, action: glfw::Action, mods: i32, win: &glfw::Window) {
        if button == glfw::MouseButtonLeft || button == glfw::MouseButtonMiddle {
            if action == glfw::Action::Press{
                self.dragging = true;
                // Disable panning so camera always orbits center
                self.panning = false;
                (self.last_x, self.last_y) = win.get_cursor_pos();
            } else if action == Action::Release {
                self.dragging = false;
                self.panning = false;
            }
        }

        if (button == glfw::MouseButtonRight) {
            unsafe {
                if action == glfw::Action::Press{
                    Gravity = true;
                } else if action == Action::Release {
                    Gravity = false;
                }
            }
        }
    }

    fn process_scroll(&mut self, xoffset: f64, yoffset: f64) {
        self.radius -= yoffset * self.zoom_speed;
        self.radius = glm::clamp(self.radius, self.min_radius, self.max_radius);
        self.update();
    }

    fn process_key(key: glfw::Key, scancode: glfw::Scancode, action: glfw::Action, mods: glfw::Modifiers) {
        if action == glfw::Action::Press && key == glfw::Key::G {
            unsafe {
                Gravity = !Gravity;
                println!("[INFO] Gravity turned {}", if Gravity { "ON"} else {"OFF"});
            }
        }
    }
}

struct Engine {
    grid_shader_program: gl::types::GLuint,
    // -- Quad & Texture render -- //
    window: Box<glfw::PWindow>,
    quad_vao: gl::types::GLuint,
    texture: gl::types::GLuint,
    shader_program: gl::types::GLuint,
    compute_program: gl::types::GLuint,
    // -- UBOs -- //
    camera_ubo: gl::types::GLuint,
    disk_ubo: gl::types::GLuint,
    objects_ubo: gl::types::GLuint,
    // -- grid mess vars -- //
    grid_vao: gl::types::GLuint,
    grid_vbo: gl::types::GLuint,
    grid_ebo: gl::types::GLuint,
    grid_index_count: gl::types::GLsizei,// originally int

    win_width: u32,  // Window width
    win_height: u32, // Window height
    compute_width: i32,   // Compute resolution width
    compute_height: i32, // Compute resolution height
    width: f64, // Width of the viewport in meters
    height: f64 // Height of the viewport in meters
}

impl Engine {
    fn new() -> Self {
        let win_width: u32 = 800;  // Window width
        let win_height: u32 = 600;

        let compute_width= 200;   // Compute resolution width
        let compute_height= 150;

        let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();
        glfw.window_hint(glfw::WindowHint::ContextVersionMajor(4));
        glfw.window_hint(glfw::WindowHint::ContextVersionMinor(3));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));

        let (mut window, events) = glfw.create_window(win_width, win_height, "Black Hole", glfw::WindowMode::Windowed)
            .expect("Failed to create GLFW window.");

        window.make_current();
        window.set_key_polling(true);

        gl::load_with(|s| window.get_proc_address(s).unwrap() as *const _);

        let shader_program = Engine::create_shader_program("./shaders/main_vs.glsl", "./shaders/main_fs.glsl");
        let compute_program = Engine::create_compute_program("./shaders/geodesic_cs.glsl");
        let grid_shader_program = Engine::create_shader_program("./shaders/grid_vs.glsl", "./shaders/grid_fs.glsl");

        let (mut camera_ubo, mut disk_ubo, mut objects_ubo) = (0, 0, 0);
        unsafe {
            gl::GenBuffers(1, &mut camera_ubo);
            gl::BindBuffer(gl::UNIFORM_BUFFER, camera_ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, 128, ptr::null_mut(), gl::DYNAMIC_DRAW); // alloc ~128 bytes
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 1, camera_ubo); // binding = 1 matches shader

            gl::GenBuffers(1, &mut disk_ubo);
            gl::BindBuffer(gl::UNIFORM_BUFFER, disk_ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, (4 * mem::size_of::<GLfloat>()) as GLsizeiptr, ptr::null_mut(), gl::DYNAMIC_DRAW); // 3 values + 1 padding
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 2, disk_ubo); // binding = 2 matches compute shader

            gl::GenBuffers(1, &mut objects_ubo);
            gl::BindBuffer(gl::UNIFORM_BUFFER, objects_ubo);
            // allocate space for 16 objects:
            // sizeof(int) + padding + 16×(vec4 posRadius + vec4 color)
            let obj_ubosize = mem::size_of::<GLint>() + 3 * mem::size_of::<GLfloat>()
                + 16 * (mem::size_of::<GLfloat>() * 4 + mem::size_of::<GLfloat>() * 4)
                + 16 * mem::size_of::<GLfloat>(); // 16 floats for mass
            gl::BufferData(gl::UNIFORM_BUFFER, obj_ubosize as GLsizeiptr, ptr::null_mut(), gl::DYNAMIC_DRAW);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 3, objects_ubo);  // binding = 3 matches shader
        }

        let result = Self::quad_vao(compute_width, compute_height);
        let quad_vao = result[0];
        let texture = result[1];

        Engine{
            window: Box::new(window),
            quad_vao,
            texture,
            shader_program,
            compute_program,
            grid_shader_program,
            // -- UBOs -- //
            camera_ubo,
            disk_ubo,
            objects_ubo,
            // -- grid mess vars -- //
            grid_vao: 0,
            grid_vbo: 0,
            grid_ebo: 0,
            grid_index_count: 0,

            win_width,  // Window width
            win_height, // Window height
            compute_width,   // Compute resolution width
            compute_height,  // Compute resolution height
            width: 100000000000.0, // Width of the viewport in meters
            height: 75000000000.0
        }
    }
    fn compile_shader(shader_type: u32, shader_source:&str) -> u32{
        unsafe {
            // Setup shader compilation checks
            let mut success = i32::from(gl::FALSE);
            let mut info_log = Vec::with_capacity(512);
            info_log.set_len(512 - 1); // -1 to skip trialing null character

            // Vertex shader
            let shader = gl::CreateShader(shader_type);
            let c_str_vert = CString::new(shader_source.as_bytes()).unwrap();
            gl::ShaderSource(shader, 1, &c_str_vert.as_ptr(), ptr::null());
            gl::CompileShader(shader);

            // Check for shader compilation errors
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
            if success != i32::from(gl::TRUE) {
                gl::GetShaderInfoLog( shader, 512, ptr::null_mut(), info_log.as_mut_ptr() as *mut GLchar,);
                panic!("ERROR::SHADER::VERTEX::COMPILATION_FAILED\n{}", String::from_utf8(info_log).unwrap());
            }

            shader
        }
    }
    fn create_shader_program(verter_path: &str, frag_path: &str)-> gl::types::GLuint {
        let vertex_shader_source = read(verter_path);
        let fragment_shader_source =read(frag_path);
        unsafe {
            // vertex shader
            let vertex_shader = Engine::compile_shader(gl::VERTEX_SHADER, vertex_shader_source.as_str());
            // fragment shader
            let fragment_shader = Engine::compile_shader(gl::FRAGMENT_SHADER, fragment_shader_source.as_str());
            let sharder_program = gl::CreateProgram();
            gl::AttachShader(sharder_program, vertex_shader);
            gl::AttachShader(sharder_program, fragment_shader);

            let mut success = i32::from(gl::FALSE);
            gl::LinkProgram(sharder_program);
            gl::GetProgramiv(sharder_program, gl::LINK_STATUS, &mut success);
            if success != i32::from(gl::TRUE) {
                let mut logLen: gl::types::GLsizei = 0;
                gl::GetProgramiv(sharder_program, gl::INFO_LOG_LENGTH, &mut logLen);

                let mut buf = Vec::with_capacity(logLen as usize);
                gl::GetProgramInfoLog(sharder_program, logLen, ptr::null_mut(), buf.as_mut_ptr() as *mut GLchar);
                panic!("Shader link error:\n{}", String::from_utf8(buf).unwrap());
            }

            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);

            sharder_program
        }
    }

    fn create_compute_program(path: &str) -> gl::types::GLuint {
        // 1) read GLSL source
        let src = read(path);

        // 2) compile
        let cs = Engine::compile_shader(gl::COMPUTE_SHADER, src.as_str());

        // 3) link
        let mut success = i32::from(gl::FALSE);
        let compute_program = unsafe {
            let prog = gl::CreateProgram();
            gl::AttachShader(prog, cs);
            gl::LinkProgram(prog);
            gl::GetProgramiv(prog, gl::LINK_STATUS, &mut success);
            if success != i32::from(gl::TRUE) {
                let mut logLen: gl::types::GLsizei = 0;
                gl::GetProgramiv(prog, gl::INFO_LOG_LENGTH, &mut logLen);

                let mut buf = Vec::with_capacity(logLen as usize);
                gl::GetProgramInfoLog(prog, logLen, ptr::null_mut(), buf.as_mut_ptr() as *mut GLchar);
                panic!("Compute shader link error:\n{}", String::from_utf8(buf).unwrap());
            }
            gl::DeleteShader(cs);
            prog
        };

        compute_program
    }

    fn quad_vao(compute_width: i32, compute_height: i32) -> Vec<gl::types::GLuint> {
        let quad_vertices = [
            // positions   // texCoords
            -1.0,  1.0,  0.0, 1.0,  // top left
            -1.0, -1.0,  0.0, 0.0,  // bottom left
            1.0, -1.0,  1.0, 0.0,  // bottom right

            -1.0,  1.0,  0.0, 1.0,  // top left
            1.0, -1.0,  1.0, 0.0,  // bottom right
            1.0,  1.0,  1.0, 1.0   // top right
        ];

        let mut vao: gl::types::GLuint = 0;
        let mut vbo: gl::types::GLuint = 0;
        let mut texture: gl::types::GLuint = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(gl::ARRAY_BUFFER,  (mem::size_of::<GLfloat>() * quad_vertices.len()) as GLsizeiptr,  (&quad_vertices[0] as *const _) as *const c_void, gl::STATIC_DRAW);

            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, (4 * mem::size_of::<GLfloat>()) as GLsizei, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 2, gl::FLOAT,gl::FALSE, (4 * mem::size_of::<GLfloat>()) as GLsizei, (2 * mem::size_of::<GLfloat>()) as *const c_void);
            gl::EnableVertexAttribArray(1);

            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as gl::types::GLint);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as gl::types::GLint);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexImage2D(gl::TEXTURE_2D, 0,             // mip
                         gl::RGBA8 as gl::types::GLint,      // internal format
                         compute_width, compute_height, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
        }

        vec!(vao, texture)
    }
}

fn main() {
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();

    // Set OpenGL version (e.g., 3.3 Core Profile)
    glfw.window_hint(glfw::WindowHint::ContextVersionMajor(4));
    glfw.window_hint(glfw::WindowHint::ContextVersionMinor(3));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    //glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));

    let (mut window, events) = glfw.create_window(800, 480, "Hello this is window", glfw::WindowMode::Windowed)
        .expect("Failed to create GLFW window.");

    window.make_current();
    window.set_key_polling(true);

    gl::load_with(|s| window.get_proc_address(s).unwrap() as *const _);

    unsafe {
        gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
        gl::FrontFace(gl::CW);
        gl::CullFace(gl::BACK);
        gl::Enable(gl::CULL_FACE);
    }

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, event);
        }

        unsafe {
            gl::ClearColor(0.0, 0.0, 0.2, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        window.swap_buffers();
    }
}

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            window.set_should_close(true)
        }
        _ => {}
    }
}