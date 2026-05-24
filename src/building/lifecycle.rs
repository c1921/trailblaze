use bevy::prelude::*;

use crate::{
    resources::{HOUSE_FOOD_CAPACITY, Inventory, PublicInventory, STORAGE_CAPACITY},
    types::{BuildingKind, ResourceKind},
    world::GameAssets,
};

use super::{
    Blueprint, BuildingEntrance, BuildingVisual, CompletedBuilding, EntranceMarker,
    EntrancePreview, Footprint, Housing,
};

pub(super) const ENTRANCE_MARKER_SCALE: Vec3 = Vec3::new(0.42, 0.08, 0.42);

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
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<Blueprint>();
        entity_commands.insert(CompletedBuilding {
            kind: blueprint.kind,
        });
        match blueprint.kind {
            BuildingKind::House => {
                entity_commands.insert((
                    Inventory::home_food(HOUSE_FOOD_CAPACITY),
                    Housing::default(),
                ));
            }
            BuildingKind::Storage => {
                let mut inventory = Inventory::public(STORAGE_CAPACITY);
                inventory.add(ResourceKind::Wood, 4);
                entity_commands.insert((inventory, PublicInventory));
            }
            BuildingKind::Woodcutter | BuildingKind::Gatherer | BuildingKind::Road => {}
        }
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

pub(super) fn sync_building_visual(
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

pub(super) fn spawn_building_visual(
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

pub(super) fn spawn_entrance_marker(
    commands: &mut Commands,
    assets: &GameAssets,
    owner: Entity,
    local_offset: Vec3,
) {
    commands.spawn((
        Mesh3d(assets.cube_mesh.clone()),
        MeshMaterial3d(assets.entrance_material.clone()),
        Transform::from_translation(entrance_marker_translation(local_offset))
            .with_scale(ENTRANCE_MARKER_SCALE),
        EntranceMarker { owner },
        ChildOf(owner),
    ));
}

pub(super) fn spawn_entrance_preview(
    commands: &mut Commands,
    assets: &GameAssets,
    parent: Entity,
    local_offset: Vec3,
) -> Entity {
    commands
        .spawn((
            Mesh3d(assets.cube_mesh.clone()),
            MeshMaterial3d(assets.entrance_material.clone()),
            Transform::from_translation(entrance_marker_translation(local_offset))
                .with_scale(ENTRANCE_MARKER_SCALE),
            EntrancePreview,
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn entrance_marker_translation(local_offset: Vec3) -> Vec3 {
    Vec3::new(local_offset.x, 0.04, local_offset.z)
}

pub(super) fn visual_translation(height: f32) -> Vec3 {
    Vec3::new(0.0, height * 0.5, 0.0)
}
