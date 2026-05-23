use bevy::prelude::*;

use crate::terrain::terrain_height;
use crate::types::MAP_BUILD_HALF_EXTENT;

pub fn xz(position: Vec3) -> Vec2 {
    Vec2::new(position.x, position.z)
}

pub fn xz_distance(left: Vec3, right: Vec3) -> f32 {
    xz(left).distance(xz(right))
}

pub fn xz_length(position: Vec3) -> f32 {
    xz(position).length()
}

pub fn within_world_bounds(point: Vec2) -> bool {
    point.x >= -MAP_BUILD_HALF_EXTENT
        && point.x <= MAP_BUILD_HALF_EXTENT
        && point.y >= -MAP_BUILD_HALF_EXTENT
        && point.y <= MAP_BUILD_HALF_EXTENT
}

const RAY_STEP: f32 = 0.5;
const BINARY_REFINE_ITERATIONS: usize = 6;

pub fn terrain_pick_max_distance() -> f32 {
    MAP_BUILD_HALF_EXTENT * 4.0
}

pub fn ray_terrain_intersection(ray: Ray3d, seed: u64, max_distance: f32) -> Option<Vec3> {
    let mut t = 0.0;
    let max_steps = (max_distance.max(0.0) / RAY_STEP).ceil() as usize;

    for _ in 0..=max_steps {
        if t > max_distance {
            break;
        }
        let point = ray.origin + ray.direction * t;
        let terrain_y = terrain_height(seed, point.x, point.z);
        if point.y <= terrain_y {
            if t <= RAY_STEP {
                return Some(point_at_terrain_height(ray, seed, t));
            }
            return Some(binary_refine_terrain(ray, seed, t - RAY_STEP, t));
        }
        t += RAY_STEP;
    }

    None
}

fn binary_refine_terrain(ray: Ray3d, seed: u64, t_low: f32, t_high: f32) -> Vec3 {
    let mut lo = t_low;
    let mut hi = t_high;

    for _ in 0..BINARY_REFINE_ITERATIONS {
        let mid = (lo + hi) * 0.5;
        let point = ray.origin + ray.direction * mid;
        let terrain_y = terrain_height(seed, point.x, point.z);
        if point.y <= terrain_y {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let t = (lo + hi) * 0.5;
    let result = ray.origin + ray.direction * t;
    Vec3::new(result.x, terrain_height(seed, result.x, result.z), result.z)
}

fn point_at_terrain_height(ray: Ray3d, seed: u64, t: f32) -> Vec3 {
    let point = ray.origin + ray.direction * t;
    Vec3::new(point.x, terrain_height(seed, point.x, point.z), point.z)
}
