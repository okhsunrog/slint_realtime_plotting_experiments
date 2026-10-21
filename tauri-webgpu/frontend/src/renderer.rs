use crate::{gpu::Gpu, webgl::WebGl};

pub struct Renderer {
    pub canvas: web_sys::HtmlCanvasElement,
    pub description: String,
    backend: Backend,
}
enum Backend {
    WebGpu(Gpu),
    WebGl(WebGl),
}
impl Renderer {
    pub async fn new(webgl: bool) -> Result<Self, String> {
        if webgl {
            let g = WebGl::new()?;
            Ok(Self {
                canvas: g.canvas.clone(),
                description: g.description.clone(),
                backend: Backend::WebGl(g),
            })
        } else {
            let g = Gpu::new().await?;
            Ok(Self {
                canvas: g.canvas.clone(),
                description: g.description.clone(),
                backend: Backend::WebGpu(g),
            })
        }
    }
    pub fn reset(&mut self) {
        match &mut self.backend {
            Backend::WebGpu(g) => g.reset(),
            Backend::WebGl(g) => g.reset(),
        }
    }
    pub fn upload(&mut self, samples: &[f32]) {
        match &mut self.backend {
            Backend::WebGpu(g) => g.upload(samples),
            Backend::WebGl(g) => g.upload(samples),
        }
    }
    pub fn render(&mut self, visible: usize, offset: usize, amplitude: f32) -> Result<(), String> {
        match &mut self.backend {
            Backend::WebGpu(g) => g.render(visible, offset, amplitude),
            Backend::WebGl(g) => g.render(visible, offset, amplitude),
        }
    }
    pub async fn verify_compute(&self) -> Result<String, String> {
        match &self.backend {
            Backend::WebGpu(g) => g.verify_compute().await,
            Backend::WebGl(_) => Ok("Compute disabled · min/max in WebGL2 vertex shader".into()),
        }
    }
}
