use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

use bevy::prelude::*;

use crate::building::{WorldGeometry, expanded_polygon};
use crate::math::{xz, xz_distance};
use crate::terrain::terrain_height;

const AGENT_RADIUS: f32 = 0.08;
const VISIBILITY_NODE_MARGIN: f32 = 0.16;
const NODE_DEDUP_DISTANCE: f32 = 0.05;
const SLOPE_COST_FACTOR: f32 = 1.5;

const CACHE_GRID: f32 = 0.5;

#[derive(Resource, Default)]
pub struct PathCache {
    cache: HashMap<(i32, i32, i32, i32), Option<Vec<Vec3>>>,
    geometry_revision: Option<u64>,
}

impl PathCache {
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    fn sync_geometry(&mut self, revision: u64) {
        if self.geometry_revision == Some(revision) {
            return;
        }

        self.clear();
        self.geometry_revision = Some(revision);
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
    seed: u64,
) -> Option<Vec<Vec3>> {
    cache.sync_geometry(geometry.revision());

    let key = (
        snap_to_cache_grid(start.x),
        snap_to_cache_grid(start.z),
        snap_to_cache_grid(target.x),
        snap_to_cache_grid(target.z),
    );

    if let Some(cached) = cache.cache.get(&key) {
        return cached.clone();
    }

    let result = compute_path(geometry, start, target, seed);
    cache.cache.insert(key, result.clone());
    result
}

fn compute_path(
    geometry: &WorldGeometry,
    start: Vec3,
    target: Vec3,
    seed: u64,
) -> Option<Vec<Vec3>> {
    let target = Vec3::new(target.x, start.y, target.z);
    if xz_distance(start, target) < 0.001 {
        return Some(Vec::new());
    }
    if !geometry.is_walkable_point(start) || !geometry.is_walkable_point(target) {
        return None;
    }
    if geometry.segment_clear(start, target, AGENT_RADIUS) {
        let y = terrain_height(seed, target.x, target.z);
        return Some(vec![Vec3::new(target.x, y, target.z)]);
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
            let y = terrain_height(seed, node.x, node.y);
            if geometry.is_walkable_point(Vec3::new(node.x, y, node.y)) {
                push_unique_node(&mut nodes, node);
            }
        }
    }

    let path_indices = visibility_graph_a_star(geometry, &nodes, seed)?;
    let mut waypoints: Vec<Vec3> = path_indices
        .into_iter()
        .skip(1)
        .map(|index| {
            if index == 1 {
                let y = terrain_height(seed, target.x, target.z);
                Vec3::new(target.x, y, target.z)
            } else {
                let y = terrain_height(seed, nodes[index].x, nodes[index].y);
                Vec3::new(nodes[index].x, y, nodes[index].y)
            }
        })
        .collect();

    let final_target = Vec3::new(target.x, terrain_height(seed, target.x, target.z), target.z);
    if waypoints.last().copied() != Some(final_target) {
        waypoints.push(final_target);
    }
    Some(waypoints)
}

pub fn line_of_sight_clear(geometry: &WorldGeometry, from: Vec3, to: Vec3) -> bool {
    geometry.segment_clear(from, to, AGENT_RADIUS)
}

fn visibility_graph_a_star(
    geometry: &WorldGeometry,
    nodes: &[Vec2],
    seed: u64,
) -> Option<Vec<usize>> {
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

            let xz_dist = nodes[current.index].distance(nodes[next]);
            let h_current = terrain_height(seed, nodes[current.index].x, nodes[current.index].y);
            let h_next = terrain_height(seed, nodes[next].x, nodes[next].y);
            let slope_cost = (h_next - h_current).abs() * SLOPE_COST_FACTOR;
            let next_cost = current.cost + xz_dist + slope_cost;

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
    use crate::terrain::DEFAULT_TERRAIN_SEED;

    const SEED: u64 = DEFAULT_TERRAIN_SEED;

    fn test_entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn empty_ground_returns_direct_continuous_target() {
        let geometry = WorldGeometry::default();
        let start = Vec3::new(0.2, terrain_height(SEED, 0.2, 0.3), 0.3);
        let target_y = terrain_height(SEED, 4.7, 2.2);
        let target = Vec3::new(4.7, target_y, 2.2);

        let mut cache = PathCache::default();
        let path = path_to_waypoints(&geometry, &mut cache, start, target, SEED).unwrap();

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

        let start = Vec3::new(0.0, terrain_height(SEED, 0.0, 0.13), 0.13);
        let target = Vec3::new(3.2, terrain_height(SEED, 3.2, 0.73), 0.73);

        let mut cache = PathCache::default();
        let path = path_to_waypoints(&geometry, &mut cache, start, target, SEED).unwrap();

        assert!(path.len() >= 2);
        assert_eq!(path.last().copied(), Some(target));
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

        let start = Vec3::new(0.0, terrain_height(SEED, 0.0, 0.0), 0.0);
        let target = Vec3::new(2.0, terrain_height(SEED, 2.0, 0.0), 0.0);
        let mut cache = PathCache::default();
        let path = path_to_waypoints(&geometry, &mut cache, start, target, SEED).unwrap();

        assert_eq!(path, vec![target]);
    }

    #[test]
    fn cache_invalidates_when_geometry_revision_changes() {
        let mut geometry = WorldGeometry::default();
        let start = Vec3::new(0.0, terrain_height(SEED, 0.0, 0.13), 0.13);
        let target = Vec3::new(3.2, terrain_height(SEED, 3.2, 0.73), 0.73);
        let mut cache = PathCache::default();

        let direct = path_to_waypoints(&geometry, &mut cache, start, target, SEED).unwrap();
        assert_eq!(direct, vec![target]);

        geometry.occupy_polygon(
            rectangle_polygon(Vec3::new(1.5, 0.0, 0.0), Vec2::new(0.9, 1.2), 0.2),
            test_entity(1),
            false,
        );

        let routed = path_to_waypoints(&geometry, &mut cache, start, target, SEED).unwrap();
        assert!(routed.len() >= 2);
        assert_ne!(routed, direct);
    }
}
