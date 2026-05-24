use bevy::prelude::*;

use crate::types::BuildingKind;

#[derive(Component)]
pub struct BuildPreview;

#[derive(Component)]
pub struct EntrancePreview;

#[derive(Component, Debug)]
pub struct BuildingVisual {
    pub owner: Entity,
}

#[derive(Component, Debug)]
pub struct Footprint {
    pub polygon: Vec<Vec2>,
    pub passable: bool,
}

#[derive(Component, Debug)]
pub struct Blueprint {
    pub kind: BuildingKind,
    pub required_wood: i32,
    pub delivered_wood: i32,
    pub progress: f32,
    pub build_seconds: f32,
}

impl Blueprint {
    pub fn needs_wood(&self) -> i32 {
        (self.required_wood - self.delivered_wood).max(0)
    }

    pub fn has_materials(&self) -> bool {
        self.needs_wood() == 0
    }

    pub fn is_complete(&self) -> bool {
        self.has_materials() && self.progress >= self.build_seconds
    }

    pub fn progress_ratio(&self) -> f32 {
        if self.build_seconds <= 0.0 {
            1.0
        } else {
            (self.progress / self.build_seconds).clamp(0.0, 1.0)
        }
    }

    pub fn status(&self) -> BlueprintStatus {
        if !self.has_materials() {
            BlueprintStatus::WaitingForMaterials
        } else if self.is_complete() {
            BlueprintStatus::Complete
        } else if self.progress > 0.0 {
            BlueprintStatus::Building
        } else {
            BlueprintStatus::WaitingForBuilder
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlueprintStatus {
    WaitingForMaterials,
    WaitingForBuilder,
    Building,
    Complete,
}

impl BlueprintStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingForMaterials => "Waiting for materials",
            Self::WaitingForBuilder => "Waiting for builder",
            Self::Building => "Building",
            Self::Complete => "Complete",
        }
    }
}

#[derive(Component, Debug)]
pub struct CompletedBuilding {
    pub kind: BuildingKind,
}

#[derive(Component, Debug, Default)]
pub struct Housing {
    pub residents: Vec<Entity>,
}

impl Housing {
    pub const CAPACITY: usize = 5;

    pub fn resident_count(&self) -> usize {
        self.residents.len()
    }

    pub fn has_space(&self) -> bool {
        self.resident_count() < Self::CAPACITY
    }

    pub fn add_resident(&mut self, resident: Entity) -> bool {
        if self.residents.contains(&resident) || !self.has_space() {
            return false;
        }

        self.residents.push(resident);
        true
    }

    pub fn remove_resident(&mut self, resident: Entity) {
        self.residents.retain(|entity| *entity != resident);
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingEntrance {
    pub world_position: Vec3,
    pub local_offset: Vec3,
}

#[derive(Component, Debug)]
pub struct EntranceMarker {
    pub owner: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blueprint_waits_for_materials_before_completion() {
        let mut blueprint = Blueprint {
            kind: BuildingKind::House,
            required_wood: 4,
            delivered_wood: 3,
            progress: 99.0,
            build_seconds: 5.0,
        };

        assert!(!blueprint.is_complete());
        blueprint.delivered_wood = 4;
        assert!(blueprint.is_complete());
    }

    #[test]
    fn blueprint_status_tracks_materials_and_work() {
        let mut blueprint = Blueprint {
            kind: BuildingKind::House,
            required_wood: 4,
            delivered_wood: 0,
            progress: 0.0,
            build_seconds: 5.0,
        };

        assert_eq!(blueprint.status(), BlueprintStatus::WaitingForMaterials);
        blueprint.delivered_wood = 4;
        assert_eq!(blueprint.status(), BlueprintStatus::WaitingForBuilder);
        blueprint.progress = 2.0;
        assert_eq!(blueprint.status(), BlueprintStatus::Building);
        blueprint.progress = 5.0;
        assert_eq!(blueprint.status(), BlueprintStatus::Complete);
    }

    #[test]
    fn housing_capacity_is_five_residents() {
        let mut housing = Housing::default();

        for index in 0..Housing::CAPACITY {
            assert!(housing.add_resident(Entity::from_raw_u32(index as u32).unwrap()));
        }

        assert!(!housing.has_space());
        assert!(!housing.add_resident(Entity::from_raw_u32(99).unwrap()));
        assert_eq!(housing.resident_count(), 5);
    }
}
