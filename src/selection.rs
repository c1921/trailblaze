use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    building::{Blueprint, BuildState, CompletedBuilding},
    colonist::Colonist,
    types::{BuildingKind, CELL_SIZE},
    world::{Ground, ResourceNode},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectedTarget {
    Blueprint(Entity),
    Building(Entity),
    Colonist(Entity),
    Resource(Entity),
}

impl SelectedTarget {
    pub fn entity(self) -> Entity {
        match self {
            Self::Blueprint(entity)
            | Self::Building(entity)
            | Self::Colonist(entity)
            | Self::Resource(entity) => entity,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct SelectionState {
    pub selected: Option<SelectedTarget>,
}

#[derive(Clone, Copy, Debug)]
pub struct HitCandidate {
    pub target: SelectedTarget,
    pub distance: f32,
    pub priority: u8,
}

pub fn select_target(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ground_query: Query<&GlobalTransform, With<Ground>>,
    button_interactions: Query<&Interaction, With<Button>>,
    build_state: Res<BuildState>,
    mut selection: ResMut<SelectionState>,
    resource_nodes: Query<(Entity, &ResourceNode, &Transform)>,
    colonists: Query<(Entity, &Colonist, &Transform)>,
    blueprints: Query<(Entity, &Blueprint, &Transform)>,
    buildings: Query<(Entity, &CompletedBuilding, &Transform), Without<Blueprint>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        selection.selected = None;
        return;
    }

    if build_state.selected.is_some() || !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    if button_interactions
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }

    let Some(cursor_world) = cursor_ground_position(&windows, &camera_query, &ground_query) else {
        return;
    };

    let mut candidates = Vec::new();
    collect_colonist_hits(cursor_world, &colonists, &mut candidates);
    collect_blueprint_hits(cursor_world, &blueprints, &mut candidates);
    collect_building_hits(cursor_world, &buildings, &mut candidates);
    collect_resource_hits(cursor_world, &resource_nodes, &mut candidates);

    selection.selected = best_hit(&candidates).map(|candidate| candidate.target);
}

pub fn draw_selection_highlight(
    selection: Res<SelectionState>,
    mut gizmos: Gizmos,
    resource_nodes: Query<(Entity, &Transform), With<ResourceNode>>,
    colonists: Query<(Entity, &Transform), With<Colonist>>,
    blueprints: Query<(Entity, &Blueprint, &Transform)>,
    buildings: Query<(Entity, &CompletedBuilding, &Transform), Without<Blueprint>>,
) {
    let Some(selected) = selection.selected else {
        return;
    };

    let Some((position, radius)) = selected_position_and_radius(
        selected,
        &resource_nodes,
        &colonists,
        &blueprints,
        &buildings,
    ) else {
        return;
    };

    gizmos.circle(
        Isometry3d::new(
            Vec3::new(position.x, 0.08, position.z),
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        ),
        radius,
        LinearRgba::rgb(1.0, 0.88, 0.18),
    );
    gizmos.cube(
        Transform::from_translation(Vec3::new(position.x, position.y + 0.08, position.z))
            .with_scale(Vec3::splat(radius * 1.6)),
        LinearRgba::rgb(1.0, 0.88, 0.18),
    );
}

pub fn best_hit(candidates: &[HitCandidate]) -> Option<HitCandidate> {
    candidates.iter().copied().min_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.distance.total_cmp(&right.distance))
            .then_with(|| left.target.entity().cmp(&right.target.entity()))
    })
}

fn collect_colonist_hits(
    cursor_world: Vec3,
    colonists: &Query<(Entity, &Colonist, &Transform)>,
    candidates: &mut Vec<HitCandidate>,
) {
    for (entity, _, transform) in colonists {
        let distance = xz_distance(cursor_world, transform.translation);
        if distance <= 0.7 {
            candidates.push(HitCandidate {
                target: SelectedTarget::Colonist(entity),
                distance,
                priority: 0,
            });
        }
    }
}

fn collect_blueprint_hits(
    cursor_world: Vec3,
    blueprints: &Query<(Entity, &Blueprint, &Transform)>,
    candidates: &mut Vec<HitCandidate>,
) {
    for (entity, blueprint, transform) in blueprints {
        if point_in_building_box(cursor_world, transform, blueprint.kind, 0.25) {
            candidates.push(HitCandidate {
                target: SelectedTarget::Blueprint(entity),
                distance: xz_distance(cursor_world, transform.translation),
                priority: 1,
            });
        }
    }
}

fn collect_building_hits(
    cursor_world: Vec3,
    buildings: &Query<(Entity, &CompletedBuilding, &Transform), Without<Blueprint>>,
    candidates: &mut Vec<HitCandidate>,
) {
    for (entity, building, transform) in buildings {
        if point_in_building_box(cursor_world, transform, building.kind, 0.25) {
            candidates.push(HitCandidate {
                target: SelectedTarget::Building(entity),
                distance: xz_distance(cursor_world, transform.translation),
                priority: 2,
            });
        }
    }
}

fn collect_resource_hits(
    cursor_world: Vec3,
    resource_nodes: &Query<(Entity, &ResourceNode, &Transform)>,
    candidates: &mut Vec<HitCandidate>,
) {
    for (entity, _, transform) in resource_nodes {
        let distance = xz_distance(cursor_world, transform.translation);
        if distance <= transform.scale.x.max(transform.scale.z) * 0.75 + 0.3 {
            candidates.push(HitCandidate {
                target: SelectedTarget::Resource(entity),
                distance,
                priority: 3,
            });
        }
    }
}

fn selected_position_and_radius(
    selected: SelectedTarget,
    resource_nodes: &Query<(Entity, &Transform), With<ResourceNode>>,
    colonists: &Query<(Entity, &Transform), With<Colonist>>,
    blueprints: &Query<(Entity, &Blueprint, &Transform)>,
    buildings: &Query<(Entity, &CompletedBuilding, &Transform), Without<Blueprint>>,
) -> Option<(Vec3, f32)> {
    match selected {
        SelectedTarget::Resource(entity) => resource_nodes
            .get(entity)
            .ok()
            .map(|(_, transform)| (transform.translation, 0.75)),
        SelectedTarget::Colonist(entity) => colonists
            .get(entity)
            .ok()
            .map(|(_, transform)| (transform.translation, 0.45)),
        SelectedTarget::Blueprint(entity) => {
            blueprints
                .get(entity)
                .ok()
                .map(|(_, blueprint, transform)| {
                    let size = building_visual_size(blueprint.kind);
                    (transform.translation, size.x.max(size.y) * 0.65)
                })
        }
        SelectedTarget::Building(entity) => {
            buildings.get(entity).ok().map(|(_, building, transform)| {
                let size = building_visual_size(building.kind);
                (transform.translation, size.x.max(size.y) * 0.65)
            })
        }
    }
}

fn cursor_ground_position(
    windows: &Query<&Window, With<PrimaryWindow>>,
    camera_query: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ground_query: &Query<&GlobalTransform, With<Ground>>,
) -> Option<Vec3> {
    let window = windows.single().ok()?;
    let cursor_position = window.cursor_position()?;
    let (camera, camera_transform) = camera_query.single().ok()?;
    let ground = ground_query.single().ok()?;
    let ray = camera
        .viewport_to_world(camera_transform, cursor_position)
        .ok()?;

    ray.plane_intersection_point(ground.translation(), InfinitePlane3d::new(ground.up()))
}

fn point_in_building_box(
    point: Vec3,
    transform: &Transform,
    kind: BuildingKind,
    padding: f32,
) -> bool {
    point_in_rotated_box(point, transform, building_visual_size(kind), padding)
}

fn point_in_rotated_box(point: Vec3, transform: &Transform, size: Vec2, padding: f32) -> bool {
    let offset = Vec3::new(
        point.x - transform.translation.x,
        0.0,
        point.z - transform.translation.z,
    );
    let local = transform.rotation.inverse() * offset;
    let half_x = size.x.abs() * 0.5 + padding;
    let half_z = size.y.abs() * 0.5 + padding;

    local.x.abs() <= half_x && local.z.abs() <= half_z
}

fn building_visual_size(kind: BuildingKind) -> Vec2 {
    let definition = kind.definition();

    if kind == BuildingKind::Road {
        Vec2::splat(CELL_SIZE * 0.95)
    } else {
        Vec2::new(
            definition.size.x as f32 * CELL_SIZE * 0.9,
            definition.size.y as f32 * CELL_SIZE * 0.9,
        )
    }
}

fn xz_distance(left: Vec3, right: Vec3) -> f32 {
    Vec2::new(left.x - right.x, left.z - right.z).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn best_hit_prefers_priority_before_distance() {
        let close_resource = HitCandidate {
            target: SelectedTarget::Resource(test_entity(1)),
            distance: 0.1,
            priority: 3,
        };
        let farther_colonist = HitCandidate {
            target: SelectedTarget::Colonist(test_entity(2)),
            distance: 0.6,
            priority: 0,
        };

        assert_eq!(
            best_hit(&[close_resource, farther_colonist])
                .unwrap()
                .target,
            SelectedTarget::Colonist(test_entity(2))
        );
    }

    #[test]
    fn best_hit_uses_distance_inside_same_priority() {
        let farther = HitCandidate {
            target: SelectedTarget::Building(test_entity(1)),
            distance: 0.8,
            priority: 2,
        };
        let closer = HitCandidate {
            target: SelectedTarget::Building(test_entity(2)),
            distance: 0.2,
            priority: 2,
        };

        assert_eq!(
            best_hit(&[farther, closer]).unwrap().target,
            SelectedTarget::Building(test_entity(2))
        );
    }

    #[test]
    fn building_hit_uses_root_rotation_instead_of_scale() {
        let transform = Transform {
            translation: Vec3::ZERO,
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            scale: Vec3::ONE,
        };

        assert!(point_in_building_box(
            Vec3::new(0.0, 0.0, -1.2),
            &transform,
            BuildingKind::Storage,
            0.0
        ));
        assert!(!point_in_building_box(
            Vec3::new(1.2, 0.0, 0.0),
            &transform,
            BuildingKind::Storage,
            0.0
        ));
    }
}
