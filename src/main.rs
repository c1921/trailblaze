mod building;
mod camera;
mod colonist;
mod resources;
mod simulation;
mod types;
mod ui;
mod world;

use bevy::prelude::*;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.76, 0.8, 0.86)))
        .init_resource::<resources::ResourceStock>()
        .init_resource::<simulation::SimulationClock>()
        .init_resource::<building::BuildState>()
        .init_resource::<building::MapGrid>()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (world::setup_scene, ui::spawn_ui).chain())
        .add_systems(
            Update,
            (
                simulation::control_time,
                camera::control_camera,
                camera::draw_grid,
                ui::handle_ui_buttons,
                building::handle_build_hotkeys,
                building::update_build_preview,
                building::place_blueprint,
                colonist::assign_idle_colonists,
                colonist::update_colonists,
                building::finish_blueprints,
                ui::update_ui_text,
            ),
        )
        .run();
}
