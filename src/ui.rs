use bevy::prelude::*;

use crate::{
    building::{
        Blueprint, BuildState, CompletedBuilding, Footprint, Housing, Workplace, WorldGeometry,
    },
    colonist::Colonist,
    farm::{CompletedFarmPlot, FarmPlot},
    resources::{CentralStorage, Inventory, PublicInventory, public_stock},
    selection::{SelectedTarget, SelectionState},
    simulation::SimulationClock,
    terrain::TerrainGenerationConfig,
    types::{BuildingKind, CONSTRUCTION_KINDS, ConstructionKind, MAP_GRID_CELLS, ResourceKind},
    world::ResourceNode,
};

pub(crate) const PANEL: Color = Color::srgba(0.08, 0.09, 0.1, 0.82);
pub(crate) const BUTTON: Color = Color::srgb(0.18, 0.2, 0.22);
pub(crate) const BUTTON_HOVER: Color = Color::srgb(0.26, 0.29, 0.31);
pub(crate) const BUTTON_ACTIVE: Color = Color::srgb(0.26, 0.42, 0.28);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiVisibility>()
            .add_systems(Startup, spawn_ui.after(crate::world::setup_scene))
            .add_systems(
                Update,
                (
                    toggle_ui_visibility,
                    update_ui_visibility,
                    handle_ui_buttons,
                    update_ui_text,
                ),
            );
    }
}

#[derive(Resource)]
pub struct UiVisibility {
    pub visible: bool,
}

impl Default for UiVisibility {
    fn default() -> Self {
        Self { visible: true }
    }
}

#[derive(Component)]
struct UiRoot;

fn toggle_ui_visibility(keyboard: Res<ButtonInput<KeyCode>>, mut visibility: ResMut<UiVisibility>) {
    if keyboard.just_pressed(KeyCode::F1) {
        visibility.visible = !visibility.visible;
    }
}

fn update_ui_visibility(visibility: Res<UiVisibility>, mut panels: Query<&mut Node, With<UiRoot>>) {
    if !visibility.is_changed() {
        return;
    }
    for mut node in &mut panels {
        node.display = if visibility.visible {
            Display::Flex
        } else {
            Display::None
        };
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
pub struct JobControlsRoot;

#[derive(Component)]
pub struct JobSlotsText;

#[derive(Component)]
pub struct JobSlotButton(pub i8);

#[derive(Component)]
pub struct BuildButton(pub ConstructionKind);

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
        UiRoot,
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
                Text::new("Wood: 0  Food: 0  Firewood: 0  Pop: 0/0  Time: 1x"),
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
        UiRoot,
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
        UiRoot,
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
            (
                JobControlsRoot,
                Node {
                    display: Display::None,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    ..default()
                },
                children![
                    job_slot_button("-", -1),
                    (
                        JobSlotsText,
                        Text::new("Workers: 0/0"),
                        TextColor(Color::srgb(0.9, 0.92, 0.86)),
                    ),
                    job_slot_button("+", 1),
                ],
            ),
        ],
    ));

    commands.spawn((
        UiRoot,
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
            build_button(CONSTRUCTION_KINDS[0]),
            build_button(CONSTRUCTION_KINDS[1]),
            build_button(CONSTRUCTION_KINDS[2]),
            build_button(CONSTRUCTION_KINDS[3]),
            build_button(CONSTRUCTION_KINDS[4]),
            build_button(CONSTRUCTION_KINDS[5]),
            build_button(CONSTRUCTION_KINDS[6]),
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
    selection: Res<SelectionState>,
    mut workplaces: Query<&mut Workplace>,
    mut build_buttons: Query<
        (&Interaction, &BuildButton, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Without<SnapButton>,
            Without<TimeButton>,
            Without<JobSlotButton>,
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
            Without<JobSlotButton>,
        ),
    >,
    mut time_buttons: Query<
        (&Interaction, &TimeButton, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Without<BuildButton>,
            Without<SnapButton>,
            Without<JobSlotButton>,
        ),
    >,
    mut job_slot_buttons: Query<
        (&Interaction, &JobSlotButton, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Without<BuildButton>,
            Without<SnapButton>,
            Without<TimeButton>,
        ),
    >,
) {
    for (interaction, button, mut color) in &mut build_buttons {
        match *interaction {
            Interaction::Pressed => {
                build_state.select_construction(button.0);
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

    for (interaction, button, mut color) in &mut job_slot_buttons {
        match *interaction {
            Interaction::Pressed => {
                if let Some(SelectedTarget::Building(entity)) = selection.selected
                    && let Ok(mut workplace) = workplaces.get_mut(entity)
                {
                    workplace.adjust_desired_slots(button.0);
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
        Option<&Workplace>,
    )>,
    farms: Query<(Entity, &CompletedFarmPlot, &Footprint)>,
    blueprints: Query<(Entity, &Blueprint, Option<&FarmPlot>, Option<&Footprint>)>,
    resource_nodes: Query<(Entity, &ResourceNode)>,
    public_inventories: Query<&Inventory, With<PublicInventory>>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<ResourceText>>,
        Query<&mut Text, With<StatusText>>,
        Query<&mut Text, With<SelectionTitle>>,
        Query<&mut Text, With<SelectionBody>>,
        Query<&mut Text, With<TerrainDebugText>>,
        Query<&mut Text, With<FpsText>>,
        Query<&mut Node, With<JobControlsRoot>>,
        Query<&mut Text, With<JobSlotsText>>,
    )>,
) {
    let stock = public_stock(public_inventories.iter());
    let population = colonists.iter().count() as i32;
    let capacity: i32 = completed
        .iter()
        .map(|(_, building, _, _, _, _)| building.kind.definition().population_capacity)
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
            "{}: {}  {}: {}  {}: {}  Pop: {}/{}  Homeless: {}  Idle: {}  Time: {}",
            ResourceKind::Wood.label(),
            stock.wood,
            ResourceKind::Food.label(),
            stock.food,
            ResourceKind::Firewood.label(),
            stock.firewood,
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
        &farms,
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
    let job_status = selected_job_status(&selection, &colonists, &completed);
    if let Ok(mut node) = text_queries.p6().single_mut() {
        node.display = if job_status.is_some() {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut text) = text_queries.p7().single_mut() {
        if let Some((assigned, desired)) = job_status {
            text.0 = format!("Workers: {}/{}", assigned, desired);
        } else {
            text.0 = "Workers: 0/0".to_string();
        }
    }
}

fn resource_node_counts(resource_nodes: &Query<(Entity, &ResourceNode)>) -> (usize, usize) {
    let mut wood = 0;
    let mut food = 0;
    for (_, node) in resource_nodes {
        match node.kind {
            ResourceKind::Wood => wood += 1,
            ResourceKind::Food => food += 1,
            ResourceKind::Firewood => {}
        }
    }

    (wood, food)
}

fn selected_job_status(
    selection: &SelectionState,
    colonists: &Query<(Entity, &Colonist)>,
    completed: &Query<(
        Entity,
        &CompletedBuilding,
        Option<&Inventory>,
        Option<&Housing>,
        Option<&CentralStorage>,
        Option<&Workplace>,
    )>,
) -> Option<(usize, u8)> {
    let Some(SelectedTarget::Building(entity)) = selection.selected else {
        return None;
    };
    let (_, _, _, _, _, workplace) = completed.get(entity).ok()?;
    let workplace = workplace?;
    Some((
        assigned_worker_count(colonists, entity),
        workplace.desired_slots,
    ))
}

fn assigned_worker_count(colonists: &Query<(Entity, &Colonist)>, workplace: Entity) -> usize {
    colonists
        .iter()
        .filter(|(_, colonist)| colonist.workplace == Some(workplace))
        .count()
}

fn build_button(kind: ConstructionKind) -> impl Bundle {
    let label = format!("{} {}", kind.hotkey_label(), kind.label());
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

fn job_slot_button(label: &'static str, delta: i8) -> impl Bundle {
    (
        Button,
        JobSlotButton(delta),
        Node {
            min_width: px(34),
            height: px(30),
            display: Display::Flex,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::axes(px(8), px(4)),
            ..default()
        },
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

fn selected_panel_text(
    selection: &SelectionState,
    colonists: &Query<(Entity, &Colonist)>,
    completed: &Query<(
        Entity,
        &CompletedBuilding,
        Option<&Inventory>,
        Option<&Housing>,
        Option<&CentralStorage>,
        Option<&Workplace>,
    )>,
    farms: &Query<(Entity, &CompletedFarmPlot, &Footprint)>,
    blueprints: &Query<(Entity, &Blueprint, Option<&FarmPlot>, Option<&Footprint>)>,
    resource_nodes: &Query<(Entity, &ResourceNode)>,
) -> (String, String) {
    let Some(selected) = selection.selected else {
        return (
            "Nothing selected".to_string(),
            "Click a settler, building, farm, blueprint, or resource node.".to_string(),
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
                let workplace = colonist
                    .workplace
                    .map(|entity| format!("{entity:?}"))
                    .unwrap_or_else(|| "None".to_string());
                (
                    colonist.name.clone(),
                    format!(
                        "Status: {}\nProfession: {}\nWorkplace: {}\nSatiety: {:.0}/100\nHome: {}\nSpeed: {:.1}",
                        colonist.status_label(),
                        colonist.profession.label(),
                        workplace,
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
                    ResourceKind::Wood => Some(BuildingKind::Woodcutter),
                    ResourceKind::Food => Some(BuildingKind::Gatherer),
                    ResourceKind::Firewood => None,
                };
                let enabled = required_building
                    .map(|required_building| {
                        completed
                            .iter()
                            .any(|(_, building, _, _, _, _)| building.kind == required_building)
                    })
                    .unwrap_or(false);
                let used_by = required_building
                    .map(|required_building| required_building.definition().label)
                    .unwrap_or("Chopping Yard");
                (
                    format!("{} node", node.kind.label()),
                    format!(
                        "Remaining: {}\nUsed by: {}\nReady to gather: {}",
                        node.amount,
                        used_by,
                        if enabled { "Yes" } else { "Needs building" }
                    ),
                )
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Blueprint(entity) => blueprints
            .get(entity)
            .map(|(_, blueprint, farm_plot, footprint)| {
                let label = blueprint.kind.label();
                let area_cells = farm_plot.map(|plot| plot.area_cells).or_else(|| {
                    footprint.map(|footprint| {
                        crate::building::polygon_area(&footprint.polygon)
                            / crate::types::CELL_SIZE.powi(2)
                    })
                });
                let material_line = if blueprint.kind == ConstructionKind::Farm {
                    "No materials required."
                } else if blueprint.needs_wood() > 0 {
                    "Settlers will deliver wood when stock is available."
                } else {
                    "Waiting for a builder to finish construction."
                };
                let mut body = format!(
                    "Status: {}\nWood: {}/{}\nConstruction: {:.0}%\n{}",
                    blueprint.status().label(),
                    blueprint.delivered_wood,
                    blueprint.required_wood,
                    blueprint.progress_ratio() * 100.0,
                    material_line
                );
                if let Some(area_cells) = area_cells {
                    body.push_str(&format!("\nArea: {:.1} cells", area_cells));
                }
                (format!("{} blueprint", label), body)
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Building(entity) => completed
            .get(entity)
            .map(|(_, building, inventory, housing, central, workplace)| {
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
                        "\nInventory: Wood {}  Food {}  Firewood {}\nCapacity: {}/{}",
                        inventory.wood,
                        inventory.food,
                        inventory.firewood,
                        inventory.used_capacity(),
                        inventory.capacity
                    ));
                }

                if let Some(workplace) = workplace {
                    let assigned = assigned_worker_count(colonists, entity);
                    body.push_str(&format!(
                        "\nWorkers: {}/{}  Profession: {}",
                        assigned,
                        workplace.desired_slots,
                        workplace.profession.label()
                    ));
                }

                (title, body)
            })
            .unwrap_or_else(|_| missing_selection()),
        SelectedTarget::Farm(entity) => farms
            .get(entity)
            .map(|(_, farm, _)| {
                (
                    "Farm plot".to_string(),
                    format!(
                        "{}\nStatus: Built\nArea: {:.1} cells",
                        ConstructionKind::Farm.description(),
                        farm.area_cells
                    ),
                )
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
