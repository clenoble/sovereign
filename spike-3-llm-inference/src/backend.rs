use anyhow::Result;

pub trait ModelBackend: Sized {
    fn load(path: &str, n_gpu_layers: u32, n_ctx: u32) -> Result<Self>;
    fn generate(&mut self, prompt: &str, max_tokens: u32) -> Result<String>;
    fn unload(self); // Consumes self, triggers drop
}
