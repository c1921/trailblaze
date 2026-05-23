use bevy::prelude::*;

use crate::types::ResourceKind;

#[derive(Resource, Debug, Clone)]
pub struct ResourceStock {
    pub wood: i32,
    pub food: i32,
}

impl Default for ResourceStock {
    fn default() -> Self {
        Self { wood: 40, food: 20 }
    }
}

impl ResourceStock {
    pub fn amount(&self, kind: ResourceKind) -> i32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Food => self.food,
        }
    }

    pub fn add(&mut self, kind: ResourceKind, amount: i32) {
        match kind {
            ResourceKind::Wood => self.wood += amount,
            ResourceKind::Food => self.food += amount,
        }
    }

    pub fn remove(&mut self, kind: ResourceKind, amount: i32) -> bool {
        let current = self.amount(kind);
        if current < amount {
            return false;
        }

        match kind {
            ResourceKind::Wood => self.wood -= amount,
            ResourceKind::Food => self.food -= amount,
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_remove_is_atomic() {
        let mut stock = ResourceStock { wood: 3, food: 0 };

        assert!(!stock.remove(ResourceKind::Wood, 4));
        assert_eq!(stock.wood, 3);

        assert!(stock.remove(ResourceKind::Wood, 2));
        assert_eq!(stock.wood, 1);
    }
}
