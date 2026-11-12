//! Bounded, opt-in source image candidates. Channel order and sampling are not native-verified.
use craft_anything_formats::{project_textures::CachedImage, scene_bundle::Scene};
use craft_anything_platform::cancel::CancelToken;
use std::collections::BTreeMap;
use wgpu as w;
#[path = "texture_decode.rs"]
mod decoding;
use decoding::{MAX_DECODED_BYTES, decode_parts, rgba_size};
fn decode(image: &CachedImage) -> Result<Vec<u8>, String> {
    decode_parts(image.width, image.height, &image.codec, &image.data)
}
pub(super) struct TextureSet {
    pub layout: w::BindGroupLayout,
    binds: Vec<w::BindGroup>,
    meshes: BTreeMap<usize, usize>,
}
impl TextureSet {
    pub fn bind(&self, mesh: usize) -> Option<&w::BindGroup> {
        self.binds.get(self.meshes.get(&mesh).copied().unwrap_or(0))
    }
    pub fn new(device: &w::Device, queue: &w::Queue, scene: &Scene, cancel: &CancelToken) -> Result<Self, String> {
        let layout = device.create_bind_group_layout(&w::BindGroupLayoutDescriptor {
            label: Some("Source texture candidates"),
            entries: &[
                w::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: w::ShaderStages::FRAGMENT,
                    ty: w::BindingType::Texture {
                        sample_type: w::TextureSampleType::Float { filterable: true },
                        view_dimension: w::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                w::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: w::ShaderStages::FRAGMENT,
                    ty: w::BindingType::Sampler(w::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&w::SamplerDescriptor {
            label: Some("Experimental repeat UV"),
            address_mode_u: w::AddressMode::Repeat,
            address_mode_v: w::AddressMode::Repeat,
            mag_filter: w::FilterMode::Linear,
            min_filter: w::FilterMode::Linear,
            ..Default::default()
        });
        let upload = |width, height, data: &[u8]| -> Result<w::BindGroup, String> {
            if width > device.limits().max_texture_dimension_2d || height > device.limits().max_texture_dimension_2d {
                return Err("Texture exceeds adapter dimensions".into());
            }
            let texture = device.create_texture(&w::TextureDescriptor {
                label: Some("Source image candidate"),
                size: w::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: w::TextureDimension::D2,
                format: w::TextureFormat::Rgba8Unorm,
                usage: w::TextureUsages::TEXTURE_BINDING | w::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                w::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: w::Origin3d::ZERO, aspect: w::TextureAspect::All },
                data,
                w::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
                w::Extent3d { width, height, depth_or_array_layers: 1 },
            );
            let view = texture.create_view(&Default::default());
            Ok(device.create_bind_group(&w::BindGroupDescriptor {
                label: Some("Source texture candidate"),
                layout: &layout,
                entries: &[
                    w::BindGroupEntry { binding: 1, resource: w::BindingResource::TextureView(&view) },
                    w::BindGroupEntry { binding: 2, resource: w::BindingResource::Sampler(&sampler) },
                ],
            }))
        };
        let mut binds = vec![upload(1, 1, &[255; 4])?];
        let mut meshes = BTreeMap::new();
        let mut images = BTreeMap::new();
        let mut total = 0usize;
        if let Some(assets) = &scene.texture_assets {
            for (mesh, guid) in &scene.mesh_textures {
                if cancel.is_cancelled() {
                    return Err("Texture upload cancelled".into());
                }
                if !scene.mesh_uvs.contains_key(mesh) {
                    continue;
                }
                let image = assets.image_for_texture(guid).ok_or("Texture image reference is missing")?;
                let index = if let Some(index) = images.get(&image.guid) {
                    *index
                } else {
                    total = total.checked_add(rgba_size(image.width, image.height)?).ok_or("Texture budget overflow")?;
                    if total > MAX_DECODED_BYTES {
                        return Err("Textures exceed 64 MiB decoded preview budget".into());
                    }
                    let rgba = decode(image)?;
                    if cancel.is_cancelled() {
                        return Err("Texture decode cancelled".into());
                    }
                    let index = binds.len();
                    binds.push(upload(image.width, image.height, &rgba)?);
                    images.insert(image.guid.clone(), index);
                    index
                };
                meshes.insert(*mesh, index);
            }
        }
        Ok(Self { layout, binds, meshes })
    }
}
