//! Original synthetic half-space selection regression; no native file is required.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use craft_anything_formats::scene_bundle::{
    Instance, Mesh, Scene, Visibility,
    math::{Bounds, IDENTITY},
};
use std::collections::BTreeMap;
fn positive_x_surface() -> Scene {
    let x = 1e-10;
    let bounds = Bounds([[x, -1., -1.], [x, 1., 1.]]);
    Scene {
        mesh_uvs: Default::default(),
        mesh_textures: Default::default(),
        texture_assets: None,
        id: "tiny-positive-x".into(),
        source_sha256: "original-synthetic".into(),
        unit_declaration: "unverified".into(),
        axis_declaration: "unverified".into(),
        source_colors: BTreeMap::new(),
        object_colors: BTreeMap::new(),
        resources: vec![],
        resource_instance_counts: vec![],
        meshes: vec![Mesh { vertex_start: 0, index_start: 0, vertex_count: 3, triangle_count: 1, origin: [x, 0., 0.], bounds }],
        instances: vec![Instance {
            id: "surface".into(),
            entity_guid: "synthetic-leaf".into(),
            mesh: 0,
            matrix: IDENTITY,
            inverse: Some(IDENTITY),
            bounds,
            resources: vec![],
        }],
        positions: vec![[x, -1., -1.], [x, 1., -1.], [x, 0., 1.]],
        gpu_positions: vec![],
        gpu_indices: [0u32, 1, 2].into_iter().flat_map(u32::to_le_bytes).collect(),
        bounds,
        render_origin: [0.; 3],
        triangles: 1,
        node_count: 1,
    }
}
#[test]
fn equivalent_subnormal_half_space_does_not_select_a_clipped_surface() {
    let scene = positive_x_surface();
    let filters = Visibility::default();
    let uncut = scene.pick([1., 0., 0.], [-1., 0., 0.], &filters, || true).unwrap();
    assert_eq!(uncut.instance, Some(0), "The synthetic triangle must first be selectable without clipping");
    for scale in [1., 1e-200, 1e-320] {
        let clipped = scene.pick_clipped([1., 0., 0.], [-1., 0., 0.], &filters, &BTreeMap::from([(0, [scale, 0., 0., 0.])]), || true).unwrap();
        assert!(clipped.complete);
        assert_eq!(clipped.instance, None, "A positive scalar change must preserve x<=0 half-space selection, including subnormal scale{scale}");
    }
}
