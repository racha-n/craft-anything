struct View {
    projection: mat4x4<f32>,
    material: vec4<f32>,
    accent: vec4<f32>,
    selected: vec4<u32>,
}
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(1) var source_texture: texture_2d<f32>;
@group(1) @binding(2) var source_sampler: sampler;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) a: vec4<f32>,
    @location(2) b: vec4<f32>,
    @location(3) c: vec4<f32>,
    @location(4) d: vec4<f32>,
    @location(5) id: u32,
    @location(6) visible: u32,
    @location(7) color: u32,
    @location(8) opacity:f32,
    @location(9) clip_plane:vec4<f32>,
    @location(10) uv:vec2<f32>,
}
struct Fragment {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) @interpolate(flat) id: u32,
    @location(2) @interpolate(flat) color: u32,
    @location(3) @interpolate(flat) opacity:f32,
    @location(4) @interpolate(flat) clip_plane:vec4<f32>,
    @location(5) uv:vec2<f32>,
    @location(6) @interpolate(flat) flags:u32,
}
@vertex fn vertex(v: Vertex) -> Fragment {
    var out: Fragment;
    let world = mat4x4<f32>(v.a,v.b,v.c,v.d) * vec4<f32>(v.position,1.0);
    out.position = world.xyz;
    out.clip = view.projection * world;
    if (v.visible & 1u) == 0u { out.clip = vec4<f32>(2.0,2.0,2.0,1.0); }
    out.id = v.id;
    out.color = v.color;
    out.opacity=v.opacity;
    out.clip_plane=v.clip_plane;
    out.uv=v.uv;
    if (v.visible & 4u) != 0u {out.uv.y=1.0-v.uv.y;}
    out.flags=v.visible;
    return out;
}
@fragment fn fragment(v: Fragment) -> @location(0) vec4<f32> {
    // Sample uniformly before fragment-dependent discard or selection branches.
    let texel = textureSample(source_texture,source_sampler,v.uv);
    let raw = cross(dpdx(v.position),dpdy(v.position));
    let n = raw / max(length(raw),0.0000001);
    let light = 0.35 + 0.65 * abs(dot(n, normalize(vec3<f32>(0.4,-0.6,0.8))));
    if dot(v.clip_plane,vec4<f32>(v.position,1.0))>0.0 || v.opacity<=0.0 {discard;}
    var color = view.material;
    if (v.color >> 24u) != 0u { color=vec4<f32>(f32(v.color & 255u),f32((v.color>>8u)&255u),f32((v.color>>16u)&255u),255.0)/255.0; }
    if (v.flags & 2u) != 0u {color=texel;}
    if v.id == view.selected.x { color = view.accent; }
    return vec4<f32>(color.rgb * light,v.opacity);
}
