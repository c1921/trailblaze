use bevy::prelude::*;

use crate::types::BuildingKind;

#[derive(Resource, Debug)]
pub struct BuildState {
    pub selected: Option<BuildingKind>,
    pub snap_to_grid: bool,
    pub rotation_angle: f32,
    pub r_hold_timer: f32,
    pub preview_entity: Option<Entity>,
    pub preview_entrance_entity: Option<Entity>,
    pub last_valid: bool,
    pub last_position: Vec3,
    pub last_polygon: Vec<Vec2>,
    pub invalid_reason: Option<PlacementIssue>,
    pub status: String,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            selected: None,
            snap_to_grid: true,
            rotation_angle: 0.0,
            r_hold_timer: 0.0,
            preview_entity: None,
            preview_entrance_entity: None,
            last_valid: false,
            last_position: Vec3::ZERO,
            last_polygon: Vec::new(),
            invalid_reason: None,
            status: "Select a building to start planning.".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementIssue {
    OutOfBounds,
    Occupied,
    EntranceBlocked,
}

impl PlacementIssue {
    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfBounds => "outside the buildable area",
            Self::Occupied => "blocked by another plan, building, resource, or entrance",
            Self::EntranceBlocked => "the entrance is blocked",
        }
    }
}
