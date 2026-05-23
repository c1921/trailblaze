use bevy::prelude::*;

use crate::{
    building::{WorldGeometry, resource_obstacle_polygon},
    camera,
    colonist::{Colonist, ColonistState},
    terrain::{
        GeneratedResource, GeneratedTerrain, TERRAIN_TILE_CELLS, TerrainGenerationConfig,
        TerrainKind, generate_terrain,
    },
    types::{BuildingKind, CELL_SIZE, MAP_PLANE_SIZE, ResourceKind, building_color},
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
    }
}

#[derive(Component)]
pub struct Ground;

#[derive(Component)]
pub struct ResourceNode {
    pub kind: ResourceKind,
    pub amount: i32,
}

#[derive(Resource, Clone)]
pub struct GameAssets {
    pub cube_mesh: Handle<Mesh>,
    pub preview_valid_material: Handle<StandardMaterial>,
    pub preview_invalid_material: Handle<StandardMaterial>,
    pub blueprint_material: Handle<StandardMaterial>,
    pub house_material: Handle<StandardMaterial>,
    pub storage_material: Handle<StandardMaterial>,
    pub woodcutter_material: Handle<StandardMaterial>,
    pub gatherer_material: Handle<StandardMaterial>,
    pub road_material: Handle<StandardMaterial>,
    pub entrance_material: Handle<StandardMaterial>,
}

impl GameAssets {
    pub fn building_material(&self, kind: BuildingKind) -> Handle<StandardMaterial> {
        match kind {
            BuildingKind::House => self.house_material.clone(),
            BuildingKind::Storage => self.storage_material.clone(),
            BuildingKind::Woodcutter => self.woodcutter_material.clone(),
            BuildingKind::Gatherer => self.gatherer_material.clone(),
            BuildingKind::Road => self.road_material.clone(),
        }
    }
}

pub fn setup_scene(
    mut commands: Commands,
    terrain_config: Res<TerrainGenerationConfig>,
    mut geometry: ResMut<WorldGeometry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube_mesh = meshes.add(Cuboid::from_length(1.0));
    let colonist_mesh = meshes.add(Cuboid::new(0.32, 0.64, 0.32));

    let ground_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.58, 0.62, 0.58),
        perceptual_roughness: 0.85,
        ..default()
    });
    let tree_material = materials.add(Color::srgb(0.16, 0.38, 0.16));
    let food_material = materials.add(Color::srgb(0.66, 0.12, 0.18));
    let colonist_material = materials.add(Color::srgb(0.92, 0.72, 0.45));
    let grass_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.57, 0.45),
        perceptual_roughness: 0.9,
        ..default()
    });
    let forest_floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.23, 0.35, 0.22),
        perceptual_roughness: 0.95,
        ..default()
    });
    let forage_field_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.38, 0.5, 0.27),
        perceptual_roughness: 0.9,
        ..default()
    });
    let preview_valid_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.25, 0.85, 0.55, 0.45),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let preview_invalid_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.2, 0.16, 0.45),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let blueprint_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.48, 0.95, 0.55),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let assets = GameAssets {
        cube_mesh: cube_mesh.clone(),
        preview_valid_material,
        preview_invalid_material,
        blueprint_material,
        house_material: materials.add(building_color(BuildingKind::House)),
        storage_material: materials.add(building_color(BuildingKind::Storage)),
        woodcutter_material: materials.add(building_color(BuildingKind::Woodcutter)),
        gatherer_material: materials.add(building_color(BuildingKind::Gatherer)),
        road_material: materials.add(building_color(BuildingKind::Road)),
        entrance_material: materials.add(Color::srgb(0.95, 0.86, 0.28)),
    };
    commands.insert_resource(assets);

    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(MAP_PLANE_SIZE, MAP_PLANE_SIZE),
            ),
        ),
        MeshMaterial3d(ground_material),
        Ground,
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let terrain = generate_terrain(*terrain_config);
    spawn_terrain_tiles(
        &mut commands,
        &mut meshes,
        &grass_material,
        &forest_floor_material,
        &forage_field_material,
        &terrain,
    );
    spawn_resource_nodes(
        &mut commands,
        &mut geometry,
        &cube_mesh,
        &tree_material,
        &food_material,
        &terrain.resources,
    );
    spawn_colonists(&mut commands, &colonist_mesh, &colonist_material);
    camera::spawn_camera(&mut commands);
}

fn spawn_terrain_tiles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    grass_material: &Handle<StandardMaterial>,
    forest_floor_material: &Handle<StandardMaterial>,
    forage_field_material: &Handle<StandardMaterial>,
    terrain: &GeneratedTerrain,
) {
    let tile_size = TERRAIN_TILE_CELLS as f32 * CELL_SIZE;
    let tile_mesh = meshes.add(Plane3d::default().mesh().size(tile_size, tile_size));

    for tile in &terrain.tiles {
        let material = match tile.kind {
            TerrainKind::Grass => grass_material.clone(),
            TerrainKind::ForestFloor => forest_floor_material.clone(),
            TerrainKind::ForageField => forage_field_material.clone(),
        };
        commands.spawn((
            Mesh3d(tile_mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::new(tile.center.x, 0.006, tile.center.z)),
        ));
    }
}

fn spawn_resource_nodes(
    commands: &mut Commands,
    geometry: &mut WorldGeometry,
    cube_mesh: &Handle<Mesh>,
    tree_material: &Handle<StandardMaterial>,
    food_material: &Handle<StandardMaterial>,
    resources: &[GeneratedResource],
) {
    for resource in resources {
        let (material, y, scale) = match resource.kind {
            ResourceKind::Wood => (tree_material.clone(), 0.65, Vec3::new(0.8, 1.3, 0.8)),
            ResourceKind::Food => (food_material.clone(), 0.25, Vec3::new(0.8, 0.5, 0.8)),
        };
        let position = Vec3::new(resource.position.x, y, resource.position.z);
        let entity = commands
            .spawn((
                Mesh3d(cube_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(position).with_scale(scale),
                ResourceNode {
                    kind: resource.kind,
                    amount: resource.amount,
                },
            ))
            .id();
        geometry.occupy_polygon(resource_obstacle_polygon(position), entity, false);
    }
}

fn spawn_colonists(
    commands: &mut Commands,
    colonist_mesh: &Handle<Mesh>,
    colonist_material: &Handle<StandardMaterial>,
) {
    for (index, position) in [
        Vec3::new(-1.2, 0.32, 1.0),
        Vec3::new(0.0, 0.32, 1.2),
        Vec3::new(1.2, 0.32, 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            Mesh3d(colonist_mesh.clone()),
            MeshMaterial3d(colonist_material.clone()),
            Transform::from_translation(position),
            Colonist {
                name: format!("Settler {}", index + 1),
                state: ColonistState::Idle,
                speed: 2.2,
            },
        ));
    }
}
