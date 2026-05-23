mod components;
mod geometry;
mod input;
mod lifecycle;
mod placement;
mod polygon;
mod state;

pub use components::*;
pub use geometry::WorldGeometry;
pub use polygon::{
    expanded_polygon, footprint_polygon, resource_obstacle_polygon, segment_intersects_polygon,
};
pub use state::{BuildState, PlacementIssue};

#[cfg(test)]
pub use polygon::{point_in_polygon, rectangle_polygon};

use bevy::prelude::*;

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildState>()
            .init_resource::<WorldGeometry>()
            .add_systems(
                Update,
                (
                    input::handle_build_hotkeys,
                    input::handle_rotation_input,
                    placement::update_build_preview,
                    placement::place_blueprint,
                    lifecycle::update_blueprint_visuals,
                    lifecycle::finish_blueprints,
                    lifecycle::sync_entrance_markers,
                ),
            );
    }
}
