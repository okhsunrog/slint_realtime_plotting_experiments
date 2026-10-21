use plot_wire::{CAPACITY, CHANNELS};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

pub struct WebGl {
    pub canvas: HtmlCanvasElement,
    pub description: String,
    gl: Gl,
    program: WebGlProgram,
    texture: WebGlTexture,
    vao: WebGlVertexArrayObject,
    viewport: WebGlUniformLocation,
    scale: WebGlUniformLocation,
    grid_pass: WebGlUniformLocation,
    texture_width: usize,
    write: usize,
    filled: usize,
}
impl WebGl {
    pub fn new() -> Result<Self, String> {
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("plot")
            .unwrap()
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "Missing canvas")?;
        let options = js_sys::JSON::parse(r#"{"alpha":false,"antialias":false,"depth":false,"stencil":false,"preserveDrawingBuffer":false,"failIfMajorPerformanceCaveat":true}"#).unwrap();
        let gl = canvas
            .get_context_with_context_options("webgl2", &options)
            .map_err(|e| format!("WebGL2: {e:?}"))?
            .ok_or("Hardware WebGL2 unavailable")?
            .dyn_into::<Gl>()
            .map_err(|_| "Invalid WebGL2 context")?;
        let shader = |kind, source| -> Result<WebGlShader, String> {
            let shader = gl.create_shader(kind).ok_or("Create WebGL shader")?;
            gl.shader_source(&shader, source);
            gl.compile_shader(&shader);
            if gl
                .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
                .as_bool()
                != Some(true)
            {
                return Err(gl.get_shader_info_log(&shader).unwrap_or_default());
            }
            Ok(shader)
        };
        let vertex = shader(Gl::VERTEX_SHADER, include_str!("webgl.vert"))?;
        let fragment = shader(Gl::FRAGMENT_SHADER, include_str!("webgl.frag"))?;
        let program = gl.create_program().ok_or("Create WebGL program")?;
        gl.attach_shader(&program, &vertex);
        gl.attach_shader(&program, &fragment);
        gl.link_program(&program);
        gl.delete_shader(Some(&vertex));
        gl.delete_shader(Some(&fragment));
        if gl
            .get_program_parameter(&program, Gl::LINK_STATUS)
            .as_bool()
            != Some(true)
        {
            return Err(gl.get_program_info_log(&program).unwrap_or_default());
        }
        gl.use_program(Some(&program));
        let location = |name| {
            gl.get_uniform_location(&program, name)
                .ok_or_else(|| format!("Missing uniform {name}"))
        };
        let viewport = location("viewport")?;
        let scale = location("scale")?;
        let grid_pass = location("grid_pass")?;
        gl.uniform1i(Some(&location("samples")?), 0);
        let max = gl
            .get_parameter(Gl::MAX_TEXTURE_SIZE)
            .map_err(|e| format!("Texture limits: {e:?}"))?
            .as_f64()
            .ok_or("Invalid texture limit")? as usize;
        let texture_width = max.min(2048);
        let height = (CAPACITY * CHANNELS).div_ceil(texture_width);
        if height > max {
            return Err("GPU texture too small for sample ring".into());
        }
        let texture = gl.create_texture().ok_or("Create sample texture")?;
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&texture));
        gl.tex_storage_2d(
            Gl::TEXTURE_2D,
            1,
            Gl::R32F,
            texture_width as i32,
            height as i32,
        );
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::NEAREST as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::NEAREST as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
        let vao = gl
            .create_vertex_array()
            .ok_or("Create WebGL vertex array")?;
        gl.bind_vertex_array(Some(&vao));
        let description = format!(
            "WebGL2 · {}",
            gl.get_parameter(Gl::RENDERER)
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        );
        let result = Self {
            canvas,
            description,
            gl,
            program,
            texture,
            vao,
            viewport,
            scale,
            grid_pass,
            texture_width,
            write: 0,
            filled: 0,
        };
        result.check()?;
        Ok(result)
    }
    fn check(&self) -> Result<(), String> {
        if self.gl.is_context_lost() {
            return Err("WebGL2 context lost; reload to recover".into());
        }
        let code = self.gl.get_error();
        if code != Gl::NO_ERROR {
            return Err(format!("WebGL2 error 0x{code:x}"));
        }
        Ok(())
    }
    pub fn reset(&mut self) {
        self.write = 0;
        self.filled = 0;
    }
    fn upload_at(&self, mut at: usize, mut samples: &[f32]) -> Result<(), JsValue> {
        while !samples.is_empty() {
            let x = at % self.texture_width;
            let count = if x == 0 && samples.len() >= self.texture_width {
                samples.len() / self.texture_width * self.texture_width
            } else {
                samples.len().min(self.texture_width - x)
            };
            let width = count.min(self.texture_width);
            let height = count.div_ceil(self.texture_width);
            // WebGL copies synchronously. No WASM allocation or memory growth may
            // occur between creating this view and the texSubImage2D call.
            let view = unsafe { js_sys::Float32Array::view(&samples[..count]) };
            self.gl
                .tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_array_buffer_view(
                    Gl::TEXTURE_2D,
                    0,
                    x as i32,
                    (at / self.texture_width) as i32,
                    width as i32,
                    height as i32,
                    Gl::RED,
                    Gl::FLOAT,
                    Some(view.as_ref()),
                )?;
            at += count;
            samples = &samples[count..];
        }
        Ok(())
    }
    pub fn upload(&mut self, samples: &[f32]) {
        let frames = samples.len() / CHANNELS;
        let first = frames.min(CAPACITY - self.write) * CHANNELS;
        if let Err(e) = self
            .upload_at(self.write * CHANNELS, &samples[..first])
            .and_then(|_| self.upload_at(0, &samples[first..]))
        {
            crate::fail(format!("WebGL upload: {e:?}"));
            return;
        }
        self.write = (self.write + frames) % CAPACITY;
        self.filled = (self.filled + frames).min(CAPACITY);
    }
    pub fn render(&mut self, visible: usize, offset: usize, amplitude: f32) -> Result<(), String> {
        let dpr = web_sys::window().unwrap().device_pixel_ratio();
        let width = ((self.canvas.client_width() as f64 * dpr).round() as u32).clamp(1, 8192);
        let height = ((self.canvas.client_height() as f64 * dpr).round() as u32).clamp(1, 4096);
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }
        let offset = offset.min(self.filled.saturating_sub(2));
        let visible = visible.min(self.filled - offset).max(1);
        let gl = &self.gl;
        gl.viewport(0, 0, width as i32, height as i32);
        gl.use_program(Some(&self.program));
        gl.bind_vertex_array(Some(&self.vao));
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
        gl.uniform4i(
            Some(&self.viewport),
            self.write as i32,
            visible as i32,
            offset as i32,
            width as i32,
        );
        gl.uniform4f(
            Some(&self.scale),
            height as f32,
            0.72 / amplitude,
            dpr as f32,
            self.texture_width as f32,
        );
        gl.uniform1i(Some(&self.grid_pass), 1);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        if self.filled > 0 {
            gl.uniform1i(Some(&self.grid_pass), 0);
            gl.draw_arrays_instanced(Gl::TRIANGLES, 0, 6, (width as usize * CHANNELS) as i32);
        }
        if self.gl.is_context_lost() {
            Err("WebGL2 context lost; reload to recover".into())
        } else {
            Ok(())
        }
    }
}
