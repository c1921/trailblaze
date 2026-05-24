use bevy::prelude::*;

use crate::{
    building::{Blueprint, BuildState, CompletedBuilding, Housing, WorldGeometry},
    colonist::Colonist,
    resources::{CentralStorage, Inventory, PublicInventory, public_stock},
    selection::{SelectedTarget, SelectionState},
    simulation::SimulationClock,
    terrain::TerrainGenerationConfig,
    types::{BuildingKind, MAP_GRID_CELLS, ResourceKind},
    world::ResourceNode,
};

pub(crate) const PANEL: Color = Color::srgba(0.08, 0.09, 0.1, 0.82);
pub(crate) const BUTTON: Color = Color::srgb(0.18, 0.2, 0.22);
pub(crate) const BUTTON_HOVER: Color = Color::srgb(0.26, 0.29, 0.31);
pub(crate) const BUTTON_ACTIVE: Color = Color::srgb(0.26, 0.42, 0.28);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ui.after(crate::world::setup_scene))
            .add_systems(Update, (handle_ui_buttons, update_ui_text));
    }
}

#[derive(Component)]
pub struct ResourceText;

#[derive(Component)]
pub struct StatusText;

#[derive(Component)]
pub struct TerrainDebugText;

#[derive(Component)]
pub struct SelectionTitle;

#[derive(Component)]
pub struct SelectionBody;

#[derive(Component)]
pub struct BuildButton(pub BuildingKind);

#[derive(Component)]
pub enum TimeButton {
    Pause,
    Speed(f32),
}

#[derive(Component)]
pub struct SnapButton;

#[derive(Component)]
pub struct FpsText;

pub fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            right: px(12),
            top: px(12),
            height: px(44),
            display: Display::Flex,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::axes(px(14), px(8)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            (
                ResourceText,
                Text::new("Wood: 0  Food: 0  Pop: 0/0  Time: 1x"),
                TextColor(Color::WHITE),
            ),
            (
                StatusText,
                Text::new("Select a building."),
                TextColor(Color::srgb(0.86, 0.9, 0.92)),
            ),
            (
                FpsText,
                Text::new("FPS: --"),
                TextColor(Color::srgb(0.86, 0.9, 0.92)),
            )
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(68),
            width: px(470),
            min_height: px(38),
            display: Display::Flex,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(12), px(6)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![(
            TerrainDebugText,
            Text::new("Map: 96x96  Seed: 0x0000000000000000  Nodes: Wood 0 / Food 0"),
            TextColor(Color::srgb(0.84, 0.88, 0.9)),
        )],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            top: px(68),
            width: px(330),
            min_height: px(220),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            padding: UiRect::all(px(14)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            (
                SelectionTitle,
                Text::new("Nothing selected"),
                TextColor(Color::WHITE),
            ),
            (
                SelectionBody,
                Text::new("Click a settler, building, blueprint, or resource node."),
                TextColor(Color::srgb(0.84, 0.88, 0.9)),
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            display: Display::Flex,
            column_gap: px(8),
            align_items: AlignItems::Center,
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            build_button(BuildingKind::House),
            build_button(BuildingKind::Storage),
            build_button(BuildingKind::Woodcutter),
            build_button(BuildingKind::Gatherer),
            build_button(BuildingKind::Road),
            utility_button("Snap G", SnapButton),
            time_button("Pause", TimeButton::Pause),
            time_button("1x", TimeButton::Speed(1.0)),
            time_button("2x", TimeButton::Speed(2.0)),
            time_button("4x", TimeButton::Speed(4.0)),
        ],
    ));
}

pub fn handle_ui_buttons(
    mut build_state: ResMut<BuildState>,
    mut clock: ResMut<SimulationClock>,
    mut build_buttons: Query<
        (&Interaction, &BuildButton, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Without<SnapButton>,
            Without<TimeButton>,
        ),
    >,
    mut snap_buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            With<SnapButton>,
            Without<BuildButton>,
            Without<TimeButton>,
        ),
    >,
    mut time_buttons: Query<
        (&Interaction, &TimeButton, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Without<BuildButton>,
            Without<SnapButton>,
        ),
    >,
) {
    for (interaction, button, mut color) in &mut build_buttons {
        match *interaction {
            Interaction::Pressed => {
                build_state.selected = Some(button.0);
                build_state.status = format!("Planning {}.", button.0.definition().label);
                *color = BackgroundColor(BUTTON_ACTIVE);
            }
            Interaction::Hovered => *color = BackgroundColor(BUTTON_HOVER),
            Interaction::None => *color = BackgroundColor(BUTTON),
        }
    }

    for (interaction, mut color) in &mut snap_buttons {
        match *interaction {
            Interaction::Pressed => {
                build_state.snap_to_grid = !build_state.snap_to_grid;
                *color = BackgroundColor(BUTTON_ACTIVE);
            }
            Interaction::Hovered => *color = BackgroundColor(BUTTON_HOVER),
            Interaction::None => *color = BackgroundColor(BUTTON),
        }
    }

    for (interaction, button, mut color) in &mut time_buttons {
        match *interaction {
            Interaction::Pressed => {
                match button {
                    TimeButton::Pause => clock.paused = !clock.paused,
                    TimeButton::Speed(speed) => {
                        clock.paused = false;
                        clock.speed = *speed;
                    }
                }
                *color = BackgroundColor(BUTTON_ACTIVE);
            }
            Interaction::Hovered => *color = BackgroundColor(BUTTON_HOVER),
            Interaction::None => *color = BackgroundColor(BUTTON),
        }
    }
}

pub fn update_ui_text(
    clock: Res<SimulationClock>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    terrain_config: Res<TerrainGenerationConfig>,
    build_state: Res<BuildState>,
    geometry: Res<WorldGeometry>,
    selection: Res<SelectionState>,
    colonists: Query<(Entity, &Colonist)>,
    completed: Query<(
        Entity,
        &CompletedBuilding,
        Option<&Inventory>,
        Option<&Housing>,
        Option<&CentralStorage>,
    )>,
    blueprints: Query<(Entity, &Blueprint)>,
    resource_nodes: Query<(Entity, &ResourceNode)>,
    public_inventories: Query<&Inventory, With<PublicInventory>>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<ResourceText>>,
        Query<&mut Text, With<StatusText>>,
        Query<&mut Text, With<SelectionTitle>>,
        Query<&mut Text, With<SelectionBody>>,
        Query<&mut Text, With<TerrainDebugText>>,
        Query<&mut Text, With<FpsText>>,
    )>,
) {
    let stock = public_stock(public_inventories.iter());
    let population = colonists.iter().count() as i32;
    let capacity: i32 = completed
        .iter()
        .map(|(_, building, _, _, _)| building.kind.definition().population_capacity)
        .sum();
    let homeless = colonists
        .iter()
        .filter(|(_, colonist)| colonist.home.is_none())
        .count();
    let idle_count = colonists
        .iter()
        .filter(|(_, colonist)| matches!(colonist.state, crate::colonist::ColonistState::Idle))
        .count();
    let (obstacles, road_obstacles, _) = geometry.summary();
    let (wood_nodes, food_nodes) = resource_node_counts(&resource_nodes);

    if let Ok(mut text) = text_queries.p0().single_mut() {
        text.0 = format!(
            "{}: {}  {}: {}  Pop: {}/{}  Homeless: {}  Idle: {}  Time: {}",
            ResourceKind::Wood.label(),
            stock.wood,
            ResourceKind::Food.label(),
            stock.food,
            population,
            capacity,
            homeless,
            idle_count,
            clock.label()
        );
    }

    if let Ok(mut text) = text_queries.p1().single_mut() {
        text.0 = format!(
            "{}  Obstacles: {}  Roads: {}  Snap: {}",
            build_state.status,
            obstacles,
            road_obstacles,
            if build_state.snap_to_grid {
                "On"
            } else {
                "Off"
            }
        );
    }

    let (title, body) = selected_panel_text(
        &selection,
        &colonists,
        &completed,
        &blueprints,
        &resource_nodes,
    );
    if let Ok(mut text) = text_queries.p2().single_mut() {
        text.0 = title;
    }
    if let Ok(mut text) = text_queries.p3().single_mut() {
        text.0 = body;
    }
    if let Ok(mut text) = text_queries.p4().single_mut() {
        text.0 = format!(
            "Map: {}x{}  Seed: 0x{:016X}  Nodes: Wood {} / Food {}",
            MAP_GRID_CELLS, MAP_GRID_CELLS, terrain_config.seed, wood_nodes, food_nodes
        );
    }
    if let Ok(mut text) = text_queries.p5().single_mut() {
        let fps = diagnostics
            .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .map(|f| f as i32)
            .unwrap_or(0);
        text.0 = format!("FPS: {}", fps);
    }
}

fn resource_node_counts(resource_nodes: &Query<(Entity, &ResourceNode)>) -> (usize, usize) {
    let mut wood = 0;
    let mut food = 0;
    for (_, node) in resource_nodes {
        match node.kind {
            ResourceKind::Wood => wood += 1,
            ResourceKind::Food => food += 1,
        }
    }

    (wood, food)
}

fn build_button(kind: BuildingKind) -> impl Bundle {
    let definition = kind.definition();
    let label = format!("{} {}", hotkey_label(kind), definition.label);
    (
        Button,
        BuildButton(kind),
        button_node(),
        BackgroundColor(BUTTON),
        children![(Text::new(label), TextColor(Color::WHITE))],
    )
}

pub(crate) fn utility_button<T: Component>(label: &'static str, marker: T) -> impl Bundle {
    (
        Button,
        marker,
        button_node(),
        BackgroundColor(BUTTON),
        children![(Text::new(label), TextColor(Color::WHITE))],
    )
}

fn time_button(label: &'static str, marker: TimeButton) -> impl Bundle {
    (
        Button,
        marker,
        button_node(),
        BackgroundColor(BUTTON),
        children![(Text::new(label), TextColor(Color::WHITE))],
    )
}

pub(crate) fn button_node() -> Node {
    Node {
        min_width: px(72),
        height: px(34),
        display: Display::Flex,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        padding: UiRect::axes(px(10), px(4)),
        ..default()
    }
}

fn hotkey_label(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::House => "1",
        BuildingKind::Storage => "2",
        BuildingKind::Woodcutter => "3",
        BuildingKind::Gatherer => "4",
        BuildingKind::Road => "5",
    }
}

fn selected_panel_text(
    selection: &SelectionState,
    colonists: &Query<(Entity, &Colonist)>,
    completed: &Query<(
        Entity,
        &CompletedBuilding,
        Option<&Inventory>,
        Option<&Housing>,
        Option<&CentralStorage>,
    )>,
    blueprints: &Query<(Entity, &Blueprint)>,
    resource_nodes: &Query<(Entity, &ResourceNode)>,
) -> (String, String) {
    let Some(selected) = selection.selected else {
        return (
            "Nothing selected".to_string(),
            "Click a settler, building, blueprint, or resource node.".to_string(),
        );
    };

    match selected {
        SelectedTarget::Colonist(entity) => colonists
            .get(entity)
            .map(|(_, colonist)| {
                let home = colonist
                    .home
                    .map(|entity| format!("{entity:?}"))
                    .unwrap_or_else(|| "None".to_string());
                (
                    colonist.name.clone(),
                    format!(
                        "Status: {}\nSatiety: {:.0}/100\nHome: {}\nSpeed: {:.1}",
                        colonist.status_label(),
                        colonist.satiety,
                        home,
                        colonist.speed
                    ),
                )
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Resource(entity) => resource_nodes
            .get(entity)
            .map(|(_, node)| {
                let required_building = match node.kind {
                    ResourceKind::Wood => BuildingKind::Woodcutter,
                    ResourceKind::Food => BuildingKind::Gatherer,
                };
                let enabled = completed
                    .iter()
                    .any(|(_, building, _, _, _)| building.kind == required_building);
                (
                    format!("{} node", node.kind.label()),
                    format!(
                        "Remaining: {}\nUsed by: {}\nReady to gather: {}",
                        node.amount,
                        required_building.definition().label,
                        if enabled { "Yes" } else { "Needs building" }
                    ),
                )
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Blueprint(entity) => blueprints
            .get(entity)
            .map(|(_, blueprint)| {
                let definition = blueprint.kind.definition();
                (
                    format!("{} blueprint", definition.label),
                    format!(
                        "Status: {}\nWood: {}/{}\nConstruction: {:.0}%\n{}",
                        blueprint.status().label(),
                        blueprint.delivered_wood,
                        blueprint.required_wood,
                        blueprint.progress_ratio() * 100.0,
                        if blueprint.needs_wood() > 0 {
                            "Settlers will deliver wood when stock is available."
                        } else {
                            "Waiting for a builder to finish construction."
                        }
                    ),
                )
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Building(entity) => completed
            .get(entity)
            .map(|(_, building, inventory, housing, central)| {
                let definition = building.kind.definition();
                let title = if central.is_some() {
                    "Central Storage".to_string()
                } else {
                    definition.label.to_string()
                };
                let mut body = format!("{}\nStatus: Operating", building.kind.description());

                if let Some(housing) = housing {
                    body.push_str(&format!(
                        "\nResidents: {}/{}",
                        housing.resident_count(),
                        Housing::CAPACITY
                    ));
                } else if definition.population_capacity > 0 {
                    body.push_str(&format!("\nCapacity: {}", definition.population_capacity));
                }

                if let Some(inventory) = inventory {
                    body.push_str(&format!(
                        "\nInventory: Wood {}  Food {}\nCapacity: {}/{}",
                        inventory.wood,
                        inventory.food,
                        inventory.used_capacity(),
                        inventory.capacity
                    ));
                }

                (title, body)
            })
            .unwrap_or_else(|_| missing_selection()),
    }
}

fn missing_selection() -> (String, String) {
    (
        "Selection lost".to_string(),
        "The selected object is no longer available.".to_string(),
    )
}
