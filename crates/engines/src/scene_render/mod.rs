//! Shared safe-Rust rendering for the desktop and headless frame/video exports.
pub mod animation;
pub mod avi;
pub mod camera;
pub mod draw_plan;
pub mod gpu;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Palette {
    pub canvas: [f32; 4],
    pub text_dim: [f32; 4],
    pub accent: [f32; 4],
    pub warning: [f32; 4],
    pub success: [f32; 4],
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            canvas: [0.75, 0.83, 0.9, 1.],
            text_dim: [0.3, 0.35, 0.4, 1.],
            accent: [0.2, 0.4, 0.8, 1.],
            warning: [1., 0.7, 0.15, 1.],
            success: [0.2, 0.7, 0.35, 1.],
        }
    }
}
pub fn headless(
    scene: &craft_anything_formats::scene_bundle::Scene,
    cancel: &craft_anything_platform::cancel::CancelToken,
) -> Result<(gpu::Gpu, String), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))?
    .map_err(|e| format!("No headless graphics adapter: {e}"))?;
    let name = adapter.get_info().name;
    let (device, queue) =
        wait(adapter.request_device(&wgpu::DeviceDescriptor::default()))?.map_err(|e| format!("Cannot create headless graphics device: {e}"))?;
    let gpu = gpu::Gpu::new(&device, &queue, scene, cancel)?;
    Ok((gpu, name))
}
fn wait<F: std::future::Future>(future: F) -> Result<F::Output, String> {
    use std::{
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };
    let start = Instant::now();
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return Ok(out);
        }
        if start.elapsed() > Duration::from_secs(15) {
            return Err("Graphics initialization timed out".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
