use bevy::prelude::*;

use crate::types::ConstructionKind;

#[derive(Resource, Debug)]
pub struct BuildState {
    pub selected: Option<ConstructionKind>,
    pub snap_to_grid: bool,
    pub rotation_angle: f32,
    pub r_hold_timer: f32,
    pub preview_entity: Option<Entity>,
    pub preview_entrance_entity: Option<Entity>,
    pub last_valid: bool,
    pub last_position: Vec3,
    pub last_polygon: Vec<Vec2>,
    pub last_access_point: Option<Vec3>,
    pub farm_points: Vec<Vec2>,
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
            last_access_point: None,
            farm_points: Vec::new(),
            invalid_reason: None,
            status: "Select a building to start planning.".to_string(),
        }
    }
}

impl BuildState {
    pub fn select_construction(&mut self, construction: ConstructionKind) {
        self.selected = Some(construction);
        self.last_valid = false;
        self.last_polygon.clear();
        self.last_access_point = None;
        self.invalid_reason = None;
        self.farm_points.clear();
        self.status = match construction {
            ConstructionKind::Building(kind) => {
                format!("Planning {}.", kind.definition().label)
            }
            ConstructionKind::Farm => "Planning Farm. Click to place the first corner.".to_string(),
        };
    }

    pub fn cancel(&mut self) {
        self.selected = None;
        self.last_valid = false;
        self.last_polygon.clear();
        self.last_access_point = None;
        self.invalid_reason = None;
        self.farm_points.clear();
        self.status = "Build mode cancelled.".to_string();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementIssue {
    OutOfBounds,
    Occupied,
    EntranceBlocked,
    TooSteep,
    TooFewPoints,
    InvalidShape,
}

impl PlacementIssue {
    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfBounds => "outside the buildable area",
            Self::Occupied => "blocked by another plan, building, resource, or entrance",
            Self::EntranceBlocked => "the entrance is blocked",
            Self::TooSteep => "the terrain is too steep",
            Self::TooFewPoints => "at least three corners are required",
            Self::InvalidShape => "the outline must be a convex non-overlapping polygon",
        }
    }
}
