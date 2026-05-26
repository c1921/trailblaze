mod building;
mod camera;
mod colonist;
mod debug_console;
mod farm;
mod math;
mod navigation;
mod resources;
mod selection;
mod simulation;
mod terrain;
mod types;
mod ui;
mod world;

use bevy::pbr::wireframe::WireframePlugin;
use bevy::prelude::*;
use bevy::render::RenderDebugFlags;

fn main() {
    let hide_ui = std::env::args().any(|arg| arg == "--hide-ui");
    let wireframe = std::env::args().any(|arg| arg == "--wireframe");

    let mut app = App::new();

    app.insert_resource(ClearColor(Color::srgb(0.76, 0.8, 0.86)));

    if hide_ui {
        app.insert_resource(ui::UiVisibility { visible: false });
    }
    if wireframe {
        app.insert_resource(debug_console::DebugConsoleState {
            wireframe_mode: true,
            ..default()
        });
    }

    app.add_plugins(DefaultPlugins)
        .add_plugins(WireframePlugin {
            debug_flags: RenderDebugFlags::empty(),
        })
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
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
