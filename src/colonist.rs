use bevy::prelude::*;

use crate::{
    building::{Blueprint, BuildingEntrance, CompletedBuilding, Housing, WorldGeometry},
    math::xz_distance,
    navigation::{PathCache, line_of_sight_clear, path_to_waypoints},
    resources::{
        COLONIST_CARRY_CAPACITY, HOME_FOOD_PER_RESIDENT, Inventory, PublicInventory, carried_amount,
    },
    simulation::SimulationClock,
    terrain::{TerrainSeed, terrain_height},
    types::{BuildingKind, ResourceKind},
    world::ResourceNode,
};

const GATHER_SECONDS: f32 = 1.4;
const BUILD_RATE: f32 = 1.0;
const COLONIST_HALF_HEIGHT: f32 = 0.32;
const SATIETY_LOSS_SECONDS: f32 = 10.0;
const HUNGER_THRESHOLD: f32 = 50.0;
const EAT_RESTORE: f32 = 50.0;
const EAT_REST_SECONDS: f32 = 2.0;

pub struct ColonistPlugin;

impl Plugin for ColonistPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PathCache>().add_systems(
            Update,
            (
                tick_colonist_needs,
                assign_housing,
                assign_idle_colonists,
                update_colonists,
            )
                .chain(),
        );
    }
}

#[derive(Component, Debug)]
pub struct Colonist {
    pub name: String,
    pub state: ColonistState,
    pub speed: f32,
    pub path_rebuild_timer: f32,
    pub home: Option<Entity>,
    pub satiety: f32,
    pub carry_capacity: i32,
}

impl Colonist {
    pub fn status_label(&self) -> String {
        self.state.label()
    }

    pub fn is_hungry(&self) -> bool {
        self.satiety < HUNGER_THRESHOLD
    }
}

#[derive(Debug, Clone)]
pub enum ColonistState {
    Idle,
    Moving {
        target: Vec3,
        path: Vec<Vec3>,
        task: Task,
    },
    Gathering {
        resource: Entity,
        kind: ResourceKind,
        timer: f32,
    },
    Building {
        blueprint: Entity,
    },
    Eating {
        timer: f32,
    },
}

#[derive(Debug, Clone)]
pub enum Task {
    PickupMaterial {
        source: Entity,
        blueprint: Entity,
        kind: ResourceKind,
        amount: i32,
        delivery_target: Vec3,
    },
    DeliverMaterial {
        blueprint: Entity,
        kind: ResourceKind,
        amount: i32,
    },
    Build {
        blueprint: Entity,
    },
    Gather {
        resource: Entity,
        kind: ResourceKind,
    },
    Deposit {
        inventory: Entity,
        kind: ResourceKind,
        amount: i32,
    },
    Eat {
        inventory: Entity,
    },
    PickupHomeFood {
        source: Entity,
        home: Entity,
        amount: i32,
    },
    DeliverHomeFood {
        home: Entity,
        amount: i32,
    },
}

impl ColonistState {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Idle".to_string(),
            Self::Moving { task, .. } => format!("Moving: {}", task.label()),
            Self::Gathering { kind, .. } => format!("Gathering {}", kind.label()),
            Self::Building { .. } => "Building a blueprint".to_string(),
            Self::Eating { .. } => "Eating".to_string(),
        }
    }
}

impl Task {
    pub fn label(&self) -> String {
        match self {
            Self::PickupMaterial { kind, amount, .. } => {
                format!("Picking up {} {}", amount, kind.label())
            }
            Self::DeliverMaterial { kind, amount, .. } => {
                format!("Delivering {} {}", amount, kind.label())
            }
            Self::Build { .. } => "Going to build".to_string(),
            Self::Gather { kind, .. } => format!("Going to gather {}", kind.label()),
            Self::Deposit { kind, amount, .. } => {
                format!("Depositing {} {}", amount, kind.label())
            }
            Self::Eat { .. } => "Going to eat".to_string(),
            Self::PickupHomeFood { amount, .. } => format!("Picking up {} Food for home", amount),
            Self::DeliverHomeFood { amount, .. } => {
                format!("Delivering {} Food home", amount)
            }
        }
    }
}

pub fn tick_colonist_needs(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<SimulationClock>,
    mut colonists: Query<(Entity, &mut Colonist)>,
    mut homes: Query<&mut Housing>,
) {
    let dt = clock.scaled_delta(&time);
    if dt == 0.0 {
        return;
    }

    for (entity, mut colonist) in &mut colonists {
        colonist.satiety = decayed_satiety(colonist.satiety, dt);
        if colonist.satiety > 0.0 {
            continue;
        }

        if let Some(home) = colonist.home.take() {
            if let Ok(mut housing) = homes.get_mut(home) {
                housing.remove_resident(entity);
            }
        }
        commands.entity(entity).despawn();
    }
}

pub fn assign_housing(
    mut colonists: Query<(Entity, &mut Colonist, &Transform)>,
    mut homes: Query<(Entity, &mut Housing, &Transform, &CompletedBuilding), Without<Colonist>>,
) {
    let mut snapshots: Vec<(Entity, usize, Vec3)> = homes
        .iter()
        .filter(|(_, _, _, building)| building.kind == BuildingKind::House)
        .map(|(entity, housing, transform, _)| {
            (entity, housing.resident_count(), transform.translation)
        })
        .collect();

    for (colonist_entity, mut colonist, transform) in &mut colonists {
        if colonist.satiety <= 0.0 {
            continue;
        }

        if let Some(home) = colonist.home {
            if snapshots.iter().any(|(entity, _, _)| *entity == home) {
                continue;
            }
            colonist.home = None;
        }

        let Some(index) = best_home_candidate(&snapshots, transform.translation) else {
            continue;
        };
        let home = snapshots[index].0;

        if let Ok((_, mut housing, _, _)) = homes.get_mut(home) {
            if housing.add_resident(colonist_entity) {
                colonist.home = Some(home);
                snapshots[index].1 += 1;
            }
        }
    }
}

pub fn assign_idle_colonists(
    geometry: Res<WorldGeometry>,
    terrain_seed: Res<TerrainSeed>,
    mut cache: ResMut<PathCache>,
    mut colonists: Query<(&mut Colonist, &Transform)>,
    blueprints: Query<(Entity, &Blueprint, &Transform, Option<&BuildingEntrance>)>,
    resources: Query<(Entity, &ResourceNode, &Transform)>,
    completed: Query<(&CompletedBuilding, &Transform, Option<&BuildingEntrance>)>,
    inventories: Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
) {
    let seed = terrain_seed.0;
    let mut reserved_deliveries = active_material_deliveries(&colonists);
    let has_woodcutter = completed
        .iter()
        .any(|(building, _, _)| building.kind == BuildingKind::Woodcutter);
    let has_gatherer = completed
        .iter()
        .any(|(building, _, _)| building.kind == BuildingKind::Gatherer);

    for (mut colonist, transform) in &mut colonists {
        if colonist.satiety <= 0.0 {
            continue;
        }

        if !matches!(colonist.state, ColonistState::Idle) {
            continue;
        }

        let start = transform.translation;

        if colonist.is_hungry() {
            if let Some(state) =
                eat_candidate(&geometry, &mut *cache, start, &colonist, &inventories, seed)
            {
                colonist.state = state;
                continue;
            }
        }

        let mut assigned = false;
        if let Some((blueprint, amount, state)) = material_delivery_candidate(
            &geometry,
            &mut *cache,
            start,
            &colonist,
            &blueprints,
            &inventories,
            &reserved_deliveries,
            seed,
        ) {
            reserved_deliveries.push((blueprint, amount));
            colonist.state = state;
            assigned = true;
        }
        if assigned {
            continue;
        }

        if let Some(state) = build_candidate(&geometry, &mut *cache, start, &blueprints, seed) {
            colonist.state = state;
            continue;
        }

        if let Some(state) =
            home_restock_candidate(&geometry, &mut *cache, start, &colonist, &inventories, seed)
        {
            colonist.state = state;
            continue;
        }

        if has_woodcutter {
            if let Some((_, state)) = gather_candidate(
                &geometry,
                &mut *cache,
                start,
                ResourceKind::Wood,
                &resources,
                &inventories,
                seed,
            ) {
                colonist.state = state;
                continue;
            }
        }

        if has_gatherer {
            if let Some((_, state)) = gather_candidate(
                &geometry,
                &mut *cache,
                start,
                ResourceKind::Food,
                &resources,
                &inventories,
                seed,
            ) {
                colonist.state = state;
            }
        }
    }
}

fn active_material_deliveries(
    colonists: &Query<(&mut Colonist, &Transform)>,
) -> Vec<(Entity, i32)> {
    colonists
        .iter()
        .filter_map(|(colonist, _)| match &colonist.state {
            ColonistState::Moving {
                task:
                    Task::PickupMaterial {
                        blueprint,
                        kind: ResourceKind::Wood,
                        amount,
                        ..
                    }
                    | Task::DeliverMaterial {
                        blueprint,
                        kind: ResourceKind::Wood,
                        amount,
                    },
                ..
            } => Some((*blueprint, *amount)),
            _ => None,
        })
        .collect()
}

fn eat_candidate(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    colonist: &Colonist,
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    seed: u64,
) -> Option<ColonistState> {
    if let Some(home) = colonist.home {
        if let Ok((_, inventory, transform, entrance, _, _, _)) = inventories.get(home) {
            if inventory.amount(ResourceKind::Food) > 0 {
                let target = building_interaction_position(transform, entrance);
                if let Some((state, _)) = moving_state_to_target(
                    geometry,
                    cache,
                    start,
                    target,
                    Task::Eat { inventory: home },
                    seed,
                ) {
                    return Some(state);
                }
            }
        }
    }

    let mut candidates: Vec<(f32, Entity, Vec3)> = inventories
        .iter()
        .filter(|(_, inventory, _, _, public, _, _)| {
            public.is_some() && inventory.amount(ResourceKind::Food) > 0
        })
        .map(|(entity, _, transform, entrance, _, _, _)| {
            let target = building_interaction_position(transform, entrance);
            (xz_distance(start, target), entity, target)
        })
        .collect();
    candidates.sort_by(|(dist_a, ..), (dist_b, ..)| dist_a.total_cmp(dist_b));

    for (_, inventory, target) in candidates {
        if let Some((state, _)) = moving_state_to_target(
            geometry,
            cache,
            start,
            target,
            Task::Eat { inventory },
            seed,
        ) {
            return Some(state);
        }
    }

    None
}

fn material_delivery_candidate(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    colonist: &Colonist,
    blueprints: &Query<(Entity, &Blueprint, &Transform, Option<&BuildingEntrance>)>,
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    reserved_deliveries: &[(Entity, i32)],
    seed: u64,
) -> Option<(Entity, i32, ColonistState)> {
    let mut candidates: Vec<(i32, f32, Entity, i32, Vec3, Task)> = Vec::new();

    for (blueprint_entity, blueprint, blueprint_transform, blueprint_entrance) in blueprints {
        let reserved = reserved_deliveries
            .iter()
            .filter(|(target, _)| *target == blueprint_entity)
            .map(|(_, wood)| *wood)
            .sum::<i32>();
        let remaining = blueprint.needs_wood() - reserved;
        if remaining <= 0 {
            continue;
        }

        let amount = carried_amount(ResourceKind::Wood, colonist.carry_capacity).min(remaining);
        let blueprint_target =
            building_interaction_position(blueprint_transform, blueprint_entrance);

        for (source, inventory, source_transform, source_entrance, public, _, _) in inventories {
            if public.is_none() || inventory.amount(ResourceKind::Wood) < amount {
                continue;
            }

            let source_target = building_interaction_position(source_transform, source_entrance);
            let distance =
                xz_distance(start, source_target) + xz_distance(source_target, blueprint_target);
            candidates.push((
                remaining,
                distance,
                blueprint_entity,
                amount,
                source_target,
                Task::PickupMaterial {
                    source,
                    blueprint: blueprint_entity,
                    kind: ResourceKind::Wood,
                    amount,
                    delivery_target: blueprint_target,
                },
            ));
        }
    }

    candidates.sort_by(|(rem_a, dist_a, ..), (rem_b, dist_b, ..)| {
        rem_b.cmp(rem_a).then_with(|| dist_a.total_cmp(dist_b))
    });

    for (_, _, blueprint, amount, target, task) in candidates {
        if let Some((state, _)) = moving_state_to_target(geometry, cache, start, target, task, seed)
        {
            return Some((blueprint, amount, state));
        }
    }

    None
}

fn build_candidate(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    blueprints: &Query<(Entity, &Blueprint, &Transform, Option<&BuildingEntrance>)>,
    seed: u64,
) -> Option<ColonistState> {
    let mut candidates: Vec<(f32, Vec3, Task)> = blueprints
        .iter()
        .filter(|(_, blueprint, _, _)| {
            blueprint.has_materials() && blueprint.progress < blueprint.build_seconds
        })
        .map(|(entity, _, blueprint_transform, entrance)| {
            let target = building_interaction_position(blueprint_transform, entrance);
            (
                xz_distance(start, target),
                target,
                Task::Build { blueprint: entity },
            )
        })
        .collect();

    candidates.sort_by(|(dist_a, ..), (dist_b, ..)| dist_a.total_cmp(dist_b));

    for (_, target, task) in candidates {
        if let Some((state, _)) = moving_state_to_target(geometry, cache, start, target, task, seed)
        {
            return Some(state);
        }
    }

    None
}

fn home_restock_candidate(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    colonist: &Colonist,
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    seed: u64,
) -> Option<ColonistState> {
    let home = colonist.home?;
    let Ok((_, home_inventory, _, _, _, home_housing, _)) = inventories.get(home) else {
        return None;
    };
    let housing = home_housing?;
    let target_food = (housing.resident_count() as i32 * HOME_FOOD_PER_RESIDENT).min(
        home_inventory.max_addable(ResourceKind::Food) + home_inventory.amount(ResourceKind::Food),
    );
    let need = (target_food - home_inventory.amount(ResourceKind::Food))
        .min(home_inventory.max_addable(ResourceKind::Food));
    if need <= 0 {
        return None;
    }

    let mut candidates: Vec<(f32, Entity, i32, Vec3)> = inventories
        .iter()
        .filter_map(|(source, inventory, transform, entrance, public, _, _)| {
            if public.is_none() {
                return None;
            }

            let amount = carried_amount(ResourceKind::Food, colonist.carry_capacity)
                .min(need)
                .min(inventory.amount(ResourceKind::Food));
            if amount <= 0 {
                return None;
            }

            let target = building_interaction_position(transform, entrance);
            Some((xz_distance(start, target), source, amount, target))
        })
        .collect();
    candidates.sort_by(|(dist_a, ..), (dist_b, ..)| dist_a.total_cmp(dist_b));

    for (_, source, amount, target) in candidates {
        let task = Task::PickupHomeFood {
            source,
            home,
            amount,
        };
        if let Some((state, _)) = moving_state_to_target(geometry, cache, start, target, task, seed)
        {
            return Some(state);
        }
    }

    None
}

fn gather_candidate(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    kind: ResourceKind,
    resources: &Query<(Entity, &ResourceNode, &Transform)>,
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    seed: u64,
) -> Option<(usize, ColonistState)> {
    if total_public_addable(inventories, kind) <= 0 {
        return None;
    }

    let mut candidates: Vec<(f32, Entity, Vec3)> = resources
        .iter()
        .filter(|(_, node, _)| node.kind == kind && node.amount > 0)
        .map(|(resource, _, transform)| {
            (
                xz_distance(start, transform.translation),
                resource,
                transform.translation,
            )
        })
        .collect();

    candidates.sort_by(|(dist_a, ..), (dist_b, ..)| dist_a.total_cmp(dist_b));

    for (_, resource, pos) in candidates {
        if let Some(result) = movement_to_resource(
            geometry,
            cache,
            start,
            pos,
            Task::Gather { resource, kind },
            seed,
        ) {
            return Some(result);
        }
    }
    None
}

fn total_public_addable(
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    kind: ResourceKind,
) -> i32 {
    inventories
        .iter()
        .filter(|(_, _, _, _, public, _, _)| public.is_some())
        .map(|(_, inventory, _, _, _, _, _)| inventory.max_addable(kind))
        .sum()
}

fn building_interaction_position(
    transform: &Transform,
    entrance: Option<&BuildingEntrance>,
) -> Vec3 {
    entrance
        .map(|entrance| entrance.world_position)
        .unwrap_or(transform.translation)
}

fn moving_state_to_target(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    target: Vec3,
    task: Task,
    seed: u64,
) -> Option<(ColonistState, usize)> {
    let path = path_to_waypoints(geometry, cache, start, target, seed)?;
    let path_len = path.len();

    Some((ColonistState::Moving { target, path, task }, path_len))
}

fn movement_to_resource(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    resource_position: Vec3,
    task: Task,
    seed: u64,
) -> Option<(usize, ColonistState)> {
    let mut targets: Vec<(f32, Vec3)> = resource_interaction_targets(resource_position, seed)
        .into_iter()
        .filter(|target| geometry.is_walkable_point(*target))
        .map(|target| (xz_distance(start, target), target))
        .collect();

    targets.sort_by(|(dist_a, ..), (dist_b, ..)| dist_a.total_cmp(dist_b));

    for (_, target) in targets {
        if let Some((state, path_len)) =
            moving_state_to_target(geometry, cache, start, target, task.clone(), seed)
        {
            return Some((path_len, state));
        }
    }
    None
}

fn resource_interaction_targets(resource_position: Vec3, seed: u64) -> [Vec3; 8] {
    const RADIUS: f32 = 0.82;
    const DIAGONAL: f32 = RADIUS * 0.70710677;
    let base_x = resource_position.x;
    let base_z = resource_position.z;
    [
        Vec3::new(
            base_x + RADIUS,
            terrain_height(seed, base_x + RADIUS, base_z),
            base_z,
        ),
        Vec3::new(
            base_x - RADIUS,
            terrain_height(seed, base_x - RADIUS, base_z),
            base_z,
        ),
        Vec3::new(
            base_x,
            terrain_height(seed, base_x, base_z + RADIUS),
            base_z + RADIUS,
        ),
        Vec3::new(
            base_x,
            terrain_height(seed, base_x, base_z - RADIUS),
            base_z - RADIUS,
        ),
        Vec3::new(
            base_x + DIAGONAL,
            terrain_height(seed, base_x + DIAGONAL, base_z + DIAGONAL),
            base_z + DIAGONAL,
        ),
        Vec3::new(
            base_x - DIAGONAL,
            terrain_height(seed, base_x - DIAGONAL, base_z + DIAGONAL),
            base_z + DIAGONAL,
        ),
        Vec3::new(
            base_x + DIAGONAL,
            terrain_height(seed, base_x + DIAGONAL, base_z - DIAGONAL),
            base_z - DIAGONAL,
        ),
        Vec3::new(
            base_x - DIAGONAL,
            terrain_height(seed, base_x - DIAGONAL, base_z - DIAGONAL),
            base_z - DIAGONAL,
        ),
    ]
}

fn deposit_moving_state(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    inventories: &Query<
        (
            Entity,
            &Inventory,
            &Transform,
            Option<&BuildingEntrance>,
            Option<&PublicInventory>,
            Option<&Housing>,
            Option<&CompletedBuilding>,
        ),
        Without<Colonist>,
    >,
    kind: ResourceKind,
    amount: i32,
    seed: u64,
    exclude: Option<Entity>,
) -> Option<ColonistState> {
    let mut candidates: Vec<(bool, f32, Entity, Vec3)> = inventories
        .iter()
        .filter(|(entity, inventory, _, _, public, _, _)| {
            public.is_some() && Some(*entity) != exclude && inventory.max_addable(kind) > 0
        })
        .map(|(entity, inventory, transform, entrance, _, _, _)| {
            let target = building_interaction_position(transform, entrance);
            (
                inventory.max_addable(kind) >= amount,
                xz_distance(start, target),
                entity,
                target,
            )
        })
        .collect();
    candidates.sort_by(|(full_a, dist_a, ..), (full_b, dist_b, ..)| {
        full_b.cmp(full_a).then_with(|| dist_a.total_cmp(dist_b))
    });

    for (_, _, inventory, target) in candidates {
        let task = Task::Deposit {
            inventory,
            kind,
            amount,
        };
        if let Some((state, _)) = moving_state_to_target(geometry, cache, start, target, task, seed)
        {
            return Some(state);
        }
    }

    None
}

pub fn update_colonists(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<SimulationClock>,
    terrain_seed: Res<TerrainSeed>,
    mut geometry: ResMut<WorldGeometry>,
    mut cache: ResMut<PathCache>,
    mut colonists: Query<(&mut Colonist, &mut Transform)>,
    mut blueprints: Query<(Entity, &mut Blueprint)>,
    mut resources: Query<&mut ResourceNode>,
    mut inventory_access: ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
) {
    let seed = terrain_seed.0;
    let dt = clock.scaled_delta(&time);
    if dt == 0.0 {
        return;
    }
    cache.clear();

    for (mut colonist, mut transform) in &mut colonists {
        if colonist.satiety <= 0.0 {
            colonist.state = ColonistState::Idle;
            continue;
        }

        let current_state = std::mem::replace(&mut colonist.state, ColonistState::Idle);
        match current_state {
            ColonistState::Idle => {}
            ColonistState::Eating { mut timer } => {
                timer -= dt;
                if timer > 0.0 {
                    colonist.state = ColonistState::Eating { timer };
                } else {
                    colonist.state = ColonistState::Idle;
                }
            }
            ColonistState::Moving {
                target,
                mut path,
                task,
            } => {
                let path_blocked = if !path.is_empty() {
                    !line_of_sight_clear(&geometry, transform.translation, path[0])
                } else {
                    false
                };

                let needs_rebuild = if path_blocked
                    || (colonist.path_rebuild_timer <= 0.0
                        && path_needs_rebuild(&path, transform.translation, target, &geometry))
                {
                    colonist.path_rebuild_timer = 0.25;
                    true
                } else {
                    colonist.path_rebuild_timer -= dt;
                    false
                };

                if needs_rebuild {
                    if let Some((state, _)) = moving_state_to_target(
                        &geometry,
                        &mut *cache,
                        transform.translation,
                        target,
                        task.clone(),
                        seed,
                    ) {
                        if let ColonistState::Moving {
                            path: rebuilt_path, ..
                        } = state
                        {
                            path = rebuilt_path;
                        }
                    } else {
                        colonist.state =
                            unreachable_moving_state(target, task, &mut inventory_access);
                        continue;
                    }
                }

                if move_along_path(&mut transform, target, &mut path, colonist.speed, dt, seed) {
                    colonist.state = complete_movement_task(
                        &mut geometry,
                        &mut cache,
                        &mut inventory_access,
                        &mut blueprints,
                        &mut colonist,
                        transform.translation,
                        task,
                        seed,
                    );
                } else {
                    colonist.state = ColonistState::Moving { target, path, task };
                }
            }
            ColonistState::Gathering {
                resource,
                kind,
                mut timer,
            } => {
                timer += dt;
                if timer < GATHER_SECONDS {
                    colonist.state = ColonistState::Gathering {
                        resource,
                        kind,
                        timer,
                    };
                    continue;
                }

                let public_capacity = {
                    let inventories = inventory_access.p0();
                    total_public_addable(&inventories, kind)
                };
                if public_capacity <= 0 {
                    colonist.state = ColonistState::Idle;
                    continue;
                }

                let mut amount = 0;
                if let Ok(mut node) = resources.get_mut(resource) {
                    amount = carried_amount(kind, colonist.carry_capacity)
                        .min(node.amount)
                        .min(public_capacity);
                    node.amount -= amount;
                    if amount > 0 && node.amount <= 0 {
                        geometry.release_entity(resource);
                        commands.entity(resource).despawn();
                    }
                }

                if amount > 0 {
                    let inventories = inventory_access.p0();
                    colonist.state = deposit_moving_state(
                        &geometry,
                        &mut *cache,
                        transform.translation,
                        &inventories,
                        kind,
                        amount,
                        seed,
                        None,
                    )
                    .unwrap_or(ColonistState::Idle);
                } else {
                    colonist.state = ColonistState::Idle;
                }
            }
            ColonistState::Building {
                blueprint: blueprint_entity,
            } => {
                if let Ok((_, mut blueprint)) = blueprints.get_mut(blueprint_entity) {
                    if blueprint.has_materials() {
                        blueprint.progress =
                            (blueprint.progress + BUILD_RATE * dt).min(blueprint.build_seconds);
                    }
                    if blueprint.is_complete() {
                        colonist.state = ColonistState::Idle;
                    } else {
                        colonist.state = ColonistState::Building {
                            blueprint: blueprint_entity,
                        };
                    }
                } else {
                    colonist.state = ColonistState::Idle;
                }
            }
        }
    }
}

fn complete_movement_task(
    geometry: &mut WorldGeometry,
    cache: &mut PathCache,
    inventory_access: &mut ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
    blueprints: &mut Query<(Entity, &mut Blueprint)>,
    colonist: &mut Colonist,
    position: Vec3,
    task: Task,
    seed: u64,
) -> ColonistState {
    match task {
        Task::PickupMaterial {
            source,
            blueprint,
            kind,
            amount,
            delivery_target,
        } => {
            let removed = remove_from_inventory(inventory_access, source, kind, amount);
            if !removed {
                return ColonistState::Idle;
            }

            if blueprints.contains(blueprint) {
                moving_state_to_target(
                    geometry,
                    cache,
                    position,
                    delivery_target,
                    Task::DeliverMaterial {
                        blueprint,
                        kind,
                        amount,
                    },
                    seed,
                )
                .map(|(state, _)| state)
                .unwrap_or_else(|| {
                    add_to_nearest_public(inventory_access, position, kind, amount);
                    ColonistState::Idle
                })
            } else {
                add_to_nearest_public(inventory_access, position, kind, amount);
                ColonistState::Idle
            }
        }
        Task::DeliverMaterial {
            blueprint,
            kind,
            amount,
        } => {
            if let Ok((_, mut blueprint)) = blueprints.get_mut(blueprint) {
                if kind == ResourceKind::Wood {
                    blueprint.delivered_wood =
                        (blueprint.delivered_wood + amount).min(blueprint.required_wood);
                }
            } else {
                add_to_nearest_public(inventory_access, position, kind, amount);
            }
            ColonistState::Idle
        }
        Task::Build { blueprint } => ColonistState::Building { blueprint },
        Task::Gather { resource, kind } => ColonistState::Gathering {
            resource,
            kind,
            timer: 0.0,
        },
        Task::Deposit {
            inventory,
            kind,
            amount,
        } => {
            let leftover = add_to_inventory(inventory_access, inventory, kind, amount);
            if leftover > 0 {
                let inventories = inventory_access.p0();
                deposit_moving_state(
                    geometry,
                    cache,
                    position,
                    &inventories,
                    kind,
                    leftover,
                    seed,
                    Some(inventory),
                )
                .unwrap_or(ColonistState::Idle)
            } else {
                ColonistState::Idle
            }
        }
        Task::Eat { inventory } => {
            if remove_from_inventory(inventory_access, inventory, ResourceKind::Food, 1) {
                colonist.satiety = restored_satiety(colonist.satiety);
                ColonistState::Eating {
                    timer: EAT_REST_SECONDS,
                }
            } else {
                ColonistState::Idle
            }
        }
        Task::PickupHomeFood {
            source,
            home,
            amount,
        } => {
            if !remove_from_inventory(inventory_access, source, ResourceKind::Food, amount) {
                return ColonistState::Idle;
            }

            let target = {
                let inventories = inventory_access.p0();
                inventories
                    .get(home)
                    .ok()
                    .map(|(_, _, transform, entrance, _, _, _)| {
                        building_interaction_position(transform, entrance)
                    })
            };

            if let Some(target) = target {
                moving_state_to_target(
                    geometry,
                    cache,
                    position,
                    target,
                    Task::DeliverHomeFood { home, amount },
                    seed,
                )
                .map(|(state, _)| state)
                .unwrap_or_else(|| {
                    add_to_nearest_public(inventory_access, position, ResourceKind::Food, amount);
                    ColonistState::Idle
                })
            } else {
                add_to_nearest_public(inventory_access, position, ResourceKind::Food, amount);
                ColonistState::Idle
            }
        }
        Task::DeliverHomeFood { home, amount } => {
            let leftover = add_to_inventory(inventory_access, home, ResourceKind::Food, amount);
            if leftover > 0 {
                add_to_nearest_public(inventory_access, position, ResourceKind::Food, leftover);
            }
            ColonistState::Idle
        }
    }
}

fn remove_from_inventory(
    inventory_access: &mut ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
    inventory: Entity,
    kind: ResourceKind,
    amount: i32,
) -> bool {
    inventory_access
        .p1()
        .get_mut(inventory)
        .map(|mut inventory| inventory.remove(kind, amount))
        .unwrap_or(false)
}

fn add_to_inventory(
    inventory_access: &mut ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
    inventory: Entity,
    kind: ResourceKind,
    amount: i32,
) -> i32 {
    let accepted = inventory_access
        .p1()
        .get_mut(inventory)
        .map(|mut inventory| inventory.add_partial(kind, amount))
        .unwrap_or(0);
    amount - accepted
}

fn add_to_nearest_public(
    inventory_access: &mut ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
    position: Vec3,
    kind: ResourceKind,
    amount: i32,
) -> i32 {
    let mut targets: Vec<(f32, Entity)> = {
        let inventories = inventory_access.p0();
        inventories
            .iter()
            .filter(|(_, inventory, _, _, public, _, _)| {
                public.is_some() && inventory.max_addable(kind) > 0
            })
            .map(|(entity, _, transform, entrance, _, _, _)| {
                (
                    xz_distance(position, building_interaction_position(transform, entrance)),
                    entity,
                )
            })
            .collect()
    };
    targets.sort_by(|(dist_a, _), (dist_b, _)| dist_a.total_cmp(dist_b));

    let mut remaining = amount;
    for (_, target) in targets {
        if remaining <= 0 {
            break;
        }

        let accepted = inventory_access
            .p1()
            .get_mut(target)
            .map(|mut inventory| inventory.add_partial(kind, remaining))
            .unwrap_or(0);
        remaining -= accepted;
    }

    remaining
}

fn path_needs_rebuild(
    path: &[Vec3],
    current: Vec3,
    target: Vec3,
    geometry: &WorldGeometry,
) -> bool {
    if path.is_empty() {
        return xz_distance(current, target) > 0.05;
    }

    if !line_of_sight_clear(geometry, current, path[0]) {
        return true;
    }

    path.windows(2)
        .any(|w| !line_of_sight_clear(geometry, w[0], w[1]))
}

fn unreachable_moving_state(
    target: Vec3,
    task: Task,
    inventory_access: &mut ParamSet<(
        Query<
            (
                Entity,
                &Inventory,
                &Transform,
                Option<&BuildingEntrance>,
                Option<&PublicInventory>,
                Option<&Housing>,
                Option<&CompletedBuilding>,
            ),
            Without<Colonist>,
        >,
        Query<&mut Inventory>,
    )>,
) -> ColonistState {
    match task {
        Task::Deposit { .. } => ColonistState::Moving {
            target,
            path: Vec::new(),
            task,
        },
        Task::DeliverMaterial { kind, amount, .. } => {
            add_to_nearest_public(inventory_access, target, kind, amount);
            ColonistState::Idle
        }
        Task::DeliverHomeFood { amount, .. } => {
            add_to_nearest_public(inventory_access, target, ResourceKind::Food, amount);
            ColonistState::Idle
        }
        Task::PickupMaterial { .. }
        | Task::Build { .. }
        | Task::Gather { .. }
        | Task::Eat { .. }
        | Task::PickupHomeFood { .. } => ColonistState::Idle,
    }
}

fn move_along_path(
    transform: &mut Transform,
    target: Vec3,
    path: &mut Vec<Vec3>,
    speed: f32,
    dt: f32,
    seed: u64,
) -> bool {
    let waypoint = path.first().copied().unwrap_or(target);
    if move_toward(transform, waypoint, speed, dt, seed) {
        if !path.is_empty() {
            path.remove(0);
        }
        return path.is_empty();
    }

    false
}

fn move_toward(transform: &mut Transform, target: Vec3, speed: f32, dt: f32, seed: u64) -> bool {
    let to_target = target - transform.translation;
    let xz_dist = (to_target.x * to_target.x + to_target.z * to_target.z).sqrt();
    if xz_dist <= 0.05 {
        let ground_y = terrain_height(seed, target.x, target.z) + COLONIST_HALF_HEIGHT;
        transform.translation = Vec3::new(target.x, ground_y, target.z);
        return true;
    }

    let step = speed * dt;
    if step >= xz_dist {
        let ground_y = terrain_height(seed, target.x, target.z) + COLONIST_HALF_HEIGHT;
        transform.translation = Vec3::new(target.x, ground_y, target.z);
        true
    } else {
        let dir_x = to_target.x / xz_dist;
        let dir_z = to_target.z / xz_dist;
        let new_x = transform.translation.x + dir_x * step;
        let new_z = transform.translation.z + dir_z * step;
        let ground_y = terrain_height(seed, new_x, new_z) + COLONIST_HALF_HEIGHT;
        transform.translation = Vec3::new(new_x, ground_y, new_z);
        let yaw = dir_x.atan2(dir_z);
        transform.rotation = Quat::from_rotation_y(yaw);
        false
    }
}

fn decayed_satiety(satiety: f32, dt: f32) -> f32 {
    (satiety - dt / SATIETY_LOSS_SECONDS).max(0.0)
}

fn restored_satiety(satiety: f32) -> f32 {
    (satiety + EAT_RESTORE).min(100.0)
}

fn best_home_candidate(homes: &[(Entity, usize, Vec3)], start: Vec3) -> Option<usize> {
    homes
        .iter()
        .enumerate()
        .filter(|(_, (_, residents, _))| *residents < Housing::CAPACITY)
        .min_by(|(_, (_, count_a, pos_a)), (_, (_, count_b, pos_b))| {
            count_a
                .cmp(count_b)
                .then_with(|| xz_distance(start, *pos_a).total_cmp(&xz_distance(start, *pos_b)))
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::DEFAULT_TERRAIN_SEED;

    const SEED: u64 = DEFAULT_TERRAIN_SEED;

    #[test]
    fn move_along_path_advances_one_waypoint_at_a_time() {
        let mut transform = Transform::from_translation(Vec3::new(
            0.0,
            terrain_height(SEED, 0.0, 0.0) + COLONIST_HALF_HEIGHT,
            0.0,
        ));
        let ty = terrain_height(SEED, 2.0, 0.0) + COLONIST_HALF_HEIGHT;
        let target = Vec3::new(2.0, ty, 0.0);
        let wy = terrain_height(SEED, 1.0, 0.0) + COLONIST_HALF_HEIGHT;
        let mut path = vec![Vec3::new(1.0, wy, 0.0), target];

        assert!(!move_along_path(
            &mut transform,
            target,
            &mut path,
            10.0,
            0.1,
            SEED,
        ));
        assert!((transform.translation.x - 1.0).abs() < 0.01);
        assert_eq!(path, vec![target]);

        assert!(move_along_path(
            &mut transform,
            target,
            &mut path,
            10.0,
            0.1,
            SEED,
        ));
        assert!((transform.translation.x - target.x).abs() < 0.01);
        assert!(path.is_empty());
    }

    #[test]
    fn carried_amount_uses_resource_unit_size() {
        assert_eq!(
            carried_amount(ResourceKind::Wood, COLONIST_CARRY_CAPACITY),
            1
        );
        assert_eq!(
            carried_amount(ResourceKind::Food, COLONIST_CARRY_CAPACITY),
            10
        );
    }

    #[test]
    fn hungry_threshold_starts_below_fifty() {
        let colonist = Colonist {
            name: "Test".to_string(),
            state: ColonistState::Idle,
            speed: 1.0,
            path_rebuild_timer: 0.0,
            home: None,
            satiety: 49.9,
            carry_capacity: COLONIST_CARRY_CAPACITY,
        };

        assert!(colonist.is_hungry());
    }

    #[test]
    fn satiety_decays_with_simulation_time_and_caps_at_zero() {
        assert_eq!(decayed_satiety(100.0, 10.0), 99.0);
        assert_eq!(decayed_satiety(0.5, 10.0), 0.0);
    }

    #[test]
    fn eating_restores_satiety_without_exceeding_one_hundred() {
        assert_eq!(restored_satiety(40.0), 90.0);
        assert_eq!(restored_satiety(80.0), 100.0);
    }

    #[test]
    fn housing_assignment_balances_before_distance() {
        let near_full = Entity::from_raw_u32(1).unwrap();
        let far_empty = Entity::from_raw_u32(2).unwrap();
        let homes = vec![
            (near_full, 4, Vec3::new(0.1, 0.0, 0.0)),
            (far_empty, 0, Vec3::new(100.0, 0.0, 0.0)),
        ];

        let selected = best_home_candidate(&homes, Vec3::ZERO).unwrap();

        assert_eq!(homes[selected].0, far_empty);
    }
}
