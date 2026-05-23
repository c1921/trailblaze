use bevy::prelude::*;

pub const CELL_SIZE: f32 = 1.0;
pub const MAP_HALF_CELLS: i32 = 24;
pub const ROAD_COST: f32 = 0.5;
pub const GROUND_COST: f32 = 1.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ResourceKind {
    Wood,
    Food,
}

impl ResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wood => "Wood",
            Self::Food => "Food",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum BuildingKind {
    House,
    Storage,
    Woodcutter,
    Gatherer,
    Road,
}

pub const BUILDING_KINDS: [BuildingKind; 5] = [
    BuildingKind::House,
    BuildingKind::Storage,
    BuildingKind::Woodcutter,
    BuildingKind::Gatherer,
    BuildingKind::Road,
];

#[derive(Clone, Copy, Debug)]
pub struct BuildingDefinition {
    pub label: &'static str,
    pub size: IVec2,
    pub wood_cost: i32,
    pub build_seconds: f32,
    pub height: f32,
    pub population_capacity: i32,
}

impl BuildingKind {
    pub fn definition(self) -> BuildingDefinition {
        match self {
            Self::House => BuildingDefinition {
                label: "House",
                size: IVec2::new(2, 2),
                wood_cost: 10,
                build_seconds: 5.0,
                height: 1.1,
                population_capacity: 4,
            },
            Self::Storage => BuildingDefinition {
                label: "Storage",
                size: IVec2::new(3, 2),
                wood_cost: 12,
                build_seconds: 6.0,
                height: 0.9,
                population_capacity: 0,
            },
            Self::Woodcutter => BuildingDefinition {
                label: "Woodcutter",
                size: IVec2::new(2, 2),
                wood_cost: 8,
                build_seconds: 4.0,
                height: 1.0,
                population_capacity: 0,
            },
            Self::Gatherer => BuildingDefinition {
                label: "Gatherer",
                size: IVec2::new(2, 2),
                wood_cost: 8,
                build_seconds: 4.0,
                height: 1.0,
                population_capacity: 0,
            },
            Self::Road => BuildingDefinition {
                label: "Road",
                size: IVec2::new(1, 1),
                wood_cost: 1,
                build_seconds: 0.6,
                height: 0.05,
                population_capacity: 0,
            },
        }
    }

    pub fn hotkey(self) -> KeyCode {
        match self {
            Self::House => KeyCode::Digit1,
            Self::Storage => KeyCode::Digit2,
            Self::Woodcutter => KeyCode::Digit3,
            Self::Gatherer => KeyCode::Digit4,
            Self::Road => KeyCode::Digit5,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::House => "Provides housing capacity for settlers.",
            Self::Storage => "Receives gathered supplies and construction materials.",
            Self::Woodcutter => "Unlocks automatic wood gathering from tree resource nodes.",
            Self::Gatherer => "Unlocks automatic food gathering from forage resource nodes.",
            Self::Road => "Marks planned paths through the settlement.",
        }
    }

    pub fn entrance_direction(self) -> Option<IVec2> {
        match self {
            Self::Road => None,
            _ => Some(IVec2::NEG_Y),
        }
    }
}

pub fn building_color(kind: BuildingKind) -> Color {
    match kind {
        BuildingKind::House => Color::srgb(0.74, 0.38, 0.24),
        BuildingKind::Storage => Color::srgb(0.72, 0.58, 0.34),
        BuildingKind::Woodcutter => Color::srgb(0.35, 0.55, 0.23),
        BuildingKind::Gatherer => Color::srgb(0.45, 0.38, 0.68),
        BuildingKind::Road => Color::srgb(0.22, 0.2, 0.18),
    }
}

pub fn world_to_cell(position: Vec3) -> IVec2 {
    IVec2::new(
        (position.x / CELL_SIZE).round() as i32,
        (position.z / CELL_SIZE).round() as i32,
    )
}

pub fn cell_to_world(cell: IVec2) -> Vec3 {
    Vec3::new(cell.x as f32 * CELL_SIZE, 0.0, cell.y as f32 * CELL_SIZE)
}

pub fn snap_to_grid(position: Vec3) -> Vec3 {
    cell_to_world(world_to_cell(position))
}

pub fn rotated_size(size: IVec2, rotation_steps: i32) -> IVec2 {
    if rotation_steps.rem_euclid(2) == 0 {
        size
    } else {
        IVec2::new(size.y, size.x)
    }
}

pub fn footprint_cells(center: IVec2, size: IVec2) -> Vec<IVec2> {
    let start = center - IVec2::new((size.x - 1) / 2, (size.y - 1) / 2);
    let mut cells = Vec::with_capacity((size.x * size.y) as usize);

    for x in 0..size.x {
        for y in 0..size.y {
            cells.push(start + IVec2::new(x, y));
        }
    }

    cells
}

pub fn rotated_direction(direction: IVec2, rotation_steps: i32) -> IVec2 {
    match rotation_steps.rem_euclid(4) {
        0 => direction,
        1 => IVec2::new(-direction.y, direction.x),
        2 => -direction,
        _ => IVec2::new(direction.y, -direction.x),
    }
}

pub fn entrance_cell(center: IVec2, size: IVec2, rotation_steps: i32, direction: IVec2) -> IVec2 {
    let size = rotated_size(size, rotation_steps);
    let direction = rotated_direction(direction, rotation_steps);
    let cells = footprint_cells(center, size);
    let min_x = cells.iter().map(|cell| cell.x).min().unwrap_or(center.x);
    let max_x = cells.iter().map(|cell| cell.x).max().unwrap_or(center.x);
    let min_y = cells.iter().map(|cell| cell.y).min().unwrap_or(center.y);
    let max_y = cells.iter().map(|cell| cell.y).max().unwrap_or(center.y);

    match (direction.x.signum(), direction.y.signum()) {
        (-1, _) => IVec2::new(min_x - 1, center.y),
        (1, _) => IVec2::new(max_x + 1, center.y),
        (_, -1) => IVec2::new(center.x, min_y - 1),
        (_, 1) => IVec2::new(center.x, max_y + 1),
        _ => center,
    }
}

pub fn within_map(cell: IVec2) -> bool {
    cell.x >= -MAP_HALF_CELLS
        && cell.x <= MAP_HALF_CELLS
        && cell.y >= -MAP_HALF_CELLS
        && cell.y <= MAP_HALF_CELLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_matches_building_area() {
        let cells = footprint_cells(IVec2::ZERO, IVec2::new(3, 2));
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&IVec2::new(-1, 0)));
        assert!(cells.contains(&IVec2::new(1, 1)));
    }

    #[test]
    fn rotation_swaps_rectangular_size() {
        assert_eq!(rotated_size(IVec2::new(3, 2), 1), IVec2::new(2, 3));
        assert_eq!(rotated_size(IVec2::new(3, 2), 2), IVec2::new(3, 2));
    }

    #[test]
    fn entrance_cell_tracks_rotation() {
        let center = IVec2::ZERO;
        let size = IVec2::new(3, 2);
        let direction = IVec2::NEG_Y;

        assert_eq!(entrance_cell(center, size, 0, direction), IVec2::new(0, -1));
        assert_eq!(entrance_cell(center, size, 1, direction), IVec2::new(2, 0));
        assert_eq!(entrance_cell(center, size, 2, direction), IVec2::new(0, 2));
        assert_eq!(entrance_cell(center, size, 3, direction), IVec2::new(-1, 0));
    }
}
