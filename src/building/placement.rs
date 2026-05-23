use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    types::{
        BuildingKind, CELL_SIZE, entrance_local_offset, entrance_world_position, snap_to_grid,
    },
    world::{GameAssets, Ground},
};

use super::lifecycle::{
    spawn_building_visual, spawn_entrance_marker, spawn_entrance_preview, sync_building_visual,
};
use super::polygon::{FOOTPRINT_SCALE, ROAD_FOOTPRINT_SCALE};
use super::{
    Blueprint, BuildPreview, BuildState, BuildingEntrance, BuildingVisual, Footprint,
    PlacementIssue, WorldGeometry, footprint_polygon,
};

pub fn update_build_preview(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ground_query: Query<&GlobalTransform, With<Ground>>,
    assets: Option<Res<GameAssets>>,
    geometry: Res<WorldGeometry>,
    mut build_state: ResMut<BuildState>,
    mut preview_query: Query<
        (Entity, &mut Transform, &mut Visibility),
        (With<BuildPreview>, Without<BuildingVisual>),
    >,
    mut visual_query: Query<(
        &BuildingVisual,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let Some(assets) = assets else {
        return;
    };

    let Some(kind) = build_state.selected else {
        hide_preview(&mut build_state, &mut preview_query);
        hide_entrance_preview(&mut commands, &mut build_state);
        return;
    };

    let Some(cursor_world) = cursor_ground_position(&windows, &camera_query, &ground_query) else {
        hide_preview(&mut build_state, &mut preview_query);
        hide_entrance_preview(&mut commands, &mut build_state);
        return;
    };

    let definition = kind.definition();
    let position = if build_state.snap_to_grid {
        snap_to_grid(cursor_world)
    } else {
        cursor_world
    };
    let snapped_position = if build_state.snap_to_grid {
        snap_to_grid(position)
    } else {
        position
    };
    let polygon = footprint_polygon(
        kind,
        snapped_position,
        definition.size,
        build_state.rotation_angle,
    );
    let entrance = planned_entrance(
        kind,
        snapped_position,
        definition.size,
        build_state.rotation_angle,
    );
    let placement_issue = geometry.placement_issue_for_polygon(
        &polygon,
        entrance.map(|entrance| entrance.world_position),
        kind != BuildingKind::Road,
    );
    let valid = placement_issue.is_none();
    let invalid_reason = if valid { None } else { placement_issue };

    build_state.last_valid = valid;
    build_state.invalid_reason = invalid_reason;
    build_state.last_position = snapped_position;
    build_state.last_polygon = polygon;
    let reason_label = invalid_reason
        .map(PlacementIssue::label)
        .unwrap_or("unknown reason");
    build_state.status = if valid {
        format!(
            "{} blueprint ready. Cost: {} wood.",
            definition.label, definition.wood_cost
        )
    } else {
        format!("Cannot place {}: {}.", definition.label, reason_label)
    };

    let scale = preview_scale(kind, definition.size, definition.height);
    let rotation = Quat::from_rotation_y(build_state.rotation_angle);
    let material = if valid {
        assets.preview_valid_material.clone()
    } else {
        assets.preview_invalid_material.clone()
    };

    let mut active_preview_entity = None;
    if let Some(entity) = build_state.preview_entity {
        if let Ok((_, mut transform, mut visibility)) = preview_query.get_mut(entity) {
            transform.translation = build_state.last_position;
            transform.rotation = rotation;
            transform.scale = Vec3::ONE;
            *visibility = Visibility::Visible;
            active_preview_entity = Some(entity);
        }
    }

    let preview_entity = if let Some(entity) = active_preview_entity {
        entity
    } else {
        let entity = commands
            .spawn((
                Transform {
                    translation: build_state.last_position,
                    rotation,
                    scale: Vec3::ONE,
                },
                Visibility::Visible,
                BuildPreview,
            ))
            .id();
        build_state.preview_entity = Some(entity);
        entity
    };

    sync_building_visual(
        &mut commands,
        &assets,
        preview_entity,
        material,
        scale,
        definition.height,
        &mut visual_query,
    );

    hide_entrance_preview(&mut commands, &mut build_state);
    if valid && kind != BuildingKind::Road {
        if let Some(entrance) = entrance {
            let ent_entity =
                spawn_entrance_preview(&mut commands, &assets, preview_entity, entrance.local_offset);
            build_state.preview_entrance_entity = Some(ent_entity);
        }
    }
}

pub fn place_blueprint(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    button_interactions: Query<&Interaction, With<Button>>,
    assets: Option<Res<GameAssets>>,
    mut geometry: ResMut<WorldGeometry>,
    mut build_state: ResMut<BuildState>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) || !build_state.last_valid {
        return;
    }
    if button_interactions
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }

    let Some(assets) = assets else {
        return;
    };
    let Some(kind) = build_state.selected else {
        return;
    };

    let definition = kind.definition();
    let scale = preview_scale(kind, definition.size, definition.height);
    let rotation = Quat::from_rotation_y(build_state.rotation_angle);
    let passable = kind == BuildingKind::Road;
    let polygon = build_state.last_polygon.clone();
    let entrance = planned_entrance(
        kind,
        build_state.last_position,
        definition.size,
        build_state.rotation_angle,
    );
    if let Some(issue) = geometry.placement_issue_for_polygon(
        &polygon,
        entrance.map(|entrance| entrance.world_position),
        kind != BuildingKind::Road,
    ) {
        build_state.status = format!("Cannot place {}: {}.", definition.label, issue.label());
        build_state.last_valid = false;
        build_state.invalid_reason = Some(issue);
        return;
    }

    let entity = commands
        .spawn((
            Transform {
                translation: build_state.last_position,
                rotation,
                scale: Vec3::ONE,
            },
            Visibility::Visible,
            Blueprint {
                kind,
                required_wood: definition.wood_cost,
                delivered_wood: 0,
                progress: 0.0,
                build_seconds: definition.build_seconds,
            },
            Footprint {
                polygon: polygon.clone(),
                passable,
            },
        ))
        .id();

    spawn_building_visual(
        &mut commands,
        &assets,
        entity,
        assets.blueprint_material.clone(),
        scale,
        definition.height,
    );

    geometry.occupy_polygon(polygon, entity, passable);
    if let Some(entrance) = entrance {
        commands.entity(entity).insert(BuildingEntrance {
            world_position: entrance.world_position,
            local_offset: entrance.local_offset,
        });
        geometry.reserve_entrance_point(entrance.world_position, entity);
        spawn_entrance_marker(&mut commands, &assets, entity, entrance.local_offset);
    }
    build_state.status = format!("Placed {} blueprint.", definition.label);
    build_state.last_valid = false;
    build_state.invalid_reason = None;
}

fn hide_preview(
    build_state: &mut BuildState,
    preview_query: &mut Query<
        (Entity, &mut Transform, &mut Visibility),
        (With<BuildPreview>, Without<BuildingVisual>),
    >,
) {
    build_state.invalid_reason = None;
    build_state.last_valid = false;
    if let Some(entity) = build_state.preview_entity {
        if let Ok((_, _, mut visibility)) = preview_query.get_mut(entity) {
            *visibility = Visibility::Hidden;
        }
    }
}

fn hide_entrance_preview(commands: &mut Commands, build_state: &mut BuildState) {
    if let Some(entity) = build_state.preview_entrance_entity.take() {
        commands.entity(entity).despawn();
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

fn preview_scale(kind: BuildingKind, size: IVec2, height: f32) -> Vec3 {
    if kind == BuildingKind::Road {
        Vec3::new(CELL_SIZE * ROAD_FOOTPRINT_SCALE, height, CELL_SIZE * ROAD_FOOTPRINT_SCALE)
    } else {
        Vec3::new(
            size.x as f32 * CELL_SIZE * FOOTPRINT_SCALE,
            height,
            size.y as f32 * CELL_SIZE * FOOTPRINT_SCALE,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PlannedEntrance {
    pub world_position: Vec3,
    pub local_offset: Vec3,
}

pub(super) fn planned_entrance(
    kind: BuildingKind,
    building_center: Vec3,
    size: IVec2,
    rotation_angle: f32,
) -> Option<PlannedEntrance> {
    kind.entrance_direction().map(|direction| {
        let local_offset = entrance_local_offset(size, direction);

        PlannedEntrance {
            world_position: entrance_world_position(
                building_center,
                size,
                rotation_angle,
                direction,
            ),
            local_offset,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::xz;
    use crate::types::BuildingKind;

    #[test]
    fn planned_entrance_uses_continuous_world_position() {
        let building_center = Vec3::new(1.37, 0.0, -2.22);
        let rotation_angle = 0.37;
        let definition = BuildingKind::Storage.definition();

        let entrance = planned_entrance(
            BuildingKind::Storage,
            building_center,
            definition.size,
            rotation_angle,
        )
        .unwrap();
        let visual_world_position = entrance_world_position(
            building_center,
            definition.size,
            rotation_angle,
            BuildingKind::Storage.entrance_direction().unwrap(),
        );

        assert_vec3_approx_eq(entrance.world_position, visual_world_position);
        assert_eq!(
            entrance.local_offset,
            entrance_local_offset(
                definition.size,
                BuildingKind::Storage.entrance_direction().unwrap()
            )
        );
    }

    #[test]
    fn planned_entrance_point_sits_outside_rotated_footprint() {
        let building_center = Vec3::new(1.37, 0.0, -2.22);
        let rotation_angle = 0.37;
        let definition = BuildingKind::Storage.definition();
        let polygon = footprint_polygon(
            BuildingKind::Storage,
            building_center,
            definition.size,
            rotation_angle,
        );

        let entrance = planned_entrance(
            BuildingKind::Storage,
            building_center,
            definition.size,
            rotation_angle,
        )
        .unwrap();

        assert!(!super::super::point_in_polygon(
            xz(entrance.world_position),
            &polygon
        ));
    }

    #[test]
    fn occupied_polygon_blocks_los_but_continuous_entrance_remains_reachable() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut geometry = WorldGeometry::default();
        let definition = BuildingKind::Storage.definition();
        let center = Vec3::new(1.37, 0.0, -2.22);
        let rotation = 0.37;
        let polygon = footprint_polygon(BuildingKind::Storage, center, definition.size, rotation);
        let entrance =
            planned_entrance(BuildingKind::Storage, center, definition.size, rotation).unwrap();

        geometry.occupy_polygon(polygon, entity, false);
        geometry.reserve_entrance_point(entrance.world_position, entity);

        assert!(geometry.is_walkable_point(entrance.world_position));
        assert!(
            crate::navigation::path_to_waypoints(
                &geometry,
                &mut crate::navigation::PathCache::default(),
                Vec3::new(-1.2, 0.0, 1.0),
                entrance.world_position,
            )
            .is_some()
        );
    }

    fn assert_vec3_approx_eq(actual: Vec3, expected: Vec3) {
        let delta = actual - expected;
        assert!(
            delta.length() < 0.0001,
            "expected {expected:?}, got {actual:?}"
        );
    }
}
