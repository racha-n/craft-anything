//! Bounded world-coordinate surface measurements, not certified solid quantities.
//! Exact-position welding tests edge incidence, winding and connected vertex fans.
//! Self-intersections, overlapping components and material/unit semantics are not tested.
use crate::scene_bundle::{Scene, math};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

pub const DEFAULT_TRIANGLE_BUDGET: usize = 2_000_000;
pub const MAX_TRIANGLE_BUDGET: usize = 50_000_000;
const TOPOLOGY_LIMIT: usize = 2_000_000;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    Budget,
    Cancelled,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "{message}"),
            Self::Budget => write!(
                f,
                "Quantity work or topology memory budget exceeded; select a smaller page or simpler instances. The fixed topology cap is independent of the triangle budget."
            ),
            Self::Cancelled => write!(f, "Quantity measurement cancelled."),
        }
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or(Error::Budget)
}
fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b).ok_or(Error::Budget)
}
#[derive(Debug, Serialize)]
pub struct Quantity {
    pub instance_index: usize,
    pub instance_id: String,
    pub entity_guid: String,
    pub triangles: usize,
    pub welded_vertices: usize,
    pub surface_area_stored_world_units_squared: f64,
    pub closed_edges: bool,
    pub consistent_edge_orientation: bool,
    pub connected_vertex_fans: bool,
    pub signed_tetrahedral_volume_stored_world_units_cubed: Option<f64>,
    pub absolute_algebraic_volume_stored_world_units_cubed: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct Page {
    pub schema_version: u32,
    pub offset: usize,
    pub limit: usize,
    pub total_selected_instances: usize,
    pub more: bool,
    pub expanded_triangles_tested: usize,
    pub complete_requested_page: bool,
    pub coordinate_policy: &'static str,
    pub units_verified: bool,
    pub self_intersections_checked: bool,
    pub topology_entry_cap_per_instance: usize,
    pub items: Vec<Quantity>,
}
#[derive(Clone, Copy)]
struct Edge {
    count: usize,
    balance: i64,
    corners: [usize; 2],
}
fn root(parent: &mut [usize], mut at: usize) -> Result<usize> {
    let mut steps = 0usize;
    loop {
        let next = *parent.get(at).ok_or_else(|| invalid("Invalid topology corner"))?;
        if next == at {
            return Ok(at);
        }
        let after = *parent.get(next).ok_or_else(|| invalid("Invalid topology parent"))?;
        *parent.get_mut(at).ok_or_else(|| invalid("Invalid topology corner"))? = after;
        at = next;
        steps = add(steps, 1)?;
        if steps > parent.len() {
            return Err(invalid("Topology cycle"));
        }
    }
}
fn join(parent: &mut [usize], a: usize, b: usize) -> Result<()> {
    let a = root(parent, a)?;
    let b = root(parent, b)?;
    *parent.get_mut(a).ok_or_else(|| invalid("Invalid topology union"))? = b;
    Ok(())
}
fn reserve_map<K: std::hash::Hash + Eq, V>(map: &mut HashMap<K, V>) -> Result<()> {
    if map.len() == map.capacity() {
        map.try_reserve(1024).map_err(|_| Error::Budget)?;
    }
    Ok(())
}
fn sum(total: &mut f64, correction: &mut f64, value: f64) -> Result<()> {
    let adjusted = value - *correction;
    let next = *total + adjusted;
    *correction = (next - *total) - adjusted;
    *total = next;
    if !value.is_finite() || !total.is_finite() || !correction.is_finite() {
        return Err(invalid("Measurement arithmetic is non-finite or overflows"));
    }
    Ok(())
}
fn point(scene: &Scene, mesh: &crate::scene_bundle::Mesh, triangle: usize, corner: usize) -> Result<math::Point> {
    let index = add(mesh.index_start, add(mul(triangle, 3)?, corner)?)?;
    let at = mul(index, 4)?;
    let end = add(at, 4)?;
    let raw: [u8; 4] =
        scene.gpu_indices.get(at..end).ok_or_else(|| invalid("Truncated triangle index storage"))?.try_into().map_err(|_| invalid("Truncated index"))?;
    let ix = usize::try_from(u32::from_le_bytes(raw)).map_err(|_| Error::Budget)?;
    if ix >= mesh.vertex_count {
        return Err(invalid("Triangle index is outside its mesh"));
    }
    let p = *scene.positions.get(add(mesh.vertex_start, ix)?).ok_or_else(|| invalid("Truncated position storage"))?;
    if !math::finite(p) {
        return Err(invalid("Non-finite source position"));
    }
    Ok(p)
}
fn instance(scene: &Scene, index: usize, mut keep_going: impl FnMut() -> bool) -> Result<Quantity> {
    let instance = scene.instances.get(index).ok_or_else(|| invalid("Instance index is outside the loaded scene"))?;
    let mesh = scene.meshes.get(instance.mesh).ok_or_else(|| invalid("Instance mesh is absent"))?;
    if !math::affine(&instance.matrix) {
        return Err(invalid("Instance transform is not finite affine"));
    }
    let m = &instance.matrix;
    let determinant = math::dot([m[0], m[1], m[2]], math::cross([m[4], m[5], m[6]], [m[8], m[9], m[10]]));
    if !determinant.is_finite() || determinant == 0. {
        return Err(invalid("Singular or overflowing world transform"));
    }
    if mesh.triangle_count == 0 {
        return Err(invalid("Instance has no triangles"));
    }
    let origin = point(scene, mesh, 0, 0)?;
    let mut vertices = HashMap::<[u64; 3], usize>::new();
    let mut representatives = Vec::<usize>::new();
    let mut corner_vertices = Vec::<usize>::new();
    let mut parent = Vec::<usize>::new();
    let mut edges = HashMap::<(usize, usize), Edge>::new();
    let (mut area, mut area_c, mut volume, mut volume_c) = (0., 0., 0., 0.);
    for triangle in 0..mesh.triangle_count {
        if triangle.is_multiple_of(256) && !keep_going() {
            return Err(Error::Cancelled);
        }
        if add(add(vertices.len(), edges.len())?, add(parent.len(), 12)?)? > TOPOLOGY_LIMIT {
            return Err(Error::Budget);
        }
        parent.try_reserve(3).map_err(|_| Error::Budget)?;
        corner_vertices.try_reserve(3).map_err(|_| Error::Budget)?;
        let base = parent.len();
        let mut ids = [0usize; 3];
        let mut positions = [[0.; 3]; 3];
        for (corner, p) in positions.iter_mut().enumerate() {
            // Translation is removed before the linear transform to avoid large-origin cancellation.
            let source = point(scene, mesh, triangle, corner)?;
            if !math::finite(math::transform(&instance.matrix, source)) {
                return Err(invalid("World position overflows"));
            }
            *p = math::vector(&instance.matrix, math::sub(source, origin));
            if !math::finite(*p) {
                return Err(invalid("World position arithmetic overflows"));
            }
            let key = p.map(|value| if value == 0. { 0 } else { value.to_bits() });
            let id = if let Some(id) = vertices.get(&key) {
                *id
            } else {
                reserve_map(&mut vertices)?;
                representatives.try_reserve(1).map_err(|_| Error::Budget)?;
                let id = representatives.len();
                representatives.push(add(base, corner)?);
                vertices.insert(key, id);
                id
            };
            *ids.get_mut(corner).ok_or_else(|| invalid("Triangle corner"))? = id;
            let at = parent.len();
            parent.push(at);
            corner_vertices.push(id);
        }
        let [a, b, c] = positions;
        let normal = math::cross(math::sub(b, a), math::sub(c, a));
        let twice_area = normal[0].hypot(normal[1]).hypot(normal[2]);
        if !twice_area.is_finite() || twice_area <= 0. {
            return Err(invalid("Degenerate or overflowing triangle"));
        }
        sum(&mut area, &mut area_c, twice_area * 0.5)?;
        sum(&mut volume, &mut volume_c, math::dot(a, math::cross(b, c)) / 6.)?;
        for (from, to) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let a = *ids.get(from).ok_or_else(|| invalid("Edge corner"))?;
            let b = *ids.get(to).ok_or_else(|| invalid("Edge corner"))?;
            if a == b {
                return Err(invalid("Degenerate welded edge"));
            }
            let (key, sign, corners) = if a < b { ((a, b), 1, [add(base, from)?, add(base, to)?]) } else { ((b, a), -1, [add(base, to)?, add(base, from)?]) };
            if let Some(edge) = edges.get_mut(&key) {
                edge.count = add(edge.count, 1)?;
                edge.balance = edge.balance.checked_add(sign).ok_or(Error::Budget)?;
                for (previous, current) in edge.corners.into_iter().zip(corners) {
                    join(&mut parent, previous, current)?;
                }
            } else {
                reserve_map(&mut edges)?;
                edges.insert(key, Edge { count: 1, balance: sign, corners });
            }
        }
    }
    let closed = edges.values().all(|edge| edge.count == 2);
    let consistent = edges.values().all(|edge| edge.count == 2 && edge.balance == 0);
    let mut connected = true;
    for (corner, vertex) in corner_vertices.into_iter().enumerate() {
        if corner.is_multiple_of(1024) && !keep_going() {
            return Err(Error::Cancelled);
        }
        let representative = *representatives.get(vertex).ok_or_else(|| invalid("Missing vertex representative"))?;
        if root(&mut parent, corner)? != root(&mut parent, representative)? {
            connected = false;
        }
    }
    let signed = (closed && consistent && connected).then_some(volume);
    Ok(Quantity {
        instance_index: index,
        instance_id: instance.id.clone(),
        entity_guid: instance.entity_guid.clone(),
        triangles: mesh.triangle_count,
        welded_vertices: vertices.len(),
        surface_area_stored_world_units_squared: area,
        closed_edges: closed,
        consistent_edge_orientation: consistent,
        connected_vertex_fans: connected,
        signed_tetrahedral_volume_stored_world_units_cubed: signed,
        absolute_algebraic_volume_stored_world_units_cubed: signed.map(f64::abs),
    })
}
/// Measure only the selected page. No sum, solid-volume or source-unit claim is made.
/// A work/memory-budget error discards the page; it never returns a complete aggregate.
pub fn measure(
    scene: &Scene,
    instance_indices: Option<&[usize]>,
    offset: usize,
    limit: usize,
    triangle_budget: usize,
    mut keep_going: impl FnMut() -> bool,
) -> Result<Page> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("Page limit must be between1 and1000"));
    }
    if !(1..=MAX_TRIANGLE_BUDGET).contains(&triangle_budget) {
        return Err(invalid("Triangle budget must be between1 and50000000"));
    }
    if !keep_going() {
        return Err(Error::Cancelled);
    }
    let selected = if let Some(indices) = instance_indices {
        if indices.len() > 10_000 {
            return Err(invalid("At most10000 instance filters are accepted"));
        }
        let mut unique = BTreeSet::new();
        for &index in indices {
            if index >= scene.instances.len() {
                return Err(invalid("Instance filter is outside the scene"));
            }
            unique.insert(index);
        }
        Some(unique.into_iter().collect::<Vec<_>>())
    } else {
        None
    };
    let total = selected.as_ref().map_or(scene.instances.len(), Vec::len);
    let end = offset.saturating_add(limit).min(total);
    let mut items = Vec::new();
    items.try_reserve(end.saturating_sub(offset)).map_err(|_| Error::Budget)?;
    let mut tested = 0usize;
    for ordinal in offset..end {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        let index = selected.as_ref().and_then(|v| v.get(ordinal)).copied().unwrap_or(ordinal);
        let instance = scene.instances.get(index).ok_or_else(|| invalid("Missing scene instance"))?;
        let mesh = scene.meshes.get(instance.mesh).ok_or_else(|| invalid("Missing scene mesh"))?;
        tested = add(tested, mesh.triangle_count)?;
        if tested > triangle_budget {
            return Err(Error::Budget);
        }
        items.push(self::instance(scene, index, &mut keep_going)?);
    }
    Ok(Page {
        schema_version: 1,
        offset,
        limit,
        total_selected_instances: total,
        more: end < total,
        expanded_triangles_tested: tested,
        complete_requested_page: true,
        coordinate_policy: "instance_world_linear_transform_with_translation_rebased",
        units_verified: false,
        self_intersections_checked: false,
        topology_entry_cap_per_instance: TOPOLOGY_LIMIT,
        items,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::scene_bundle::{
        Instance, Mesh,
        math::{Bounds, IDENTITY},
    };
    use std::collections::BTreeMap;
    fn scene(points: Vec<math::Point>, indices: &[u32]) -> Scene {
        let bounds = Bounds([[0.; 3], [2.; 3]]);
        let triangles = indices.len() / 3;
        Scene {
            mesh_uvs: Default::default(),
            mesh_textures: Default::default(),
            texture_assets: None,
            id: "fixture".into(),
            source_sha256: "original-fixture".into(),
            unit_declaration: "unknown".into(),
            axis_declaration: "unknown".into(),
            source_colors: BTreeMap::new(),
            object_colors: BTreeMap::new(),
            resources: vec![],
            resource_instance_counts: vec![],
            meshes: vec![Mesh { vertex_start: 0, index_start: 0, vertex_count: points.len(), triangle_count: triangles, origin: [0.; 3], bounds }],
            instances: vec![Instance {
                id: "one".into(),
                entity_guid: "leaf".into(),
                mesh: 0,
                matrix: IDENTITY,
                inverse: Some(IDENTITY),
                bounds,
                resources: vec![],
            }],
            positions: points,
            gpu_positions: vec![],
            gpu_indices: indices.iter().flat_map(|i| i.to_le_bytes()).collect(),
            bounds,
            render_origin: [0.; 3],
            triangles,
            node_count: 1,
        }
    }
    fn tetra() -> Scene {
        scene(vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.], [0., 0., 2.]], &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3])
    }
    fn one(scene: &Scene) -> Quantity {
        measure(scene, None, 0, 10, DEFAULT_TRIANGLE_BUDGET, || true).unwrap().items.remove(0)
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-10, "{a} != {b}");
    }
    #[test]
    fn tetra_nonuniform_reflection_and_large_translation() {
        let mut scene = tetra();
        let original = one(&scene);
        close(original.signed_tetrahedral_volume_stored_world_units_cubed.unwrap(), 4. / 3.);
        close(original.surface_area_stored_world_units_squared, 6. + 3f64.sqrt() * 2.);
        scene.instances[0].matrix[0] = -2.;
        scene.instances[0].matrix[5] = 3.;
        scene.instances[0].matrix[10] = 4.;
        scene.instances[0].matrix[12] = 1e16;
        scene.instances[0].matrix[13] = -1e16;
        close(one(&scene).signed_tetrahedral_volume_stored_world_units_cubed.unwrap(), -32.);
    }
    #[test]
    fn closed_box_under_nonuniform_transform() {
        let mut scene = scene(
            vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
            &[0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5],
        );
        scene.instances[0].matrix[0] = 2.;
        scene.instances[0].matrix[5] = 3.;
        scene.instances[0].matrix[10] = 4.;
        let row = one(&scene);
        close(row.surface_area_stored_world_units_squared, 52.);
        close(row.absolute_algebraic_volume_stored_world_units_cubed.unwrap(), 24.);
        assert!(row.closed_edges && row.consistent_edge_orientation && row.connected_vertex_fans);
    }
    #[test]
    fn open_and_inconsistent_shells_have_no_volume() {
        let mut scene = tetra();
        scene.meshes[0].triangle_count = 3;
        let row = one(&scene);
        assert!(!row.closed_edges);
        assert!(row.signed_tetrahedral_volume_stored_world_units_cubed.is_none());
        let mut scene = tetra();
        scene.gpu_indices[..12].copy_from_slice(&[0u32, 1, 2].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>());
        let row = one(&scene);
        assert!(row.closed_edges);
        assert!(!row.consistent_edge_orientation);
        assert!(row.absolute_algebraic_volume_stored_world_units_cubed.is_none());
    }
    #[test]
    fn exact_welding_canonicalizes_negative_zero() {
        let mut scene = tetra();
        scene.positions.push([-0., 0., 0.]);
        scene.meshes[0].vertex_count = 5;
        scene.gpu_indices[..4].copy_from_slice(&4u32.to_le_bytes());
        let row = one(&scene);
        assert_eq!(row.welded_vertices, 4);
        assert!(row.connected_vertex_fans);
        assert!(row.closed_edges);
    }
    #[test]
    fn disconnected_vertex_fans_are_rejected_for_volume() {
        let scene = scene(
            vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.], [0., 0., 2.], [-2., 0., 0.], [0., -2., 0.], [0., 0., -2.]],
            &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3, 0, 4, 5, 0, 6, 4, 0, 5, 6, 4, 6, 5],
        );
        let row = one(&scene);
        assert!(row.closed_edges && row.consistent_edge_orientation);
        assert!(!row.connected_vertex_fans);
        assert!(row.absolute_algebraic_volume_stored_world_units_cubed.is_none());
    }
    #[test]
    fn hostile_ranges_numbers_degenerate_budget_and_cancel() {
        let mut scene = tetra();
        scene.gpu_indices.pop();
        assert!(measure(&scene, None, 0, 1, 100, || true).is_err());
        let mut scene = tetra();
        scene.positions.pop();
        assert!(measure(&scene, None, 0, 1, 100, || true).is_err());
        let mut scene = tetra();
        scene.positions[0][0] = f64::NAN;
        assert!(measure(&scene, None, 0, 1, 100, || true).is_err());
        let mut scene = tetra();
        scene.positions[1] = scene.positions[0];
        assert!(measure(&scene, None, 0, 1, 100, || true).is_err());
        let mut scene = tetra();
        scene.instances[0].matrix[0] = f64::MAX;
        assert!(measure(&scene, None, 0, 1, 100, || true).is_err());
        let scene = tetra();
        assert_eq!(measure(&scene, None, 0, 1, 3, || true).unwrap_err(), Error::Budget);
        assert_eq!(measure(&scene, None, 0, 1, 100, || false).unwrap_err(), Error::Cancelled);
        let mut calls = 0;
        assert_eq!(
            measure(&scene, None, 0, 1, 100, || {
                calls += 1;
                calls < 4
            })
            .unwrap_err(),
            Error::Cancelled
        );
        assert!(measure(&scene, Some(&[9]), 0, 1, 100, || true).is_err());
        assert!(measure(&scene, None, 0, 1001, 100, || true).is_err());
        let page = measure(&scene, Some(&[0, 0]), 999, 1, 100, || true).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total_selected_instances, 1);
        assert!(!page.more);
    }
}
