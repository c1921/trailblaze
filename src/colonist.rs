use bevy::prelude::*;

use crate::{
    building::{Blueprint, BuildingEntrance, CompletedBuilding, MapGrid},
    navigation::{adjacent_cells, line_of_sight_clear, path_to_waypoints},
    resources::ResourceStock,
    simulation::SimulationClock,
    types::{BuildingKind, ResourceKind, cell_to_world, world_to_cell},
    world::ResourceNode,
};

const MATERIAL_DELIVERY_SIZE: i32 = 4;
const GATHER_AMOUNT: i32 = 4;
const GATHER_SECONDS: f32 = 1.4;
const BUILD_RATE: f32 = 1.0;
const STOCKPILE_POSITION: Vec3 = Vec3::new(0.0, 0.0, 0.0);

#[derive(Component, Debug)]
pub struct Colonist {
    pub name: String,
    pub state: ColonistState,
    pub speed: f32,
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
}

#[derive(Debug, Clone)]
pub enum Task {
    DeliverMaterial {
        blueprint: Entity,
        wood: i32,
    },
    Build {
        blueprint: Entity,
    },
    Gather {
        resource: Entity,
        kind: ResourceKind,
    },
    Deposit {
        kind: ResourceKind,
        amount: i32,
    },
}

impl Colonist {
    pub fn status_label(&self) -> String {
        self.state.label()
    }
}

impl ColonistState {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Idle".to_string(),
            Self::Moving { task, .. } => format!("Moving: {}", task.label()),
            Self::Gathering { kind, .. } => format!("Gathering {}", kind.label()),
            Self::Building { .. } => "Building a blueprint".to_string(),
        }
    }
}

impl Task {
    pub fn label(&self) -> String {
        match self {
            Self::DeliverMaterial { wood, .. } => format!("Delivering {} wood", wood),
            Self::Build { .. } => "Going to build".to_string(),
            Self::Gather { kind, .. } => format!("Going to gather {}", kind.label()),
            Self::Deposit { kind, amount } => format!("Depositing {} {}", amount, kind.label()),
        }
    }
}

pub fn assign_idle_colonists(
    mut stock: ResMut<ResourceStock>,
    grid: Res<MapGrid>,
    mut colonists: Query<(&mut Colonist, &Transform)>,
    blueprints: Query<(Entity, &Blueprint, &Transform, Option<&BuildingEntrance>)>,
    resources: Query<(Entity, &ResourceNode, &Transform)>,
    completed: Query<(&CompletedBuilding, &Transform, Option<&BuildingEntrance>)>,
) {
    let mut reserved_deliveries = active_material_deliveries(&colonists);
    let has_woodcutter = completed
        .iter()
        .any(|(building, _, _)| building.kind == BuildingKind::Woodcutter);
    let has_gatherer = completed
        .iter()
        .any(|(building, _, _)| building.kind == BuildingKind::Gatherer);

    for (mut colonist, transform) in &mut colonists {
        if !matches!(colonist.state, ColonistState::Idle) {
            continue;
        }

        let start = transform.translation;

        let mut delivery_candidate: Option<(i32, usize, Entity, i32, ColonistState)> = None;
        for (entity, blueprint, blueprint_transform, entrance) in &blueprints {
            let reserved = reserved_deliveries
                .iter()
                .filter(|(target, _)| *target == entity)
                .map(|(_, wood)| *wood)
                .sum::<i32>();
            let remaining = blueprint.needs_wood() - reserved;
            if remaining <= 0 {
                continue;
            }

            let wood = MATERIAL_DELIVERY_SIZE.min(remaining);
            let target = building_interaction_position(blueprint_transform, entrance);
            let task = Task::DeliverMaterial {
                blueprint: entity,
                wood,
            };
            let Some((state, path_len)) = moving_state_to_target(&grid, start, target, task) else {
                continue;
            };

            if delivery_candidate
                .as_ref()
                .map(|(best_remaining, best_len, _, _, _)| {
                    remaining < *best_remaining
                        || (remaining == *best_remaining && path_len < *best_len)
                })
                .unwrap_or(true)
            {
                delivery_candidate = Some((remaining, path_len, entity, wood, state));
            }
        }

        if let Some((_, _, blueprint_entity, wood, state)) = delivery_candidate {
            if stock.remove(ResourceKind::Wood, wood) {
                colonist.state = state;
                reserved_deliveries.push((blueprint_entity, wood));
                continue;
            }
        }

        let mut build_candidate: Option<(usize, ColonistState)> = None;
        for (entity, blueprint, blueprint_transform, entrance) in &blueprints {
            if !blueprint.has_materials() || blueprint.progress >= blueprint.build_seconds {
                continue;
            }

            let target = building_interaction_position(blueprint_transform, entrance);
            let task = Task::Build { blueprint: entity };
            let Some((state, path_len)) = moving_state_to_target(&grid, start, target, task) else {
                continue;
            };

            if build_candidate
                .as_ref()
                .map(|(best_len, _)| path_len < *best_len)
                .unwrap_or(true)
            {
                build_candidate = Some((path_len, state));
            }
        }

        if let Some((_, state)) = build_candidate {
            colonist.state = state;
            continue;
        }

        if has_woodcutter {
            if let Some((_, state)) = gather_candidate(&grid, start, ResourceKind::Wood, &resources)
            {
                colonist.state = state;
                continue;
            }
        }

        if has_gatherer {
            if let Some((_, state)) = gather_candidate(&grid, start, ResourceKind::Food, &resources)
            {
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
        .filter_map(|(colonist, _)| {
            if let ColonistState::Moving {
                task: Task::DeliverMaterial { blueprint, wood },
                ..
            } = &colonist.state
            {
                Some((*blueprint, *wood))
            } else {
                None
            }
        })
        .collect()
}

fn gather_candidate(
    grid: &MapGrid,
    start: Vec3,
    kind: ResourceKind,
    resources: &Query<(Entity, &ResourceNode, &Transform)>,
) -> Option<(usize, ColonistState)> {
    resources
        .iter()
        .filter(|(_, node, _)| node.kind == kind && node.amount > 0)
        .filter_map(|(resource, _, transform)| {
            movement_to_resource(
                grid,
                start,
                transform.translation,
                Task::Gather { resource, kind },
            )
        })
        .min_by_key(|(path_len, _)| *path_len)
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
    grid: &MapGrid,
    start: Vec3,
    target: Vec3,
    task: Task,
) -> Option<(ColonistState, usize)> {
    let path = path_to_waypoints(grid, start, target)?;
    let path_len = path.len();

    Some((ColonistState::Moving { target, path, task }, path_len))
}

fn movement_to_resource(
    grid: &MapGrid,
    start: Vec3,
    resource_position: Vec3,
    task: Task,
) -> Option<(usize, ColonistState)> {
    let resource_cell = world_to_cell(resource_position);

    adjacent_cells(resource_cell)
        .into_iter()
        .filter(|cell| grid.is_walkable(*cell))
        .filter_map(|cell| {
            let target = cell_to_world(cell);
            let (state, path_len) = moving_state_to_target(grid, start, target, task.clone())?;
            Some((path_len, state))
        })
        .min_by_key(|(path_len, _)| *path_len)
}

fn deposit_moving_state(
    grid: &MapGrid,
    start: Vec3,
    completed: &Query<
        (&CompletedBuilding, &Transform, Option<&BuildingEntrance>),
        Without<Colonist>,
    >,
    task: Task,
) -> ColonistState {
    let mut storage_target: Option<Vec3> = None;
    let mut best_storage: Option<(usize, ColonistState)> = None;
    for (building, transform, entrance) in completed.iter() {
        if building.kind != BuildingKind::Storage {
            continue;
        }

        let target = building_interaction_position(transform, entrance);
        storage_target.get_or_insert(target);
        if let Some((state, path_len)) = moving_state_to_target(grid, start, target, task.clone()) {
            if best_storage
                .as_ref()
                .map(|(best_len, _)| path_len < *best_len)
                .unwrap_or(true)
            {
                best_storage = Some((path_len, state));
            }
        }
    }

    if let Some((_, state)) = best_storage {
        return state;
    }
    if storage_target.is_none() {
        if let Some((state, _)) =
            moving_state_to_target(grid, start, STOCKPILE_POSITION, task.clone())
        {
            return state;
        }
    }

    ColonistState::Moving {
        target: storage_target.unwrap_or(STOCKPILE_POSITION),
        path: Vec::new(),
        task,
    }
}

pub fn update_colonists(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<SimulationClock>,
    mut grid: ResMut<MapGrid>,
    mut stock: ResMut<ResourceStock>,
    mut colonists: Query<(&mut Colonist, &mut Transform)>,
    mut blueprints: Query<&mut Blueprint>,
    mut resources: Query<&mut ResourceNode>,
    completed: Query<
        (&CompletedBuilding, &Transform, Option<&BuildingEntrance>),
        Without<Colonist>,
    >,
) {
    let dt = clock.scaled_delta(&time);
    if dt == 0.0 {
        return;
    }

    for (mut colonist, mut transform) in &mut colonists {
        let current_state = colonist.state.clone();
        match current_state {
            ColonistState::Idle => {}
            ColonistState::Moving {
                target,
                mut path,
                task,
            } => {
                if path_needs_rebuild(&path, transform.translation, target, &grid) {
                    if let Some((state, _)) =
                        moving_state_to_target(&grid, transform.translation, target, task.clone())
                    {
                        if let ColonistState::Moving {
                            path: rebuilt_path, ..
                        } = state
                        {
                            path = rebuilt_path;
                        }
                    } else {
                        colonist.state = unreachable_moving_state(target, task, &mut stock);
                        continue;
                    }
                }

                if move_along_path(&mut transform, target, &mut path, colonist.speed, dt) {
                    match task {
                        Task::DeliverMaterial { blueprint, wood } => {
                            if let Ok(mut blueprint) = blueprints.get_mut(blueprint) {
                                blueprint.delivered_wood =
                                    (blueprint.delivered_wood + wood).min(blueprint.required_wood);
                            } else {
                                stock.add(ResourceKind::Wood, wood);
                            }
                            colonist.state = ColonistState::Idle;
                        }
                        Task::Build { blueprint } => {
                            colonist.state = ColonistState::Building { blueprint };
                        }
                        Task::Gather { resource, kind } => {
                            colonist.state = ColonistState::Gathering {
                                resource,
                                kind,
                                timer: 0.0,
                            };
                        }
                        Task::Deposit { kind, amount } => {
                            stock.add(kind, amount);
                            colonist.state = ColonistState::Idle;
                        }
                    }
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

                let mut amount = 0;
                if let Ok(mut node) = resources.get_mut(resource) {
                    amount = GATHER_AMOUNT.min(node.amount);
                    node.amount -= amount;
                    if node.amount <= 0 {
                        grid.release_entity(resource);
                        commands.entity(resource).despawn();
                    }
                }

                if amount > 0 {
                    colonist.state = deposit_moving_state(
                        &grid,
                        transform.translation,
                        &completed,
                        Task::Deposit { kind, amount },
                    );
                } else {
                    colonist.state = ColonistState::Idle;
                }
            }
            ColonistState::Building {
                blueprint: blueprint_entity,
            } => {
                if let Ok(mut blueprint) = blueprints.get_mut(blueprint_entity) {
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

fn path_needs_rebuild(path: &[Vec3], current: Vec3, target: Vec3, grid: &MapGrid) -> bool {
    if path.is_empty() {
        return xz_distance(current, target) > 0.05;
    }

    if !line_of_sight_clear(grid, current, path[0]) {
        return true;
    }

    path.windows(2).any(|w| !line_of_sight_clear(grid, w[0], w[1]))
}

fn unreachable_moving_state(target: Vec3, task: Task, stock: &mut ResourceStock) -> ColonistState {
    match task {
        Task::Deposit { .. } => ColonistState::Moving {
            target,
            path: Vec::new(),
            task,
        },
        Task::DeliverMaterial { wood, .. } => {
            stock.add(ResourceKind::Wood, wood);
            ColonistState::Idle
        }
        Task::Build { .. } | Task::Gather { .. } => ColonistState::Idle,
    }
}

fn move_along_path(
    transform: &mut Transform,
    target: Vec3,
    path: &mut Vec<Vec3>,
    speed: f32,
    dt: f32,
) -> bool {
    let waypoint = path.first().copied().unwrap_or(target);
    if move_toward(transform, waypoint, speed, dt) {
        if !path.is_empty() {
            path.remove(0);
        }
        return path.is_empty();
    }

    false
}

fn move_toward(transform: &mut Transform, target: Vec3, speed: f32, dt: f32) -> bool {
    let target = Vec3::new(target.x, transform.translation.y, target.z);
    let to_target = target - transform.translation;
    let distance = to_target.length();
    if distance <= 0.05 {
        transform.translation = target;
        return true;
    }

    let step = speed * dt;
    if step >= distance {
        transform.translation = target;
        true
    } else {
        let direction = to_target / distance;
        transform.translation += direction * step;
        let yaw = direction.x.atan2(direction.z);
        transform.rotation = Quat::from_rotation_y(yaw);
        false
    }
}

fn xz_distance(left: Vec3, right: Vec3) -> f32 {
    Vec2::new(left.x - right.x, left.z - right.z).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_along_path_advances_one_waypoint_at_a_time() {
        let mut transform = Transform::from_translation(Vec3::ZERO);
        let target = Vec3::new(2.0, 0.0, 0.0);
        let mut path = vec![Vec3::new(1.0, 0.0, 0.0), target];

        assert!(!move_along_path(
            &mut transform,
            target,
            &mut path,
            10.0,
            0.1
        ));
        assert_eq!(transform.translation, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(path, vec![target]);

        assert!(move_along_path(
            &mut transform,
            target,
            &mut path,
            10.0,
            0.1
        ));
        assert_eq!(transform.translation, target);
        assert!(path.is_empty());
    }

    #[test]
    fn unreachable_deposit_keeps_moving_state() {
        let mut stock = ResourceStock::default();
        let target = Vec3::new(4.0, 0.0, 0.0);
        let state = unreachable_moving_state(
            target,
            Task::Deposit {
                kind: ResourceKind::Food,
                amount: 3,
            },
            &mut stock,
        );

        assert!(matches!(
            state,
            ColonistState::Moving {
                target: saved_target,
                ref path,
                task: Task::Deposit {
                    kind: ResourceKind::Food,
                    amount: 3
                },
            } if saved_target == target && path.is_empty()
        ));
    }

    #[test]
    fn unreachable_delivery_refunds_materials() {
        let mut stock = ResourceStock { wood: 0, food: 0 };
        let state = unreachable_moving_state(
            Vec3::ZERO,
            Task::DeliverMaterial {
                blueprint: Entity::from_raw_u32(1).unwrap(),
                wood: 4,
            },
            &mut stock,
        );

        assert!(matches!(state, ColonistState::Idle));
        assert_eq!(stock.wood, 4);
    }
}
