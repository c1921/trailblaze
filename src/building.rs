use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    resources::ResourceStock,
    types::{
        BUILDING_KINDS, BuildingKind, CELL_SIZE, MAP_HALF_CELLS, ResourceKind,
        entrance_local_offset, entrance_world_position, snap_to_grid,
    },
    world::{GameAssets, Ground},
};

#[cfg(test)]
use crate::types::within_map;

const FOOTPRINT_SCALE: f32 = 0.9;
const ROAD_FOOTPRINT_SCALE: f32 = 0.95;
const ENTRANCE_RESERVATION_RADIUS: f32 = 0.35;
const GEOMETRY_EPSILON: f32 = 0.0001;

#[derive(Resource, Debug)]
pub struct BuildState {
    pub selected: Option<BuildingKind>,
    pub snap_to_grid: bool,
    pub rotation_angle: f32,
    pub r_hold_timer: f32,
    pub preview_entity: Option<Entity>,
    pub preview_entrance_entity: Option<Entity>,
    pub last_valid: bool,
    pub last_position: Vec3,
    pub last_polygon: Vec<Vec2>,
    pub invalid_reason: Option<PlacementIssue>,
    pub status: String,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            selected: None,
            snap_to_grid: true,
            rotation_angle: 0.0,
            r_hold_timer: 0.0,
            preview_entity: None,
            preview_entrance_entity: None,
            last_valid: false,
            last_position: Vec3::ZERO,
            last_polygon: Vec::new(),
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

#[derive(Clone, Debug)]
pub struct Obstacle {
    pub entity: Entity,
    pub polygon: Vec<Vec2>,
    pub passable: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ReservedEntrance {
    pub entity: Entity,
    pub position: Vec2,
}

#[derive(Resource, Debug, Default)]
pub struct WorldGeometry {
    obstacles: Vec<Obstacle>,
    reserved_entrances: Vec<ReservedEntrance>,
}

#[cfg(test)]
pub type MapGrid = WorldGeometry;

impl WorldGeometry {
    #[cfg(test)]
    pub fn is_area_free(&self, cells: &[IVec2]) -> bool {
        self.placement_issue(cells).is_none()
    }

    #[cfg(test)]
    pub fn placement_issue(&self, cells: &[IVec2]) -> Option<PlacementIssue> {
        self.placement_issue_for(cells, None, true)
    }

    #[cfg(test)]
    pub fn placement_issue_for(
        &self,
        cells: &[IVec2],
        entrance: Option<IVec2>,
        block_reserved_entrances: bool,
    ) -> Option<PlacementIssue> {
        if cells.iter().any(|cell| !within_map(*cell)) {
            return Some(PlacementIssue::OutOfBounds);
        }
        if cells.iter().any(|cell| {
            let polygon = cell_polygon(*cell);
            self.obstacles
                .iter()
                .any(|obstacle| polygons_intersect(&polygon, &obstacle.polygon))
        }) {
            return Some(PlacementIssue::Occupied);
        }
        if block_reserved_entrances
            && cells.iter().any(|cell| {
                self.reserved_entrances
                    .iter()
                    .any(|reserved| reserved.position == cell_center_2d(*cell))
            })
        {
            return Some(PlacementIssue::Occupied);
        }
        if let Some(entrance) = entrance {
            if !within_map(entrance) {
                return Some(PlacementIssue::OutOfBounds);
            }
            let entrance_position = cell_center_2d(entrance);
            if !self.is_walkable(entrance)
                || self.reserved_entrances.iter().any(|reserved| {
                    reserved.position.distance(entrance_position) <= ENTRANCE_RESERVATION_RADIUS
                })
            {
                return Some(PlacementIssue::EntranceBlocked);
            }
        }

        None
    }

    pub fn placement_issue_for_polygon(
        &self,
        polygon: &[Vec2],
        entrance: Option<Vec3>,
        block_reserved_entrances: bool,
    ) -> Option<PlacementIssue> {
        if polygon.iter().any(|point| !within_world_bounds(*point)) {
            return Some(PlacementIssue::OutOfBounds);
        }
        if self
            .obstacles
            .iter()
            .any(|obstacle| polygons_intersect(polygon, &obstacle.polygon))
        {
            return Some(PlacementIssue::Occupied);
        }
        if block_reserved_entrances
            && self.reserved_entrances.iter().any(|reserved| {
                point_in_polygon(reserved.position, polygon)
                    || distance_to_polygon(reserved.position, polygon)
                        <= ENTRANCE_RESERVATION_RADIUS
            })
        {
            return Some(PlacementIssue::Occupied);
        }

        if let Some(entrance) = entrance {
            let entrance = xz(entrance);
            if !within_world_bounds(entrance) {
                return Some(PlacementIssue::OutOfBounds);
            }
            if !self.is_walkable_point_2d(entrance)
                || self.reserved_entrances.iter().any(|reserved| {
                    reserved.position.distance(entrance) <= ENTRANCE_RESERVATION_RADIUS
                })
            {
                return Some(PlacementIssue::EntranceBlocked);
            }
        }

        None
    }

    #[cfg(test)]
    pub fn occupy(&mut self, cells: &[IVec2], entity: Entity, passable: bool) {
        for cell in cells {
            self.occupy_polygon(cell_polygon(*cell), entity, passable);
        }
    }

    pub fn occupy_polygon(&mut self, polygon: Vec<Vec2>, entity: Entity, passable: bool) {
        self.obstacles.push(Obstacle {
            entity,
            polygon,
            passable,
        });
    }

    #[cfg(test)]
    pub fn reserve_entrance(&mut self, cell: IVec2, entity: Entity) {
        self.reserve_entrance_point_2d(cell_center_2d(cell), entity);
    }

    pub fn reserve_entrance_point(&mut self, position: Vec3, entity: Entity) {
        self.reserve_entrance_point_2d(xz(position), entity);
    }

    pub fn reserve_entrance_point_2d(&mut self, position: Vec2, entity: Entity) {
        self.reserved_entrances
            .push(ReservedEntrance { entity, position });
    }

    pub fn release_entity(&mut self, entity: Entity) {
        self.obstacles.retain(|obstacle| obstacle.entity != entity);
        self.reserved_entrances
            .retain(|reserved| reserved.entity != entity);
    }

    #[cfg(test)]
    pub fn is_walkable(&self, cell: IVec2) -> bool {
        within_map(cell) && self.is_walkable_point_2d(cell_center_2d(cell))
    }

    pub fn is_walkable_point(&self, point: Vec3) -> bool {
        self.is_walkable_point_2d(xz(point))
    }

    pub fn is_walkable_point_2d(&self, point: Vec2) -> bool {
        within_world_bounds(point)
            && self
                .obstacles
                .iter()
                .filter(|obstacle| !obstacle.passable)
                .all(|obstacle| !point_in_polygon(point, &obstacle.polygon))
    }

    pub fn obstacles(&self) -> &[Obstacle] {
        &self.obstacles
    }

    pub fn segment_clear(&self, from: Vec3, to: Vec3, padding: f32) -> bool {
        self.segment_clear_2d(xz(from), xz(to), padding)
    }

    pub fn segment_clear_2d(&self, from: Vec2, to: Vec2, padding: f32) -> bool {
        if !within_world_bounds(from) || !within_world_bounds(to) {
            return false;
        }

        self.obstacles
            .iter()
            .filter(|obstacle| !obstacle.passable)
            .all(|obstacle| {
                let polygon = expanded_polygon(&obstacle.polygon, padding);
                !segment_intersects_polygon(from, to, &polygon)
                    && !point_in_polygon(from, &polygon)
                    && !point_in_polygon(to, &polygon)
            })
    }

    pub fn summary(&self) -> (usize, usize, usize) {
        let road_cells = self.obstacles.iter().filter(|cell| cell.passable).count();
        let mut entities = Vec::new();
        for obstacle in &self.obstacles {
            if !entities.contains(&obstacle.entity) {
                entities.push(obstacle.entity);
            }
        }

        (self.obstacles.len(), road_cells, entities.len())
    }
}

#[derive(Component)]
pub struct BuildPreview;

#[derive(Component)]
pub struct EntrancePreview;

#[derive(Component, Debug)]
pub struct BuildingVisual {
    pub owner: Entity,
}

#[derive(Component, Debug)]
pub struct Footprint {
    pub polygon: Vec<Vec2>,
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
    pub world_position: Vec3,
    pub local_offset: Vec3,
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

    if keyboard.just_pressed(KeyCode::Escape)
        || (build_state.selected.is_some() && mouse_buttons.just_pressed(MouseButton::Right))
    {
        build_state.selected = None;
        build_state.last_valid = false;
        build_state.invalid_reason = None;
        build_state.status = "Build mode cancelled.".to_string();
    }
}

pub fn handle_rotation_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut build_state: ResMut<BuildState>,
) {
    if build_state.selected.is_none() {
        return;
    }

    if build_state.snap_to_grid {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_state.rotation_angle = (build_state.rotation_angle + std::f32::consts::FRAC_PI_2)
                .rem_euclid(std::f32::consts::TAU);
        }
    } else {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_state.r_hold_timer = 0.0;
        }
        if keyboard.pressed(KeyCode::KeyR) {
            build_state.r_hold_timer += time.delta_secs();
            if build_state.r_hold_timer >= 0.2 {
                build_state.rotation_angle = (build_state.rotation_angle
                    + std::f32::consts::PI * time.delta_secs())
                .rem_euclid(std::f32::consts::TAU);
            }
        }
        if keyboard.just_released(KeyCode::KeyR) {
            if build_state.r_hold_timer > 0.0 && build_state.r_hold_timer < 0.2 {
                build_state.rotation_angle = (build_state.rotation_angle
                    + std::f32::consts::FRAC_PI_2)
                    .rem_euclid(std::f32::consts::TAU);
            }
            build_state.r_hold_timer = 0.0;
        }
    }
}

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
            let ent_entity = commands
                .spawn((
                    Mesh3d(assets.cube_mesh.clone()),
                    MeshMaterial3d(assets.entrance_material.clone()),
                    Transform::from_translation(entrance_marker_translation(entrance.local_offset))
                        .with_scale(Vec3::new(0.42, 0.08, 0.42)),
                    EntrancePreview,
                    ChildOf(preview_entity),
                ))
                .id();
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

pub fn update_blueprint_visuals(
    blueprints: Query<&Blueprint>,
    mut visuals: Query<(&BuildingVisual, &mut Transform)>,
) {
    for (visual, mut transform) in &mut visuals {
        let Ok(blueprint) = blueprints.get(visual.owner) else {
            continue;
        };
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
    blueprint_query: Query<(Entity, &Blueprint, Option<&Footprint>)>,
    mut visuals: Query<(&BuildingVisual, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(assets) = assets else {
        return;
    };

    for (entity, blueprint, footprint) in &blueprint_query {
        if !blueprint.is_complete() {
            continue;
        }

        for (visual, mut material) in &mut visuals {
            if visual.owner == entity {
                material.0 = assets.building_material(blueprint.kind);
                break;
            }
        }
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
                polygon: footprint.polygon.clone(),
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
            transform.translation = entrance_marker_translation(entrance.local_offset);
        }
    }
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
        Vec3::new(CELL_SIZE * 0.95, height, CELL_SIZE * 0.95)
    } else {
        Vec3::new(
            size.x as f32 * CELL_SIZE * 0.9,
            height,
            size.y as f32 * CELL_SIZE * 0.9,
        )
    }
}

fn visual_translation(height: f32) -> Vec3 {
    Vec3::new(0.0, height * 0.5, 0.0)
}

#[derive(Clone, Copy, Debug)]
struct PlannedEntrance {
    world_position: Vec3,
    local_offset: Vec3,
}

fn planned_entrance(
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

fn sync_building_visual(
    commands: &mut Commands,
    assets: &GameAssets,
    owner: Entity,
    material: Handle<StandardMaterial>,
    scale: Vec3,
    height: f32,
    visuals: &mut Query<(
        &BuildingVisual,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (visual, mut transform, mut visual_material) in visuals.iter_mut() {
        if visual.owner == owner {
            transform.translation = visual_translation(height);
            transform.scale = scale;
            visual_material.0 = material;
            return;
        }
    }

    spawn_building_visual(commands, assets, owner, material, scale, height);
}

fn spawn_building_visual(
    commands: &mut Commands,
    assets: &GameAssets,
    owner: Entity,
    material: Handle<StandardMaterial>,
    scale: Vec3,
    height: f32,
) {
    commands.spawn((
        Mesh3d(assets.cube_mesh.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(visual_translation(height)).with_scale(scale),
        BuildingVisual { owner },
        ChildOf(owner),
    ));
}

fn spawn_entrance_marker(
    commands: &mut Commands,
    assets: &GameAssets,
    owner: Entity,
    local_offset: Vec3,
) {
    commands.spawn((
        Mesh3d(assets.cube_mesh.clone()),
        MeshMaterial3d(assets.entrance_material.clone()),
        Transform::from_translation(entrance_marker_translation(local_offset))
            .with_scale(Vec3::new(0.42, 0.08, 0.42)),
        EntranceMarker { owner },
        ChildOf(owner),
    ));
}

fn entrance_marker_translation(local_offset: Vec3) -> Vec3 {
    Vec3::new(local_offset.x, 0.04, local_offset.z)
}

pub fn footprint_polygon(
    kind: BuildingKind,
    center: Vec3,
    size: IVec2,
    rotation_angle: f32,
) -> Vec<Vec2> {
    rectangle_polygon(center, footprint_dimensions(kind, size), rotation_angle)
}

pub fn resource_obstacle_polygon(position: Vec3) -> Vec<Vec2> {
    rectangle_polygon(
        Vec3::new(position.x, 0.0, position.z),
        Vec2::splat(0.8),
        0.0,
    )
}

pub fn rectangle_polygon(center: Vec3, size: Vec2, rotation_angle: f32) -> Vec<Vec2> {
    let half = size * 0.5;
    let cos = rotation_angle.cos();
    let sin = rotation_angle.sin();
    [
        (-half.x, -half.y),
        (half.x, -half.y),
        (half.x, half.y),
        (-half.x, half.y),
    ]
    .into_iter()
    .map(|(local_x, local_z)| {
        Vec2::new(
            center.x + local_x * cos + local_z * sin,
            center.z - local_x * sin + local_z * cos,
        )
    })
    .collect()
}

pub fn expanded_polygon(polygon: &[Vec2], padding: f32) -> Vec<Vec2> {
    if padding <= 0.0 || polygon.is_empty() {
        return polygon.to_vec();
    }

    let center = polygon
        .iter()
        .copied()
        .fold(Vec2::ZERO, |sum, point| sum + point)
        / polygon.len() as f32;
    polygon
        .iter()
        .map(|point| {
            let from_center = *point - center;
            if from_center.length_squared() < GEOMETRY_EPSILON {
                *point
            } else {
                *point + from_center.normalize() * padding
            }
        })
        .collect()
}

pub fn polygons_intersect(left: &[Vec2], right: &[Vec2]) -> bool {
    if left.len() < 3 || right.len() < 3 {
        return false;
    }

    !has_separating_axis(left, right) && !has_separating_axis(right, left)
}

pub fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }

    let mut sign = 0.0f32;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let cross = cross_2d(b - a, point - a);
        if cross.abs() <= GEOMETRY_EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign * cross < -GEOMETRY_EPSILON {
            return false;
        }
    }

    true
}

pub fn segment_intersects_polygon(from: Vec2, to: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    if point_in_polygon(from, polygon) || point_in_polygon(to, polygon) {
        return true;
    }

    polygon.iter().enumerate().any(|(index, start)| {
        let end = polygon[(index + 1) % polygon.len()];
        segments_intersect(from, to, *start, end)
    })
}

pub fn distance_to_polygon(point: Vec2, polygon: &[Vec2]) -> f32 {
    if point_in_polygon(point, polygon) {
        return 0.0;
    }

    polygon
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = polygon[(index + 1) % polygon.len()];
            distance_to_segment(point, *start, end)
        })
        .fold(f32::MAX, f32::min)
}

pub fn xz(position: Vec3) -> Vec2 {
    Vec2::new(position.x, position.z)
}

fn footprint_dimensions(kind: BuildingKind, size: IVec2) -> Vec2 {
    if kind == BuildingKind::Road {
        Vec2::splat(CELL_SIZE * ROAD_FOOTPRINT_SCALE)
    } else {
        Vec2::new(
            size.x as f32 * CELL_SIZE * FOOTPRINT_SCALE,
            size.y as f32 * CELL_SIZE * FOOTPRINT_SCALE,
        )
    }
}

fn has_separating_axis(left: &[Vec2], right: &[Vec2]) -> bool {
    for index in 0..left.len() {
        let a = left[index];
        let b = left[(index + 1) % left.len()];
        let edge = b - a;
        if edge.length_squared() <= GEOMETRY_EPSILON {
            continue;
        }
        let axis = Vec2::new(-edge.y, edge.x).normalize();
        let (left_min, left_max) = project_polygon(left, axis);
        let (right_min, right_max) = project_polygon(right, axis);
        if left_max <= right_min + GEOMETRY_EPSILON || right_max <= left_min + GEOMETRY_EPSILON {
            return true;
        }
    }
    false
}

fn project_polygon(polygon: &[Vec2], axis: Vec2) -> (f32, f32) {
    polygon
        .iter()
        .map(|point| point.dot(axis))
        .fold((f32::MAX, f32::MIN), |(min, max), value| {
            (min.min(value), max.max(value))
        })
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let r = b - a;
    let s = d - c;
    let denominator = cross_2d(r, s);
    let c_minus_a = c - a;

    if denominator.abs() <= GEOMETRY_EPSILON {
        if cross_2d(c_minus_a, r).abs() > GEOMETRY_EPSILON {
            return false;
        }
        let rr = r.length_squared();
        if rr <= GEOMETRY_EPSILON {
            return a.distance(c) <= GEOMETRY_EPSILON;
        }
        let t0 = c_minus_a.dot(r) / rr;
        let t1 = t0 + s.dot(r) / rr;
        let min_t = t0.min(t1);
        let max_t = t0.max(t1);
        return max_t > GEOMETRY_EPSILON && min_t < 1.0 - GEOMETRY_EPSILON;
    }

    let t = cross_2d(c_minus_a, s) / denominator;
    let u = cross_2d(c_minus_a, r) / denominator;
    t >= -GEOMETRY_EPSILON
        && t <= 1.0 + GEOMETRY_EPSILON
        && u >= -GEOMETRY_EPSILON
        && u <= 1.0 + GEOMETRY_EPSILON
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= GEOMETRY_EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

fn cross_2d(left: Vec2, right: Vec2) -> f32 {
    left.x * right.y - left.y * right.x
}

#[cfg(test)]
fn cell_polygon(cell: IVec2) -> Vec<Vec2> {
    rectangle_polygon(
        Vec3::new(cell.x as f32 * CELL_SIZE, 0.0, cell.y as f32 * CELL_SIZE),
        Vec2::splat(CELL_SIZE),
        0.0,
    )
}

#[cfg(test)]
fn cell_center_2d(cell: IVec2) -> Vec2 {
    Vec2::new(cell.x as f32 * CELL_SIZE, cell.y as f32 * CELL_SIZE)
}

fn within_world_bounds(point: Vec2) -> bool {
    let half = (MAP_HALF_CELLS as f32 + 0.5) * CELL_SIZE;
    point.x >= -half && point.x <= half && point.y >= -half && point.y <= half
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

        assert!(!point_in_polygon(xz(entrance.world_position), &polygon));
    }

    #[test]
    fn polygon_placement_rejects_overlapping_rotated_footprint() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut geometry = WorldGeometry::default();
        let definition = BuildingKind::Storage.definition();
        let first = footprint_polygon(BuildingKind::Storage, Vec3::ZERO, definition.size, 0.37);
        let overlapping = footprint_polygon(
            BuildingKind::House,
            Vec3::new(0.4, 0.0, 0.2),
            BuildingKind::House.definition().size,
            -0.2,
        );

        geometry.occupy_polygon(first, entity, false);

        assert_eq!(
            geometry.placement_issue_for_polygon(&overlapping, None, true),
            Some(PlacementIssue::Occupied)
        );
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
