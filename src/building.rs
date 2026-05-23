use std::collections::HashMap;

use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    resources::ResourceStock,
    types::{
        BUILDING_KINDS, BuildingKind, CELL_SIZE, ResourceKind, cell_to_world, entrance_cell,
        footprint_cells, rotated_size, snap_to_grid, within_map, world_to_cell,
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
    pub invalid_reason: Option<PlacementIssue>,
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
            invalid_reason: None,
            status: "Select a building to start planning.".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementIssue {
    OutOfBounds,
    Occupied,
    EntranceBlocked,
}

impl PlacementIssue {
    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfBounds => "outside the buildable area",
            Self::Occupied => "blocked by another plan, building, resource, or entrance",
            Self::EntranceBlocked => "the entrance is blocked",
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
    reserved_entrances: HashMap<IVec2, Entity>,
}

impl MapGrid {
    pub fn is_area_free(&self, cells: &[IVec2]) -> bool {
        self.placement_issue(cells).is_none()
    }

    pub fn placement_issue(&self, cells: &[IVec2]) -> Option<PlacementIssue> {
        self.placement_issue_for(cells, None, true)
    }

    pub fn placement_issue_for(
        &self,
        cells: &[IVec2],
        entrance: Option<IVec2>,
        block_reserved_entrances: bool,
    ) -> Option<PlacementIssue> {
        if cells.iter().any(|cell| !within_map(*cell)) {
            return Some(PlacementIssue::OutOfBounds);
        }
        if cells.iter().any(|cell| self.occupied.contains_key(cell)) {
            return Some(PlacementIssue::Occupied);
        }
        if block_reserved_entrances
            && cells
                .iter()
                .any(|cell| self.reserved_entrances.contains_key(cell))
        {
            return Some(PlacementIssue::Occupied);
        }
        if let Some(entrance) = entrance {
            if !within_map(entrance) {
                return Some(PlacementIssue::OutOfBounds);
            }
            if !self.is_walkable(entrance) || self.reserved_entrances.contains_key(&entrance) {
                return Some(PlacementIssue::EntranceBlocked);
            }
        }

        None
    }

    pub fn occupy(&mut self, cells: &[IVec2], entity: Entity, passable: bool) {
        for cell in cells {
            self.occupied
                .insert(*cell, OccupiedCell { entity, passable });
        }
    }

    pub fn reserve_entrance(&mut self, cell: IVec2, entity: Entity) {
        self.reserved_entrances.insert(cell, entity);
    }

    pub fn release_entity(&mut self, entity: Entity) {
        self.occupied.retain(|_, cell| cell.entity != entity);
        self.reserved_entrances
            .retain(|_, reserved_entity| *reserved_entity != entity);
    }

    pub fn movement_cost(&self, cell: IVec2) -> Option<f32> {
        use crate::types::{GROUND_COST, ROAD_COST};
        if !within_map(cell) {
            return None;
        }
        match self.occupied.get(&cell) {
            Some(OccupiedCell { passable: true, .. }) => Some(ROAD_COST),
            Some(OccupiedCell { passable: false, .. }) => None,
            None => Some(GROUND_COST),
        }
    }

    pub fn is_walkable(&self, cell: IVec2) -> bool {
        within_map(cell)
            && self
                .occupied
                .get(&cell)
                .map(|cell| cell.passable)
                .unwrap_or(true)
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

    pub fn progress_ratio(&self) -> f32 {
        if self.build_seconds <= 0.0 {
            1.0
        } else {
            (self.progress / self.build_seconds).clamp(0.0, 1.0)
        }
    }

    pub fn status(&self) -> BlueprintStatus {
        if !self.has_materials() {
            BlueprintStatus::WaitingForMaterials
        } else if self.is_complete() {
            BlueprintStatus::Complete
        } else if self.progress > 0.0 {
            BlueprintStatus::Building
        } else {
            BlueprintStatus::WaitingForBuilder
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlueprintStatus {
    WaitingForMaterials,
    WaitingForBuilder,
    Building,
    Complete,
}

impl BlueprintStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingForMaterials => "Waiting for materials",
            Self::WaitingForBuilder => "Waiting for builder",
            Self::Building => "Building",
            Self::Complete => "Complete",
        }
    }
}

#[derive(Component, Debug)]
pub struct CompletedBuilding {
    pub kind: BuildingKind,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingEntrance {
    pub cell: IVec2,
}

#[derive(Component, Debug)]
pub struct EntranceMarker {
    pub owner: Entity,
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
        build_state.invalid_reason = None;
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
    let entrance = planned_entrance_cell(kind, center_cell, build_state.rotation_steps);
    let placement_issue = grid.placement_issue_for(&cells, entrance, kind != BuildingKind::Road);
    let valid = placement_issue.is_none();
    let invalid_reason = if valid { None } else { placement_issue };

    build_state.last_valid = valid;
    build_state.invalid_reason = invalid_reason;
    build_state.last_position = if build_state.snap_to_grid {
        cell_to_world(center_cell)
    } else {
        position
    };
    build_state.last_cells = cells;
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
    let center_cell = world_to_cell(build_state.last_position);
    let entrance = planned_entrance_cell(kind, center_cell, build_state.rotation_steps);
    if let Some(issue) = grid.placement_issue_for(&cells, entrance, kind != BuildingKind::Road) {
        build_state.status = format!("Cannot place {}: {}.", definition.label, issue.label());
        build_state.last_valid = false;
        build_state.invalid_reason = Some(issue);
        return;
    }

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
    if let Some(entrance) = entrance {
        commands
            .entity(entity)
            .insert(BuildingEntrance { cell: entrance });
        grid.reserve_entrance(entrance, entity);
        spawn_entrance_marker(&mut commands, &assets, entity, entrance);
    }
    build_state.status = format!("Placed {} blueprint.", definition.label);
    build_state.last_valid = false;
    build_state.invalid_reason = None;
}

pub fn update_blueprint_visuals(mut blueprints: Query<(&Blueprint, &mut Transform)>) {
    for (blueprint, mut transform) in &mut blueprints {
        let height = blueprint.kind.definition().height;
        let visual_height = (height * (0.35 + blueprint.progress_ratio() * 0.65)).max(0.04);
        transform.scale.y = visual_height;
        transform.translation.y = visual_height * 0.5;
    }
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

pub fn sync_entrance_markers(
    entrances: Query<&BuildingEntrance>,
    mut markers: Query<(&EntranceMarker, &mut Transform)>,
) {
    for (marker, mut transform) in &mut markers {
        if let Ok(entrance) = entrances.get(marker.owner) {
            transform.translation = entrance_marker_translation(entrance.cell);
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
    build_state.invalid_reason = None;
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

fn planned_entrance_cell(
    kind: BuildingKind,
    center_cell: IVec2,
    rotation_steps: i32,
) -> Option<IVec2> {
    kind.entrance_direction().map(|direction| {
        entrance_cell(
            center_cell,
            kind.definition().size,
            rotation_steps,
            direction,
        )
    })
}

fn spawn_entrance_marker(commands: &mut Commands, assets: &GameAssets, owner: Entity, cell: IVec2) {
    commands.spawn((
        Mesh3d(assets.cube_mesh.clone()),
        MeshMaterial3d(assets.entrance_material.clone()),
        Transform::from_translation(entrance_marker_translation(cell))
            .with_scale(Vec3::new(0.42, 0.08, 0.42)),
        EntranceMarker { owner },
    ));
}

fn entrance_marker_translation(cell: IVec2) -> Vec3 {
    let position = cell_to_world(cell);
    Vec3::new(position.x, 0.04, position.z)
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

    #[test]
    fn blueprint_status_tracks_materials_and_work() {
        let mut blueprint = Blueprint {
            kind: BuildingKind::House,
            required_wood: 4,
            delivered_wood: 0,
            progress: 0.0,
            build_seconds: 5.0,
        };

        assert_eq!(blueprint.status(), BlueprintStatus::WaitingForMaterials);
        blueprint.delivered_wood = 4;
        assert_eq!(blueprint.status(), BlueprintStatus::WaitingForBuilder);
        blueprint.progress = 2.0;
        assert_eq!(blueprint.status(), BlueprintStatus::Building);
        blueprint.progress = 5.0;
        assert_eq!(blueprint.status(), BlueprintStatus::Complete);
    }

    #[test]
    fn placement_issue_prefers_map_bounds_before_occupancy() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(0, 0)], entity, false);

        assert_eq!(
            grid.placement_issue(&[IVec2::new(0, 0)]),
            Some(PlacementIssue::Occupied)
        );
        assert_eq!(
            grid.placement_issue(&[IVec2::new(0, 0), IVec2::new(999, 999)]),
            Some(PlacementIssue::OutOfBounds)
        );
    }

    #[test]
    fn grid_releases_entity_occupancy() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut grid = MapGrid::default();
        let cell = IVec2::new(2, 2);

        grid.occupy(&[cell], entity, false);
        assert!(!grid.is_walkable(cell));

        grid.release_entity(entity);
        assert!(grid.is_walkable(cell));
    }

    #[test]
    fn road_cells_are_walkable() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut grid = MapGrid::default();
        let cell = IVec2::new(2, 2);

        grid.occupy(&[cell], entity, true);

        assert!(grid.is_walkable(cell));
    }

    #[test]
    fn reserved_entrance_blocks_non_road_footprint() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut grid = MapGrid::default();
        let cell = IVec2::new(2, 2);

        grid.reserve_entrance(cell, entity);

        assert_eq!(
            grid.placement_issue_for(&[cell], None, true),
            Some(PlacementIssue::Occupied)
        );
        assert_eq!(grid.placement_issue_for(&[cell], None, false), None);
    }

    #[test]
    fn placement_rejects_blocked_entrance() {
        let mut world = World::new();
        let building = world.spawn_empty().id();
        let blocker = world.spawn_empty().id();
        let mut grid = MapGrid::default();

        grid.occupy(&[IVec2::new(0, -1)], blocker, false);

        assert_eq!(
            grid.placement_issue_for(&[IVec2::ZERO], Some(IVec2::new(0, -1)), true),
            Some(PlacementIssue::EntranceBlocked)
        );

        grid.release_entity(blocker);
        grid.reserve_entrance(IVec2::new(0, -1), building);
        assert_eq!(
            grid.placement_issue_for(&[IVec2::ZERO], Some(IVec2::new(0, -1)), true),
            Some(PlacementIssue::EntranceBlocked)
        );
    }
}
