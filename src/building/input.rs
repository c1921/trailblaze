use bevy::prelude::*;

use crate::types::{CONSTRUCTION_KINDS, ConstructionKind};

use super::BuildState;

pub fn handle_build_hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut build_state: ResMut<BuildState>,
) {
    for construction in CONSTRUCTION_KINDS {
        if keyboard.just_pressed(construction.hotkey()) {
            build_state.select_construction(construction);
        }
    }

    if keyboard.just_pressed(KeyCode::KeyG) {
        build_state.snap_to_grid = !build_state.snap_to_grid;
        build_state.status = format!(
            "Grid snap {}.",
            if build_state.snap_to_grid {
                "on"
            } else {
                "off"
            }
        );
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        build_state.cancel();
    } else if build_state.selected == Some(ConstructionKind::Farm)
        && mouse_buttons.just_pressed(MouseButton::Right)
    {
        if build_state.farm_points.pop().is_some() {
            build_state.last_valid = false;
            build_state.invalid_reason = None;
            build_state.status = if build_state.farm_points.is_empty() {
                "Planning Farm. Click to place the first corner.".to_string()
            } else {
                format!(
                    "Removed corner. Farm has {} corners.",
                    build_state.farm_points.len()
                )
            };
        }
    } else if build_state.selected.is_some() && mouse_buttons.just_pressed(MouseButton::Right) {
        build_state.cancel();
    }
}

pub fn handle_rotation_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut build_state: ResMut<BuildState>,
) {
    if build_state
        .selected
        .and_then(ConstructionKind::as_building)
        .is_none()
    {
        return;
    }

    if build_state.snap_to_grid {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_state.rotation_angle = (build_state.rotation_angle + std::f32::consts::FRAC_PI_2)
                .rem_euclid(std::f32::consts::TAU);
        }
    } else {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_state.r_hold_timer = 0.0;
        }
        if keyboard.pressed(KeyCode::KeyR) {
            build_state.r_hold_timer += time.delta_secs();
            if build_state.r_hold_timer >= 0.2 {
                build_state.rotation_angle = (build_state.rotation_angle
                    + std::f32::consts::PI * time.delta_secs())
                .rem_euclid(std::f32::consts::TAU);
            }
        }
        if keyboard.just_released(KeyCode::KeyR) {
            if build_state.r_hold_timer > 0.0 && build_state.r_hold_timer < 0.2 {
                build_state.rotation_angle = (build_state.rotation_angle
                    + std::f32::consts::FRAC_PI_2)
                    .rem_euclid(std::f32::consts::TAU);
            }
            build_state.r_hold_timer = 0.0;
        }
    }
}
