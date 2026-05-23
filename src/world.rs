use bevy::prelude::*;

use crate::{
    building::{WorldGeometry, resource_obstacle_polygon},
    camera,
    colonist::{Colonist, ColonistState},
    terrain::{
        GeneratedResource, GeneratedTerrain, TERRAIN_TILE_CELLS, TerrainGenerationConfig,
        TerrainKind, TerrainSeed, generate_terrain, terrain_height,
    },
    types::{BuildingKind, CELL_SIZE, ResourceKind, building_color},
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
    }
}

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
    pub colonist_mesh: Handle<Mesh>,
    pub colonist_material: Handle<StandardMaterial>,
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
    terrain_seed: Res<TerrainSeed>,
    mut geometry: ResMut<WorldGeometry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube_mesh = meshes.add(Cuboid::from_length(1.0));
    let colonist_mesh = meshes.add(Cuboid::new(0.32, 0.64, 0.32));

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
        colonist_mesh: colonist_mesh.clone(),
        colonist_material: colonist_material.clone(),
    };
    commands.insert_resource(assets);

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
        terrain_seed.0,
    );
    spawn_resource_nodes(
        &mut commands,
        &mut geometry,
        &cube_mesh,
        &tree_material,
        &food_material,
        &terrain.resources,
    );
    spawn_colonists(&mut commands, &colonist_mesh, &colonist_material, terrain_seed.0);
    camera::spawn_camera(&mut commands, terrain_seed.0);
}

fn spawn_terrain_tiles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    grass_material: &Handle<StandardMaterial>,
    forest_floor_material: &Handle<StandardMaterial>,
    forage_field_material: &Handle<StandardMaterial>,
    terrain: &GeneratedTerrain,
    seed: u64,
) {
    let tile_size = TERRAIN_TILE_CELLS as f32 * CELL_SIZE;
    let subdivisions = 4;

    for tile in &terrain.tiles {
        let material = match tile.kind {
            TerrainKind::Grass => grass_material.clone(),
            TerrainKind::ForestFloor => forest_floor_material.clone(),
            TerrainKind::ForageField => forage_field_material.clone(),
        };

        let mesh = heightfield_tile_mesh(seed, tile.center.x, tile.center.z, tile_size, subdivisions);
        let mesh_handle = meshes.add(mesh);

        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::new(tile.center.x, 0.0, tile.center.z)),
        ));
    }
}

fn heightfield_tile_mesh(
    seed: u64,
    center_x: f32,
    center_z: f32,
    tile_size: f32,
    subdivisions: u32,
) -> Mesh {
    let half = tile_size * 0.5;
    let min_x = center_x - half;
    let min_z = center_z - half;
    let verts_per_side = subdivisions + 2;
    let cell_size = tile_size / (verts_per_side - 1) as f32;

    let mut positions = Vec::with_capacity((verts_per_side * verts_per_side) as usize);
    let mut normals = Vec::with_capacity((verts_per_side * verts_per_side) as usize);
    let mut uvs = Vec::with_capacity((verts_per_side * verts_per_side) as usize);
    let mut indices = Vec::new();

    for iz in 0..verts_per_side {
        for ix in 0..verts_per_side {
            let x = min_x + ix as f32 * cell_size;
            let z = min_z + iz as f32 * cell_size;
            let y = terrain_height(seed, x, z);

            let u = ix as f32 / (verts_per_side - 1) as f32;
            let v = iz as f32 / (verts_per_side - 1) as f32;

            positions.push([x - center_x, y, z - center_z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([u, v]);
        }
    }

    for iz in 0..(verts_per_side - 1) {
        for ix in 0..(verts_per_side - 1) {
            let a = iz * verts_per_side + ix;
            let b = a + 1;
            let c = a + verts_per_side;
            let d = c + 1;

            indices.push(a);
            indices.push(c);
            indices.push(b);

            indices.push(b);
            indices.push(c);
            indices.push(d);
        }
    }

    // Compute normals
    compute_flat_normals(&mut normals, &positions, &indices);

    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

fn compute_flat_normals(normals: &mut [[f32; 3]], positions: &[[f32; 3]], indices: &[u32]) {
    for normal in normals.iter_mut() {
        *normal = [0.0, 0.0, 0.0];
    }

    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let a = Vec3::from_array(positions[tri[0] as usize]);
        let b = Vec3::from_array(positions[tri[1] as usize]);
        let c = Vec3::from_array(positions[tri[2] as usize]);
        let face_normal = (b - a).cross(c - a).normalize_or_zero();
        for &idx in &[tri[0], tri[1], tri[2]] {
            let n = &mut normals[idx as usize];
            n[0] += face_normal.x;
            n[1] += face_normal.y;
            n[2] += face_normal.z;
        }
    }

    for normal in normals.iter_mut() {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len > 0.0001 {
            normal[0] /= len;
            normal[1] /= len;
            normal[2] /= len;
        } else {
            normal[1] = 1.0;
        }
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
        let (material, y_offset, scale) = match resource.kind {
            ResourceKind::Wood => (tree_material.clone(), 0.65, Vec3::new(0.8, 1.3, 0.8)),
            ResourceKind::Food => (food_material.clone(), 0.25, Vec3::new(0.8, 0.5, 0.8)),
        };
        let position = Vec3::new(
            resource.position.x,
            resource.position.y + y_offset,
            resource.position.z,
        );
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
    seed: u64,
) {
    for (index, (x, z)) in [(-1.2, 1.0), (0.0, 1.2), (1.2, 1.0)]
        .into_iter()
        .enumerate()
    {
        let y = terrain_height(seed, x, z) + 0.32;
        commands.spawn((
            Mesh3d(colonist_mesh.clone()),
            MeshMaterial3d(colonist_material.clone()),
            Transform::from_translation(Vec3::new(x, y, z)),
            Colonist {
                name: format!("Settler {}", index + 1),
                state: ColonistState::Idle,
                speed: 2.2,
                path_rebuild_timer: 0.0,
            },
        ));
    }
}
