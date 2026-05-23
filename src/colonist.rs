use bevy::prelude::*;

use crate::{
    building::{Blueprint, CompletedBuilding},
    resources::ResourceStock,
    simulation::SimulationClock,
    types::{BuildingKind, ResourceKind},
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

pub fn assign_idle_colonists(
    mut stock: ResMut<ResourceStock>,
    mut colonists: Query<&mut Colonist>,
    blueprints: Query<(Entity, &Blueprint, &Transform)>,
    resources: Query<(Entity, &ResourceNode, &Transform)>,
    completed: Query<(&CompletedBuilding, &Transform)>,
) {
    let mut reserved_deliveries = active_material_deliveries(&colonists);
    let has_woodcutter = completed
        .iter()
        .any(|(building, _)| building.kind == BuildingKind::Woodcutter);
    let has_gatherer = completed
        .iter()
        .any(|(building, _)| building.kind == BuildingKind::Gatherer);

    for mut colonist in &mut colonists {
        if !matches!(colonist.state, ColonistState::Idle) {
            continue;
        }

        if let Some((blueprint_entity, transform, remaining_wood)) = blueprints
            .iter()
            .filter_map(|(entity, blueprint, transform)| {
                let reserved = reserved_deliveries
                    .iter()
                    .filter(|(target, _)| *target == entity)
                    .map(|(_, wood)| *wood)
                    .sum::<i32>();
                let remaining = blueprint.needs_wood() - reserved;
                (remaining > 0).then_some((entity, transform, remaining))
            })
            .min_by_key(|(_, _, remaining)| *remaining)
        {
            let wood = MATERIAL_DELIVERY_SIZE.min(remaining_wood);
            if stock.remove(ResourceKind::Wood, wood) {
                colonist.state = ColonistState::Moving {
                    target: transform.translation,
                    task: Task::DeliverMaterial {
                        blueprint: blueprint_entity,
                        wood,
                    },
                };
                reserved_deliveries.push((blueprint_entity, wood));
                continue;
            }
        }

        if let Some((blueprint_entity, _, transform)) =
            blueprints.iter().find(|(_, blueprint, _)| {
                blueprint.has_materials() && blueprint.progress < blueprint.build_seconds
            })
        {
            colonist.state = ColonistState::Moving {
                target: transform.translation,
                task: Task::Build {
                    blueprint: blueprint_entity,
                },
            };
            continue;
        }

        if has_woodcutter {
            if let Some((resource_entity, _, transform)) = resources
                .iter()
                .find(|(_, node, _)| node.kind == ResourceKind::Wood && node.amount > 0)
            {
                colonist.state = ColonistState::Moving {
                    target: transform.translation,
                    task: Task::Gather {
                        resource: resource_entity,
                        kind: ResourceKind::Wood,
                    },
                };
                continue;
            }
        }

        if has_gatherer {
            if let Some((resource_entity, _, transform)) = resources
                .iter()
                .find(|(_, node, _)| node.kind == ResourceKind::Food && node.amount > 0)
            {
                colonist.state = ColonistState::Moving {
                    target: transform.translation,
                    task: Task::Gather {
                        resource: resource_entity,
                        kind: ResourceKind::Food,
                    },
                };
            }
        }
    }
}

fn active_material_deliveries(colonists: &Query<&mut Colonist>) -> Vec<(Entity, i32)> {
    colonists
        .iter()
        .filter_map(|colonist| {
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

pub fn update_colonists(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<SimulationClock>,
    mut stock: ResMut<ResourceStock>,
    mut colonists: Query<(&mut Colonist, &mut Transform)>,
    mut blueprints: Query<&mut Blueprint>,
    mut resources: Query<&mut ResourceNode>,
    completed: Query<(&CompletedBuilding, &Transform), Without<Colonist>>,
) {
    let dt = clock.scaled_delta(&time);
    if dt == 0.0 {
        return;
    }

    for (mut colonist, mut transform) in &mut colonists {
        let current_state = colonist.state.clone();
        match current_state {
            ColonistState::Idle => {}
            ColonistState::Moving { target, task } => {
                if move_toward(&mut transform, target, colonist.speed, dt) {
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
                        commands.entity(resource).despawn();
                    }
                }

                if amount > 0 {
                    colonist.state = ColonistState::Moving {
                        target: deposit_position(&completed),
                        task: Task::Deposit { kind, amount },
                    };
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

fn deposit_position(
    completed: &Query<(&CompletedBuilding, &Transform), Without<Colonist>>,
) -> Vec3 {
    completed
        .iter()
        .find(|(building, _)| building.kind == BuildingKind::Storage)
        .map(|(_, transform)| transform.translation)
        .unwrap_or(STOCKPILE_POSITION)
}
