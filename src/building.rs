use std::collections::HashMap;

use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    resources::ResourceStock,
    types::{
        BUILDING_KINDS, BuildingKind, CELL_SIZE, ResourceKind, cell_to_world, footprint_cells,
        rotated_size, snap_to_grid, within_map, world_to_cell,
    },
    world::{GameAssets, Ground},
};

#[derive(Resource, Debug)]
pub struct BuildState {
    pub selected: Option<BuildingKind>,
    pub snap_to_grid: bool,
    pub rotation_steps: i32,
    pub preview_entity: Option<Entity>,
    pub last_valid: bool,
    pub last_position: Vec3,
    pub last_cells: Vec<IVec2>,
    pub status: String,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            selected: None,
            snap_to_grid: true,
            rotation_steps: 0,
            preview_entity: None,
            last_valid: false,
            last_position: Vec3::ZERO,
            last_cells: Vec::new(),
            status: "Select a building to start planning.".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OccupiedCell {
    pub entity: Entity,
    pub passable: bool,
}

#[derive(Resource, Debug, Default)]
pub struct MapGrid {
    occupied: HashMap<IVec2, OccupiedCell>,
}

impl MapGrid {
    pub fn is_area_free(&self, cells: &[IVec2]) -> bool {
        cells
            .iter()
            .all(|cell| within_map(*cell) && !self.occupied.contains_key(cell))
    }

    pub fn occupy(&mut self, cells: &[IVec2], entity: Entity, passable: bool) {
        for cell in cells {
            self.occupied
                .insert(*cell, OccupiedCell { entity, passable });
        }
    }

    pub fn summary(&self) -> (usize, usize, usize) {
        let road_cells = self.occupied.values().filter(|cell| cell.passable).count();
        let mut entities = Vec::new();
        for cell in self.occupied.values() {
            if !entities.contains(&cell.entity) {
                entities.push(cell.entity);
            }
        }

        (self.occupied.len(), road_cells, entities.len())
    }
}

#[derive(Component)]
pub struct BuildPreview;

#[derive(Component, Debug)]
pub struct Footprint {
    pub cells: Vec<IVec2>,
    pub passable: bool,
}

#[derive(Component, Debug)]
pub struct Blueprint {
    pub kind: BuildingKind,
    pub required_wood: i32,
    pub delivered_wood: i32,
    pub progress: f32,
    pub build_seconds: f32,
}

impl Blueprint {
    pub fn needs_wood(&self) -> i32 {
        (self.required_wood - self.delivered_wood).max(0)
    }

    pub fn has_materials(&self) -> bool {
        self.needs_wood() == 0
    }

    pub fn is_complete(&self) -> bool {
        self.has_materials() && self.progress >= self.build_seconds
    }
}

#[derive(Component, Debug)]
pub struct CompletedBuilding {
    pub kind: BuildingKind,
}

pub fn handle_build_hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut build_state: ResMut<BuildState>,
) {
    for kind in BUILDING_KINDS {
        if keyboard.just_pressed(kind.hotkey()) {
            build_state.selected = Some(kind);
            build_state.status = format!("Planning {}.", kind.definition().label);
        }
    }

    if keyboard.just_pressed(KeyCode::KeyG) {
        build_state.snap_to_grid = !build_state.snap_to_grid;
        build_state.status = format!(
            "Grid snap {}.",
            if build_state.snap_to_grid {
                "on"
            } else {
                "off"
            }
        );
    }

    if keyboard.just_pressed(KeyCode::KeyR) {
        build_state.rotation_steps = (build_state.rotation_steps + 1).rem_euclid(4);
    }

    if keyboard.just_pressed(KeyCode::Escape)
        || (build_state.selected.is_some() && mouse_buttons.just_pressed(MouseButton::Right))
    {
        build_state.selected = None;
        build_state.last_valid = false;
        build_state.status = "Build mode cancelled.".to_string();
    }
}

pub fn update_build_preview(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    ground_query: Query<&GlobalTransform, With<Ground>>,
    assets: Option<Res<GameAssets>>,
    grid: Res<MapGrid>,
    mut build_state: ResMut<BuildState>,
    mut preview_query: Query<
        (
            Entity,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Visibility,
        ),
        With<BuildPreview>,
    >,
) {
    let Some(assets) = assets else {
        return;
    };

    let Some(kind) = build_state.selected else {
        hide_preview(&mut build_state, &mut preview_query);
        return;
    };

    let Some(cursor_world) = cursor_ground_position(&windows, &camera_query, &ground_query) else {
        hide_preview(&mut build_state, &mut preview_query);
        return;
    };

    let definition = kind.definition();
    let size = rotated_size(definition.size, build_state.rotation_steps);
    let position = if build_state.snap_to_grid {
        snap_to_grid(cursor_world)
    } else {
        cursor_world
    };
    let center_cell = world_to_cell(position);
    let cells = footprint_cells(center_cell, size);
    let valid = grid.is_area_free(&cells);

    build_state.last_valid = valid;
    build_state.last_position = if build_state.snap_to_grid {
        cell_to_world(center_cell)
    } else {
        position
    };
    build_state.last_cells = cells;
    build_state.status = if valid {
        format!(
            "{} blueprint ready. Cost: {} wood.",
            definition.label, definition.wood_cost
        )
    } else {
        format!("Cannot place {} here.", definition.label)
    };

    let scale = preview_scale(kind, size, definition.height);
    let translation = preview_translation(build_state.last_position, definition.height);
    let rotation =
        Quat::from_rotation_y(build_state.rotation_steps as f32 * std::f32::consts::FRAC_PI_2);
    let material = if valid {
        assets.preview_valid_material.clone()
    } else {
        assets.preview_invalid_material.clone()
    };

    if let Some(entity) = build_state.preview_entity {
        if let Ok((_, mut transform, mut preview_material, mut visibility)) =
            preview_query.get_mut(entity)
        {
            transform.translation = translation;
            transform.rotation = rotation;
            transform.scale = scale;
            preview_material.0 = material;
            *visibility = Visibility::Visible;
            return;
        }
    }

    let entity = commands
        .spawn((
            Mesh3d(assets.cube_mesh.clone()),
            MeshMaterial3d(material),
            Transform {
                translation,
                rotation,
                scale,
            },
            BuildPreview,
        ))
        .id();
    build_state.preview_entity = Some(entity);
}

pub fn place_blueprint(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    button_interactions: Query<&Interaction, With<Button>>,
    assets: Option<Res<GameAssets>>,
    mut grid: ResMut<MapGrid>,
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
    let size = rotated_size(definition.size, build_state.rotation_steps);
    let scale = preview_scale(kind, size, definition.height);
    let translation = preview_translation(build_state.last_position, definition.height);
    let rotation =
        Quat::from_rotation_y(build_state.rotation_steps as f32 * std::f32::consts::FRAC_PI_2);
    let passable = kind == BuildingKind::Road;
    let cells = build_state.last_cells.clone();

    let entity = commands
        .spawn((
            Mesh3d(assets.cube_mesh.clone()),
            MeshMaterial3d(assets.blueprint_material.clone()),
            Transform {
                translation,
                rotation,
                scale,
            },
            Blueprint {
                kind,
                required_wood: definition.wood_cost,
                delivered_wood: 0,
                progress: 0.0,
                build_seconds: definition.build_seconds,
            },
            Footprint {
                cells: cells.clone(),
                passable,
            },
        ))
        .id();

    grid.occupy(&cells, entity, passable);
    build_state.status = format!("Placed {} blueprint.", definition.label);
    build_state.last_valid = false;
}

pub fn finish_blueprints(
    mut commands: Commands,
    assets: Option<Res<GameAssets>>,
    mut stock: ResMut<ResourceStock>,
    mut blueprint_query: Query<(
        Entity,
        &Blueprint,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&Footprint>,
    )>,
) {
    let Some(assets) = assets else {
        return;
    };

    for (entity, blueprint, mut material, footprint) in &mut blueprint_query {
        if !blueprint.is_complete() {
            continue;
        }

        material.0 = assets.building_material(blueprint.kind);
        if blueprint.kind == BuildingKind::Storage {
            stock.add(ResourceKind::Wood, 4);
        }

        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<Blueprint>();
        entity_commands.insert(CompletedBuilding {
            kind: blueprint.kind,
        });
        if let Some(footprint) = footprint {
            entity_commands.insert(Footprint {
                cells: footprint.cells.clone(),
                passable: footprint.passable,
            });
        }
    }
}

fn hide_preview(
    build_state: &mut BuildState,
    preview_query: &mut Query<
        (
            Entity,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Visibility,
        ),
        With<BuildPreview>,
    >,
) {
    build_state.last_valid = false;
    if let Some(entity) = build_state.preview_entity {
        if let Ok((_, _, _, mut visibility)) = preview_query.get_mut(entity) {
            *visibility = Visibility::Hidden;
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

fn preview_scale(kind: BuildingKind, size: IVec2, height: f32) -> Vec3 {
    if kind == BuildingKind::Road {
        Vec3::new(CELL_SIZE * 0.95, height, CELL_SIZE * 0.95)
    } else {
        Vec3::new(
            size.x as f32 * CELL_SIZE * 0.9,
            height,
            size.y as f32 * CELL_SIZE * 0.9,
        )
    }
}

fn preview_translation(position: Vec3, height: f32) -> Vec3 {
    Vec3::new(position.x, height * 0.5, position.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_rejects_occupied_area() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut grid = MapGrid::default();
        let cells = vec![IVec2::new(0, 0), IVec2::new(1, 0)];

        assert!(grid.is_area_free(&cells));
        grid.occupy(&cells, entity, false);
        assert!(!grid.is_area_free(&cells));
        assert_eq!(grid.summary().0, 2);
    }

    #[test]
    fn blueprint_waits_for_materials_before_completion() {
        let mut blueprint = Blueprint {
            kind: BuildingKind::House,
            required_wood: 4,
            delivered_wood: 3,
            progress: 99.0,
            build_seconds: 5.0,
        };

        assert!(!blueprint.is_complete());
        blueprint.delivered_wood = 4;
        assert!(blueprint.is_complete());
    }
}
