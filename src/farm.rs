use bevy::prelude::*;

use crate::{
    building::{
        PlacementIssue, WorldGeometry, is_convex_polygon, polygon_area,
        polygon_has_self_intersection, signed_polygon_area,
    },
    math::within_world_bounds,
    terrain::terrain_height,
    types::CELL_SIZE,
};

pub const FARM_CLOSE_DISTANCE: f32 = CELL_SIZE * 0.55;
pub const FARM_ACCESS_OFFSET: f32 = CELL_SIZE * 0.75;
pub const FARM_SURFACE_Y_OFFSET: f32 = 0.035;

#[derive(Component, Debug)]
pub struct FarmPlot {
    pub area_cells: f32,
}

#[derive(Component, Debug)]
pub struct CompletedFarmPlot {
    pub area_cells: f32,
}

#[derive(Component, Debug)]
pub struct FarmVisual {
    pub owner: Entity,
}

pub fn validate_farm_plan(
    geometry: &WorldGeometry,
    polygon: &[Vec2],
    access_point: Option<Vec3>,
    seed: u64,
    max_slope: f32,
) -> Option<PlacementIssue> {
    if polygon.len() < 3 {
        return Some(PlacementIssue::TooFewPoints);
    }
    if polygon_has_self_intersection(polygon) || !is_convex_polygon(polygon) {
        return Some(PlacementIssue::InvalidShape);
    }

    if let Some(issue) = geometry.placement_issue_for_polygon(polygon, None, true, seed, max_slope)
    {
        return Some(issue);
    }

    let Some(access_point) = access_point else {
        return Some(PlacementIssue::EntranceBlocked);
    };
    geometry.placement_issue_for_polygon(polygon, Some(access_point), true, seed, max_slope)
}

pub fn farm_area_cells(polygon: &[Vec2]) -> f32 {
    polygon_area(polygon) / (CELL_SIZE * CELL_SIZE)
}

pub fn farm_build_seconds(polygon: &[Vec2]) -> f32 {
    (farm_area_cells(polygon) * 0.25).max(1.0)
}

pub fn farm_origin(seed: u64, polygon: &[Vec2]) -> Vec3 {
    let center = polygon_centroid(polygon);
    Vec3::new(center.x, terrain_height(seed, center.x, center.y), center.y)
}

pub fn farm_access_point(seed: u64, polygon: &[Vec2]) -> Option<Vec3> {
    if polygon.len() < 3 {
        return None;
    }

    let signed_area = signed_polygon_area(polygon);
    if signed_area.abs() <= 0.0001 {
        return None;
    }

    let mut candidates: Vec<(f32, Vec2)> = Vec::new();
    for index in 0..polygon.len() {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];
        let edge = end - start;
        let length = edge.length();
        if length <= 0.0001 {
            continue;
        }

        let outward = if signed_area > 0.0 {
            Vec2::new(edge.y, -edge.x)
        } else {
            Vec2::new(-edge.y, edge.x)
        }
        .normalize();
        let candidate = (start + end) * 0.5 + outward * FARM_ACCESS_OFFSET;
        candidates.push((length, candidate));
    }

    candidates.sort_by(|(left, _), (right, _)| right.total_cmp(left));
    candidates
        .into_iter()
        .map(|(_, point)| point)
        .find(|point| within_world_bounds(*point))
        .map(|point| Vec3::new(point.x, terrain_height(seed, point.x, point.y), point.y))
}

pub fn farm_surface_mesh(seed: u64, polygon: &[Vec2], origin: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(polygon.len() + 1);
    let mut normals = Vec::with_capacity(polygon.len() + 1);
    let mut uvs = Vec::with_capacity(polygon.len() + 1);
    let mut indices = Vec::with_capacity(polygon.len() * 3);

    positions.push([0.0, FARM_SURFACE_Y_OFFSET, 0.0]);
    normals.push([0.0, 1.0, 0.0]);
    uvs.push([0.5, 0.5]);

    let bounds = polygon_bounds(polygon);
    let extent = (bounds.1 - bounds.0).max(Vec2::splat(0.001));
    for point in polygon {
        let height = terrain_height(seed, point.x, point.y);
        positions.push([
            point.x - origin.x,
            height - origin.y + FARM_SURFACE_Y_OFFSET,
            point.y - origin.z,
        ]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([
            (point.x - bounds.0.x) / extent.x,
            (point.y - bounds.0.y) / extent.y,
        ]);
    }

    let signed_area = signed_polygon_area(polygon);
    for index in 0..polygon.len() {
        let current = index as u32 + 1;
        let next = ((index + 1) % polygon.len()) as u32 + 1;
        if signed_area > 0.0 {
            indices.extend([0, next, current]);
        } else {
            indices.extend([0, current, next]);
        }
    }

    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

pub fn polygon_centroid(polygon: &[Vec2]) -> Vec2 {
    if polygon.is_empty() {
        return Vec2::ZERO;
    }

    polygon
        .iter()
        .copied()
        .fold(Vec2::ZERO, |sum, point| sum + point)
        / polygon.len() as f32
}

fn polygon_bounds(polygon: &[Vec2]) -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for point in polygon {
        min = min.min(*point);
        max = max.max(*point);
    }

    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{building::footprint_polygon, terrain::DEFAULT_TERRAIN_SEED, types::BuildingKind};

    const SEED: u64 = DEFAULT_TERRAIN_SEED;

    fn square() -> Vec<Vec2> {
        vec![
            Vec2::new(2.0, 2.0),
            Vec2::new(4.0, 2.0),
            Vec2::new(4.0, 4.0),
            Vec2::new(2.0, 4.0),
        ]
    }

    #[test]
    fn farm_area_uses_cell_area() {
        assert_eq!(farm_area_cells(&square()), 4.0);
    }

    #[test]
    fn farm_validation_accepts_convex_polygon() {
        let geometry = WorldGeometry::default();
        let polygon = square();

        assert_eq!(
            validate_farm_plan(
                &geometry,
                &polygon,
                farm_access_point(SEED, &polygon),
                SEED,
                10.0
            ),
            None
        );
    }

    #[test]
    fn farm_validation_rejects_bad_shapes_and_too_few_points() {
        let geometry = WorldGeometry::default();
        let concave = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(1.0, 0.5),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];

        assert_eq!(
            validate_farm_plan(&geometry, &square()[..2], None, SEED, 10.0),
            Some(PlacementIssue::TooFewPoints)
        );
        assert_eq!(
            validate_farm_plan(
                &geometry,
                &concave,
                farm_access_point(SEED, &concave),
                SEED,
                10.0
            ),
            Some(PlacementIssue::InvalidShape)
        );
    }

    #[test]
    fn farm_validation_rejects_overlap_bounds_and_slope() {
        let mut world = World::new();
        let blocker = world.spawn_empty().id();
        let mut geometry = WorldGeometry::default();
        let building = footprint_polygon(
            BuildingKind::Storage,
            Vec3::new(3.0, 0.0, 3.0),
            BuildingKind::Storage.definition().size,
            0.0,
        );
        geometry.occupy_polygon(building, blocker, false);

        let overlap = square();
        assert_eq!(
            validate_farm_plan(
                &geometry,
                &overlap,
                farm_access_point(SEED, &overlap),
                SEED,
                10.0
            ),
            Some(PlacementIssue::Occupied)
        );

        let out_of_bounds = vec![
            Vec2::new(1000.0, 1000.0),
            Vec2::new(1001.0, 1000.0),
            Vec2::new(1001.0, 1001.0),
            Vec2::new(1000.0, 1001.0),
        ];
        assert_eq!(
            validate_farm_plan(
                &WorldGeometry::default(),
                &out_of_bounds,
                farm_access_point(SEED, &out_of_bounds),
                SEED,
                10.0
            ),
            Some(PlacementIssue::OutOfBounds)
        );

        let open = vec![
            Vec2::new(20.0, 20.0),
            Vec2::new(22.0, 20.0),
            Vec2::new(22.0, 22.0),
            Vec2::new(20.0, 22.0),
        ];
        assert_eq!(
            validate_farm_plan(
                &WorldGeometry::default(),
                &open,
                farm_access_point(SEED, &open),
                SEED,
                0.0
            ),
            Some(PlacementIssue::TooSteep)
        );
    }

    #[test]
    fn occupied_farm_polygon_blocks_navigation_and_overlap() {
        let mut world = World::new();
        let farm = world.spawn_empty().id();
        let mut geometry = WorldGeometry::default();
        let polygon = square();

        geometry.occupy_polygon(polygon.clone(), farm, false);

        assert!(!geometry.is_walkable_point(Vec3::new(3.0, 0.0, 3.0)));
        assert_eq!(
            geometry.placement_issue_for_polygon(&polygon, None, true, SEED, 10.0),
            Some(PlacementIssue::Occupied)
        );
    }

    #[test]
    fn farm_blueprint_needs_no_materials() {
        let blueprint = crate::building::Blueprint {
            kind: crate::types::ConstructionKind::Farm,
            required_wood: 0,
            delivered_wood: 0,
            progress: 1.0,
            build_seconds: 1.0,
        };

        assert!(blueprint.has_materials());
        assert!(blueprint.is_complete());
    }
}
