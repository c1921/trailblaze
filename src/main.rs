mod building;
mod camera;
mod colonist;
mod debug_console;
mod math;
mod navigation;
mod resources;
mod selection;
mod simulation;
mod terrain;
mod types;
mod ui;
mod world;

use bevy::prelude::*;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.76, 0.8, 0.86)))
        .init_resource::<resources::ResourceStock>()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            simulation::SimulationPlugin,
            camera::CameraPlugin,
            terrain::TerrainPlugin,
            world::WorldPlugin,
            building::BuildingPlugin,
            selection::SelectionPlugin,
            colonist::ColonistPlugin,
            ui::UiPlugin,
            debug_console::DebugConsolePlugin,
        ))
        .run();
}
