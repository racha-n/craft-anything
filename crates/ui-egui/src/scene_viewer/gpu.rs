//! UI adapter over the shared headless renderer.
pub use craft_anything_engines::scene_render::gpu::*;
impl From<&crate::theme::Tokens> for craft_anything_engines::scene_render::Palette {
    fn from(t: &crate::theme::Tokens) -> Self {
        Self {
            canvas: t.canvas.to_normalized_gamma_f32(),
            text_dim: t.text_dim.to_normalized_gamma_f32(),
            accent: t.accent.to_normalized_gamma_f32(),
            warning: t.warning.to_normalized_gamma_f32(),
            success: t.success.to_normalized_gamma_f32(),
        }
    }
}
