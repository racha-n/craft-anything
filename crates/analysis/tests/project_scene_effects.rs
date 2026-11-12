//! Original synthetic regressions for shared-resource appearance and phase selection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use craft_anything_analysis::{
    project_model::{AppearancePolicy, CostSummary, DateMode, Frame, ProjectModel, ResourcePhase, TimelineSummary},
    project_scene,
    project_simulation::{Phase, PhaseSemantics, PhaseState, Provenance},
    project_visuals::{AppearanceEffect, ProgressBasis},
};
use craft_anything_formats::scene_bundle::{
    Instance, Resource, Scene,
    math::{Bounds, IDENTITY},
};
use serde_json::json;
use std::collections::BTreeMap;

fn shared_scene() -> Scene {
    let bounds = Bounds([[0.; 3], [1.; 3]]);
    Scene {
        mesh_uvs: Default::default(),
        mesh_textures: Default::default(),
        texture_assets: None,
        id: "synthetic-shared-scene".into(),
        source_sha256: "synthetic-source".into(),
        unit_declaration: "unverified".into(),
        axis_declaration: "unverified".into(),
        source_colors: BTreeMap::new(),
        object_colors: BTreeMap::new(),
        resources: ["building", "retained"].into_iter().map(|guid| Resource { guid: guid.into(), name: guid.into(), parent_guid: None }).collect(),
        resource_instance_counts: vec![1, 1],
        meshes: vec![],
        instances: vec![Instance {
            id: "shared-leaf".into(),
            entity_guid: "synthetic-leaf".into(),
            mesh: 0,
            matrix: IDENTITY,
            inverse: Some(IDENTITY),
            bounds,
            resources: vec![0, 1],
        }],
        positions: vec![],
        gpu_positions: vec![],
        gpu_indices: vec![],
        bounds,
        render_origin: [0.; 3],
        triangles: 0,
        node_count: 1,
    }
}
fn effect(direction: [f64; 3], progress: f64) -> AppearanceEffect {
    AppearanceEffect {
        progress,
        progress_basis: ProgressBasis::ElapsedWallclock,
        color: Some([0.2, 0.6, 0.8]),
        opacity: Some(1.),
        growth_direction: Some(direction),
        source_tasks: vec!["synthetic-task".into()],
        limitations: vec![],
    }
}
fn resource(guid: &str, phase: Phase, appearance_effect: Option<AppearanceEffect>) -> ResourcePhase {
    ResourcePhase {
        resource_guid: guid.into(),
        state: PhaseState::Resolved { phase, semantics: PhaseSemantics::InstallUnion },
        provenance: Provenance::CallerDeclared,
        unresolved_reason: None,
        unresolved_profile_effects: vec![],
        appearance_effect,
    }
}
fn frame(phases: Vec<ResourcePhase>) -> Frame {
    Frame {
        schema_version: 1,
        focus: 0,
        date_mode: DateMode::Best,
        appearance_policy: AppearancePolicy::ProfileEffectsPreview,
        resource_phases: phases,
        costs: CostSummary::default(),
        timeline: TimelineSummary::default(),
        unresolved_reasons: vec![],
        native_parity_verified: false,
    }
}
#[test]
fn visible_original_material_coowner_prevents_zero_growth_hiding_and_clipping() {
    let scene = shared_scene();
    let evaluated =
        frame(vec![resource("building", Phase::InstallActive, Some(effect([1., 0., 0.], 0.))), resource("retained", Phase::OriginalMaterial, None)]);
    let appearance = project_scene::derive(&scene, &evaluated, || false).unwrap();
    assert!(!appearance.hidden.contains(&0), "A visible original-material co-owner must not be hidden by another resource's zero growth");
    assert!(
        appearance.styles.get(&0).is_none_or(|style| style.clip_plane.is_none()),
        "Shared retained geometry must not inherit the other resource's growth clip"
    );
}
#[test]
fn nonfinite_and_zero_growth_directions_are_recoverable_errors() {
    let mut scene = shared_scene();
    scene.instances[0].resources = vec![0];
    for direction in [[f64::NAN, 0., 1.], [f64::INFINITY, 0., 0.], [0.; 3]] {
        let evaluated = frame(vec![resource("building", Phase::InstallActive, Some(effect(direction, 0.5)))]);
        assert!(
            project_scene::derive(&scene, &evaluated, || false).is_err(),
            "Invalid growth direction must not become an unclipped colour override: {direction:?}"
        );
    }
}
// Build an admitted storage tail from original values, in grammar order. No native
// file, client identity, retained bytes or implementation test helper is reused.
fn profile_tail(action: u32) -> String {
    fn block() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend([0x10, 0x48, 0x69, 0x23]);
        bytes.extend([0, 1]);
        for channel in [0.2f32, 0.6, 0.8] {
            bytes.extend(channel.to_le_bytes());
        }
        bytes.extend(100u32.to_le_bytes());
        bytes.extend([0, 1, 0]);
        bytes.extend(4u32.to_le_bytes());
        for value in [0f64, 0., 1.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0, 1, 0, 1, 0, 1]);
        bytes.extend([0x6c, 0x4a, 0x92, 0x90]);
        bytes
    }
    let mut bytes = vec![0, 1];
    bytes.extend(action.to_le_bytes());
    bytes.extend(block());
    bytes.extend(block());
    bytes.extend([0x7a, 0x54, 0xaf, 0x39]);
    bytes.extend(0u32.to_le_bytes());
    bytes.extend([0x16, 0xe4, 0x99, 0x6f]);
    bytes.extend([0u8; 32]);
    bytes.extend(block());
    bytes.extend([0x3d, 0xc8, 0xb0, 0x03]);
    bytes.extend(0u32.to_le_bytes());
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
#[test]
fn active_maintain_uses_its_own_progress_instead_of_install_union_progress() {
    let unset = 32_535_244_799i64;
    let task = |guid: &str, start: i64, finish: i64, profile: &str| {
        json!({
            "guid":guid,"name":guid,"planned_start_wallclock_scalar":start,"duration_storage_scalar":finish-start,
            "status_storage_value":2,"date_storage_windows":[start,unset,finish,unset],
            "assignments":[{"resource_guid":"component","profile_guid":profile,"planned_units":1.}]
        })
    };
    let metadata = json!({"schema_version":4,
        "tasks":[task("install-task",0,100,"install-profile"), task("maintain-task",50,80,"maintain-profile")],
        "resources":[{"guid":"component","name":"Original fixture component","kind_storage_byte":4}],
        "definition_storage":{
            "install-profile":{"section":"Use Profiles","name":"Synthetic Install","raw_tail_hex":profile_tail(1)},
            "maintain-profile":{"section":"Use Profiles","name":"Synthetic Maintain","raw_tail_hex":profile_tail(4)}
        }
    });
    let model = ProjectModel::from_metadata(&metadata).unwrap();
    let frame = model.evaluate_with_policy(60, DateMode::Best, AppearancePolicy::ProfileEffectsPreview, || false).unwrap();
    let resource = frame.resource_phases.iter().find(|phase| phase.resource_guid == "component").unwrap();
    assert!(matches!(resource.state, PhaseState::Resolved { phase: Phase::MaintainActive, .. }));
    let effect = resource.appearance_effect.as_ref().expect("The selected Maintain phase must retain its supported synthetic effect");
    assert!((effect.progress - 1. / 3.).abs() < 1e-12, "Maintain progress should be10/30, not Install progress60/100: {}", effect.progress);
    assert_eq!(effect.source_tasks, ["maintain-task"]);
}
