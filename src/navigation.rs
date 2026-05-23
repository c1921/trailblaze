use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

use bevy::prelude::*;

use crate::{
    building::MapGrid,
    types::{cell_to_world, within_map, world_to_cell, ROAD_COST},
};

const SQRT_2: f32 = 1.41421356;
const MIN_TERRAIN_COST: f32 = ROAD_COST;

#[derive(Clone, Copy, Debug)]
struct PathNode {
    cell: IVec2,
    cost: f32,
    estimated_total: f32,
}

impl PartialEq for PathNode {
    fn eq(&self, other: &Self) -> bool {
        self.cell == other.cell
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
            .then_with(|| other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal))
            .then_with(|| self.cell.x.cmp(&other.cell.x))
            .then_with(|| self.cell.y.cmp(&other.cell.y))
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn find_path(grid: &MapGrid, start: IVec2, goal: IVec2) -> Option<Vec<IVec2>> {
    if !within_map(start) || !within_map(goal) {
        return None;
    }
    if start == goal {
        return Some(Vec::new());
    }
    if !grid.is_walkable(goal) {
        return None;
    }

    let mut frontier = BinaryHeap::new();
    let mut came_from = HashMap::new();
    let mut costs = HashMap::new();

    frontier.push(PathNode {
        cell: start,
        cost: 0.0,
        estimated_total: octile_distance(start, goal),
    });
    costs.insert(start, 0.0f32);

    while let Some(current) = frontier.pop() {
        if current.cell == goal {
            return Some(reconstruct_path(start, goal, &came_from));
        }
        if current.cost > *costs.get(&current.cell).unwrap_or(&f32::MAX) {
            continue;
        }

        for (next, step_cost) in all_neighbors(grid, current.cell) {
            let next_cost = current.cost + step_cost;
            if next_cost < *costs.get(&next).unwrap_or(&f32::MAX) {
                costs.insert(next, next_cost);
                came_from.insert(next, current.cell);
                frontier.push(PathNode {
                    cell: next,
                    cost: next_cost,
                    estimated_total: next_cost + octile_distance(next, goal),
                });
            }
        }
    }

    None
}

pub fn path_to_waypoints(grid: &MapGrid, start: Vec3, target: Vec3) -> Option<Vec<Vec3>> {
    let target = Vec3::new(target.x, start.y, target.z);
    let start_cell = world_to_cell(start);
    let target_cell = world_to_cell(target);
    let path = find_path(grid, start_cell, target_cell)?;
    let smoothed = smooth_path(grid, path);
    let mut waypoints: Vec<Vec3> = smoothed
        .iter()
        .map(|cell| {
            let position = cell_to_world(*cell);
            Vec3::new(position.x, start.y, position.z)
        })
        .collect();

    if let Some(last) = waypoints.last_mut() {
        *last = target;
    } else {
        waypoints.push(target);
    }

    Some(waypoints)
}

pub fn line_of_sight_clear(grid: &MapGrid, from: Vec3, to: Vec3) -> bool {
    let diff = to - from;
    let distance = diff.length();
    if distance < 0.001 {
        return true;
    }
    let steps = (distance / 0.3).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let point = from + diff * t;
        if !grid.is_walkable(world_to_cell(point)) {
            return false;
        }
    }
    true
}

pub fn adjacent_cells(cell: IVec2) -> [IVec2; 8] {
    [
        cell + IVec2::X,
        cell - IVec2::X,
        cell + IVec2::Y,
        cell - IVec2::Y,
        cell + IVec2::new(1, 1),
        cell + IVec2::new(-1, 1),
        cell + IVec2::new(1, -1),
        cell + IVec2::new(-1, -1),
    ]
}

fn all_neighbors(grid: &MapGrid, cell: IVec2) -> Vec<(IVec2, f32)> {
    let mut neighbors = Vec::with_capacity(8);

    for (dir, mult) in [
        (IVec2::X, 1.0),
        (IVec2::NEG_X, 1.0),
        (IVec2::Y, 1.0),
        (IVec2::NEG_Y, 1.0),
        (IVec2::new(1, 1), SQRT_2),
        (IVec2::new(-1, 1), SQRT_2),
        (IVec2::new(1, -1), SQRT_2),
        (IVec2::new(-1, -1), SQRT_2),
    ] {
        let next = cell + dir;
        if let Some(terrain_cost) = grid.movement_cost(next) {
            neighbors.push((next, mult * terrain_cost));
        }
    }

    neighbors
}

fn smooth_path(grid: &MapGrid, cell_path: Vec<IVec2>) -> Vec<IVec2> {
    if cell_path.len() <= 2 {
        return cell_path;
    }
    let mut smoothed = vec![cell_path[0]];
    let mut i = 0;
    while i < cell_path.len() - 1 {
        let from = cell_to_world(cell_path[i]);
        let mut found = false;
        for j in (i + 2..cell_path.len()).rev() {
            let to = cell_to_world(cell_path[j]);
            if line_of_sight_clear(grid, from, to) {
                smoothed.push(cell_path[j]);
                i = j;
                found = true;
                break;
            }
        }
        if !found {
            i += 1;
            smoothed.push(cell_path[i]);
        }
    }
    smoothed
}

fn reconstruct_path(start: IVec2, goal: IVec2, came_from: &HashMap<IVec2, IVec2>) -> Vec<IVec2> {
    let mut current = goal;
    let mut path = Vec::new();

    while current != start {
        path.push(current);
        let Some(previous) = came_from.get(&current) else {
            return Vec::new();
        };
        current = *previous;
    }

    path.reverse();
    path
}

fn octile_distance(a: IVec2, b: IVec2) -> f32 {
    let dx = (a.x - b.x).abs() as f32;
    let dy = (a.y - b.y).abs() as f32;
    let (min_d, max_d) = if dx < dy { (dx, dy) } else { (dy, dx) };
    SQRT_2 * min_d * MIN_TERRAIN_COST + (max_d - min_d) * MIN_TERRAIN_COST
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    #[test]
    fn path_routes_around_blocked_cells() {
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(1, 0)], test_entity(1), false);

        let path = find_path(&grid, IVec2::ZERO, IVec2::new(2, 0)).unwrap();

        assert!(!path.contains(&IVec2::new(1, 0)));
        assert_eq!(path.last(), Some(&IVec2::new(2, 0)));
        assert!(path.len() >= 2);
    }

    #[test]
    fn path_can_use_road_cells() {
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(1, 0)], test_entity(1), true);

        let path = find_path(&grid, IVec2::ZERO, IVec2::new(2, 0)).unwrap();

        assert_eq!(path, vec![IVec2::new(1, 0), IVec2::new(2, 0)]);
    }

    #[test]
    fn path_rejects_blocked_goal() {
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(1, 0)], test_entity(1), false);

        assert!(find_path(&grid, IVec2::ZERO, IVec2::new(1, 0)).is_none());
    }

    #[test]
    fn path_prefers_road_over_ground() {
        let mut grid = MapGrid::default();
        grid.occupy(
            &[
                IVec2::new(1, 1),
                IVec2::new(2, 1),
                IVec2::new(3, 1),
            ],
            test_entity(2),
            true,
        );

        let path = find_path(&grid, IVec2::ZERO, IVec2::new(3, 0)).unwrap();

        assert_eq!(path.last(), Some(&IVec2::new(3, 0)));
        // Should prefer road detour (cost ~2.707) over direct ground path (cost 3.0)
        assert!(path.contains(&IVec2::new(1, 1)) || path.contains(&IVec2::new(2, 1)));
    }

    #[test]
    fn smooth_path_removes_unnecessary_waypoints() {
        let grid = MapGrid::default();
        // Straight line path
        let cells = vec![
            IVec2::new(1, 0),
            IVec2::new(2, 0),
            IVec2::new(3, 0),
            IVec2::new(4, 0),
        ];
        let smoothed = smooth_path(&grid, cells);
        assert_eq!(smoothed, vec![IVec2::new(1, 0), IVec2::new(4, 0)]);
    }

    #[test]
    fn smooth_path_preserves_corner_around_obstacle() {
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(2, 0)], test_entity(1), false);

        // Path goes around obstacle
        let cells = vec![
            IVec2::new(1, 1),
            IVec2::new(2, 1),
            IVec2::new(3, 1),
            IVec2::new(3, 0),
        ];
        let smoothed = smooth_path(&grid, cells);
        // Should keep the corner waypoint because LOS from (1,1) to (3,0) is blocked
        assert_eq!(smoothed.len(), 3);
        assert!(smoothed.contains(&IVec2::new(3, 0)));
    }

    #[test]
    fn line_of_sight_blocked_by_obstacle() {
        let mut grid = MapGrid::default();
        grid.occupy(&[IVec2::new(1, 0)], test_entity(1), false);

        assert!(!line_of_sight_clear(
            &grid,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ));
    }

    #[test]
    fn line_of_sight_clear_on_empty_ground() {
        let grid = MapGrid::default();
        assert!(line_of_sight_clear(
            &grid,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
        ));
    }
}
