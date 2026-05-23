mod building;
mod camera;
mod colonist;
mod navigation;
mod resources;
mod selection;
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
        .init_resource::<building::WorldGeometry>()
        .init_resource::<selection::SelectionState>()
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
                building::handle_rotation_input,
                building::update_build_preview,
                building::place_blueprint,
                selection::select_target,
                colonist::assign_idle_colonists,
                colonist::update_colonists,
                building::update_blueprint_visuals,
                building::finish_blueprints,
                building::sync_entrance_markers,
                selection::draw_selection_highlight,
                ui::update_ui_text,
            ),
        )
        .run();
}
