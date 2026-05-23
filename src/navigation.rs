use std::{cmp::Ordering, collections::{BinaryHeap, HashMap}};

use bevy::prelude::*;

use crate::building::{WorldGeometry, expanded_polygon};
use crate::math::{xz, xz_distance};

const AGENT_RADIUS: f32 = 0.08;
const VISIBILITY_NODE_MARGIN: f32 = 0.16;
const NODE_DEDUP_DISTANCE: f32 = 0.05;

const CACHE_GRID: f32 = 0.5;

#[derive(Resource, Default)]
pub struct PathCache {
    cache: HashMap<(i32, i32, i32, i32), Option<Vec<Vec3>>>,
}

impl PathCache {
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[derive(Clone, Copy, Debug)]
struct PathNode {
    index: usize,
    cost: f32,
    estimated_total: f32,
}

impl PartialEq for PathNode {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
            && self.cost == other.cost
            && self.estimated_total == other.estimated_total
    }
}

impl Eq for PathNode {}

impl Ord for PathNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimated_total
            .partial_cmp(&self.estimated_total)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| self.index.cmp(&other.index))
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn snap_to_cache_grid(value: f32) -> i32 {
    (value / CACHE_GRID).round() as i32
}

pub fn path_to_waypoints(
    geometry: &WorldGeometry,
    cache: &mut PathCache,
    start: Vec3,
    target: Vec3,
) -> Option<Vec<Vec3>> {
    let key = (
        snap_to_cache_grid(start.x),
        snap_to_cache_grid(start.z),
        snap_to_cache_grid(target.x),
        snap_to_cache_grid(target.z),
    );

    if let Some(cached) = cache.cache.get(&key) {
        return cached.clone();
    }

    let result = compute_path(geometry, start, target);
    cache.cache.insert(key, result.clone());
    result
}

fn compute_path(geometry: &WorldGeometry, start: Vec3, target: Vec3) -> Option<Vec<Vec3>> {
    let target = Vec3::new(target.x, start.y, target.z);
    if xz_distance(start, target) < 0.001 {
        return Some(Vec::new());
    }
    if !geometry.is_walkable_point(start) || !geometry.is_walkable_point(target) {
        return None;
    }
    if geometry.segment_clear(start, target, AGENT_RADIUS) {
        return Some(vec![target]);
    }

    let start_2d = xz(start);
    let target_2d = xz(target);
    let mut nodes = vec![start_2d, target_2d];
    for obstacle in geometry
        .obstacles()
        .iter()
        .filter(|obstacle| !obstacle.passable)
    {
        for node in expanded_polygon(&obstacle.polygon, AGENT_RADIUS + VISIBILITY_NODE_MARGIN) {
            if geometry.is_walkable_point(Vec3::new(node.x, start.y, node.y)) {
                push_unique_node(&mut nodes, node);
            }
        }
    }

    let path_indices = visibility_graph_a_star(geometry, &nodes)?;
    let mut waypoints: Vec<Vec3> = path_indices
        .into_iter()
        .skip(1)
        .map(|index| {
            if index == 1 {
                target
            } else {
                Vec3::new(nodes[index].x, start.y, nodes[index].y)
            }
        })
        .collect();

    if waypoints.last().copied() != Some(target) {
        waypoints.push(target);
    }
    Some(waypoints)
}

pub fn line_of_sight_clear(geometry: &WorldGeometry, from: Vec3, to: Vec3) -> bool {
    geometry.segment_clear(from, to, AGENT_RADIUS)
}

fn visibility_graph_a_star(geometry: &WorldGeometry, nodes: &[Vec2]) -> Option<Vec<usize>> {
    let goal = 1usize;
    let mut frontier = BinaryHeap::new();
    let mut came_from = vec![None; nodes.len()];
    let mut costs = vec![f32::MAX; nodes.len()];
    costs[0] = 0.0;
    frontier.push(PathNode {
        index: 0,
        cost: 0.0,
        estimated_total: nodes[0].distance(nodes[goal]),
    });

    while let Some(current) = frontier.pop() {
        if current.index == goal {
            return Some(reconstruct_path(&came_from, goal));
        }
        if current.cost > costs[current.index] {
            continue;
        }

        for next in 0..nodes.len() {
            if next == current.index {
                continue;
            }
            if !segment_clear_between_nodes(geometry, nodes[current.index], nodes[next]) {
                continue;
            }

            let next_cost = current.cost + nodes[current.index].distance(nodes[next]);
            if next_cost < costs[next] {
                costs[next] = next_cost;
                came_from[next] = Some(current.index);
                frontier.push(PathNode {
                    index: next,
                    cost: next_cost,
                    estimated_total: next_cost + nodes[next].distance(nodes[goal]),
                });
            }
        }
    }

    None
}

fn segment_clear_between_nodes(geometry: &WorldGeometry, from: Vec2, to: Vec2) -> bool {
    geometry.segment_clear_2d(from, to, AGENT_RADIUS)
}

fn reconstruct_path(came_from: &[Option<usize>], goal: usize) -> Vec<usize> {
    let mut current = goal;
    let mut path = vec![goal];
    while let Some(previous) = came_from[current] {
        path.push(previous);
        current = previous;
    }
    path.reverse();
    path
}

fn push_unique_node(nodes: &mut Vec<Vec2>, node: Vec2) {
    if nodes
        .iter()
        .any(|existing| existing.distance(node) <= NODE_DEDUP_DISTANCE)
    {
        return;
    }
    nodes.push(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{WorldGeometry, rectangle_polygon};

    fn test_entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn empty_ground_returns_direct_continuous_target() {
        let geometry = WorldGeometry::default();
        let target = Vec3::new(4.7, 0.0, 2.2);

        let mut cache = PathCache::default();
        let path = path_to_waypoints(&geometry, &mut cache, Vec3::new(0.2, 0.0, 0.3), target).unwrap();

        assert_eq!(path, vec![target]);
    }

    #[test]
    fn line_of_sight_blocked_by_continuous_obstacle() {
        let mut geometry = WorldGeometry::default();
        geometry.occupy_polygon(
            rectangle_polygon(Vec3::new(1.0, 0.0, 0.0), Vec2::splat(0.8), 0.0),
            test_entity(1),
            false,
        );

        assert!(!line_of_sight_clear(
            &geometry,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ));
    }

    #[test]
    fn path_routes_around_continuous_obstacle() {
        let mut geometry = WorldGeometry::default();
        geometry.occupy_polygon(
            rectangle_polygon(Vec3::new(1.5, 0.0, 0.0), Vec2::new(0.9, 1.2), 0.2),
            test_entity(1),
            false,
        );

        let mut cache = PathCache::default();
        let path = path_to_waypoints(
            &geometry,
            &mut cache,
            Vec3::new(0.0, 0.0, 0.13),
            Vec3::new(3.2, 0.0, 0.73),
        )
        .unwrap();

        assert!(path.len() >= 2);
        assert_eq!(path.last().copied(), Some(Vec3::new(3.2, 0.0, 0.73)));
        assert!(
            path.iter().any(|point| {
                (point.x.fract().abs() > 0.001) && (point.z.fract().abs() > 0.001)
            })
        );
    }

    #[test]
    fn passable_obstacles_do_not_block_navigation() {
        let mut geometry = WorldGeometry::default();
        geometry.occupy_polygon(
            rectangle_polygon(Vec3::new(1.0, 0.0, 0.0), Vec2::splat(0.8), 0.0),
            test_entity(1),
            true,
        );

        let target = Vec3::new(2.0, 0.0, 0.0);
        let mut cache = PathCache::default();
        let path = path_to_waypoints(&geometry, &mut cache, Vec3::ZERO, target).unwrap();

        assert_eq!(path, vec![target]);
    }
}
