use bevy::prelude::*;

use crate::{
    building::Blueprint,
    colonist::{Colonist, ColonistState},
    resources::{COLONIST_CARRY_CAPACITY, CentralStorage, Inventory, PublicInventory},
    selection::{SelectedTarget, SelectionState},
    types::ResourceKind,
    ui,
    world::GameAssets,
};

#[derive(Resource, Default)]
pub struct DebugConsoleState {
    pub visible: bool,
    pub fast_build: bool,
}

#[derive(Component)]
pub struct DebugConsoleRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum DebugButton {
    AddColonist,
    AddFiveColonists,
    AddWood100,
    AddWood1000,
    AddFood100,
    AddFood1000,
    InstantFinishAll,
    InstantFinishSelected,
    ToggleFastBuild,
}

#[derive(Component)]
struct FastBuildLabel;

pub struct DebugConsolePlugin;

impl Plugin for DebugConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugConsoleState>()
            .add_systems(
                Startup,
                spawn_debug_console.after(crate::world::setup_scene),
            )
            .add_systems(
                Update,
                (
                    toggle_debug_console,
                    update_debug_visibility,
                    handle_debug_buttons,
                    fast_build_blueprints,
                    update_fast_build_label,
                ),
            );
    }
}

fn spawn_debug_console(mut commands: Commands) {
    commands
        .spawn((
            DebugConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(118.0),
                min_width: Val::Px(480.0),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(ui::PANEL),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Debug Console"),
                TextColor(Color::srgb(0.86, 0.9, 0.92)),
            ));

            // Row 1: Colonists
            parent
                .spawn((Node {
                    display: Display::Flex,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn(ui::utility_button("+1 Colonist", DebugButton::AddColonist));
                    row.spawn(ui::utility_button(
                        "+5 Colonists",
                        DebugButton::AddFiveColonists,
                    ));
                });

            // Row 2: Wood
            parent
                .spawn((Node {
                    display: Display::Flex,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn(ui::utility_button("+100 Wood", DebugButton::AddWood100));
                    row.spawn(ui::utility_button("+1000 Wood", DebugButton::AddWood1000));
                });

            // Row 3: Food
            parent
                .spawn((Node {
                    display: Display::Flex,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn(ui::utility_button("+100 Food", DebugButton::AddFood100));
                    row.spawn(ui::utility_button("+1000 Food", DebugButton::AddFood1000));
                });

            // Row 4: Building
            parent
                .spawn((Node {
                    display: Display::Flex,
                    column_gap: Val::Px(8.0),
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn(ui::utility_button(
                        "Finish All",
                        DebugButton::InstantFinishAll,
                    ));
                    row.spawn(ui::utility_button(
                        "Finish Selected",
                        DebugButton::InstantFinishSelected,
                    ));
                });

            // Row 5: Fast Build toggle (built manually for FastBuildLabel marker)
            parent
                .spawn((
                    Button,
                    DebugButton::ToggleFastBuild,
                    ui::button_node(),
                    BackgroundColor(ui::BUTTON),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        FastBuildLabel,
                        Text::new("Fast Build: OFF"),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn toggle_debug_console(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<DebugConsoleState>) {
    if keyboard.just_pressed(KeyCode::Backquote) {
        state.visible = !state.visible;
    }
}

fn update_debug_visibility(
    state: Res<DebugConsoleState>,
    mut panel: Query<&mut Node, With<DebugConsoleRoot>>,
) {
    if let Ok(mut node) = panel.single_mut() {
        node.display = if state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn handle_debug_buttons(
    mut commands: Commands,
    mut debug_state: ResMut<DebugConsoleState>,
    assets: Res<GameAssets>,
    selection: Res<SelectionState>,
    colonists: Query<&Colonist>,
    mut blueprints: Query<&mut Blueprint>,
    mut central_inventories: Query<&mut Inventory, With<CentralStorage>>,
    mut public_inventories: Query<&mut Inventory, (With<PublicInventory>, Without<CentralStorage>)>,
    mut buttons: Query<
        (&Interaction, &DebugButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button, mut color) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                match button {
                    DebugButton::AddColonist => {
                        let count = colonists.iter().count() as u32;
                        spawn_debug_colonists(&mut commands, &assets, 1, count);
                    }
                    DebugButton::AddFiveColonists => {
                        let count = colonists.iter().count() as u32;
                        spawn_debug_colonists(&mut commands, &assets, 5, count);
                    }
                    DebugButton::AddWood100 => add_debug_resource(
                        &mut central_inventories,
                        &mut public_inventories,
                        ResourceKind::Wood,
                        100,
                    ),
                    DebugButton::AddWood1000 => add_debug_resource(
                        &mut central_inventories,
                        &mut public_inventories,
                        ResourceKind::Wood,
                        1000,
                    ),
                    DebugButton::AddFood100 => add_debug_resource(
                        &mut central_inventories,
                        &mut public_inventories,
                        ResourceKind::Food,
                        100,
                    ),
                    DebugButton::AddFood1000 => add_debug_resource(
                        &mut central_inventories,
                        &mut public_inventories,
                        ResourceKind::Food,
                        1000,
                    ),
                    DebugButton::InstantFinishAll => {
                        for mut bp in &mut blueprints {
                            bp.delivered_wood = bp.required_wood;
                            bp.progress = bp.build_seconds;
                        }
                    }
                    DebugButton::InstantFinishSelected => {
                        if let Some(SelectedTarget::Blueprint(entity)) = selection.selected
                            && let Ok(mut bp) = blueprints.get_mut(entity)
                        {
                            bp.delivered_wood = bp.required_wood;
                            bp.progress = bp.build_seconds;
                        }
                    }
                    DebugButton::ToggleFastBuild => {
                        debug_state.fast_build = !debug_state.fast_build;
                    }
                }
                *color = BackgroundColor(ui::BUTTON_ACTIVE);
            }
            Interaction::Hovered => *color = BackgroundColor(ui::BUTTON_HOVER),
            Interaction::None => *color = BackgroundColor(ui::BUTTON),
        }
    }
}

fn add_debug_resource(
    central_inventories: &mut Query<&mut Inventory, With<CentralStorage>>,
    public_inventories: &mut Query<
        &mut Inventory,
        (With<PublicInventory>, Without<CentralStorage>),
    >,
    kind: ResourceKind,
    amount: i32,
) {
    let mut remaining = amount;
    for mut inventory in central_inventories.iter_mut() {
        remaining -= inventory.add_partial(kind, remaining);
        if remaining <= 0 {
            return;
        }
    }

    for mut inventory in public_inventories.iter_mut() {
        remaining -= inventory.add_partial(kind, remaining);
        if remaining <= 0 {
            return;
        }
    }
}

fn fast_build_blueprints(state: Res<DebugConsoleState>, mut blueprints: Query<&mut Blueprint>) {
    if !state.fast_build {
        return;
    }
    for mut bp in &mut blueprints {
        bp.delivered_wood = bp.required_wood;
        bp.progress = bp.build_seconds;
    }
}

fn update_fast_build_label(
    state: Res<DebugConsoleState>,
    mut texts: Query<&mut Text, With<FastBuildLabel>>,
) {
    if let Ok(mut text) = texts.single_mut() {
        text.0 = if state.fast_build {
            "Fast Build: ON".to_string()
        } else {
            "Fast Build: OFF".to_string()
        };
    }
}

fn spawn_debug_colonists(
    commands: &mut Commands,
    assets: &GameAssets,
    count: u32,
    existing_count: u32,
) {
    for i in 0..count {
        let index = existing_count + i;
        let x_off = (i % 5) as f32 * 0.8 - 1.6;
        let z_off = 2.0 + (i / 5) as f32 * 0.8;
        commands.spawn((
            Mesh3d(assets.colonist_mesh.clone()),
            MeshMaterial3d(assets.colonist_material.clone()),
            Transform::from_translation(Vec3::new(x_off, 0.32, z_off)),
            Colonist {
                name: format!("Settler {}", index + 1),
                state: ColonistState::Idle,
                speed: 2.2,
                path_rebuild_timer: 0.0,
                home: None,
                satiety: 100.0,
                carry_capacity: COLONIST_CARRY_CAPACITY,
            },
        ));
    }
}
