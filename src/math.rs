use bevy::prelude::*;

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
