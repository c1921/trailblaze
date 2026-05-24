use bevy::prelude::*;

use crate::types::ResourceKind;

pub const CENTRAL_STORAGE_CAPACITY: i32 = 1_000;
pub const STORAGE_CAPACITY: i32 = 4_000;
pub const HOUSE_FOOD_CAPACITY: i32 = 20;
pub const HOME_FOOD_PER_RESIDENT: i32 = 4;
pub const COLONIST_CARRY_CAPACITY: i32 = 10;

#[derive(Component, Debug, Clone)]
pub struct Inventory {
    pub capacity: i32,
    pub wood: i32,
    pub food: i32,
    accepts_wood: bool,
    accepts_food: bool,
}

impl Inventory {
    pub fn public(capacity: i32) -> Self {
        Self {
            capacity,
            wood: 0,
            food: 0,
            accepts_wood: true,
            accepts_food: true,
        }
    }

    pub fn home_food(capacity: i32) -> Self {
        Self {
            capacity,
            wood: 0,
            food: 0,
            accepts_wood: false,
            accepts_food: true,
        }
    }

    pub fn amount(&self, kind: ResourceKind) -> i32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Food => self.food,
        }
    }

    pub fn accepts(&self, kind: ResourceKind) -> bool {
        match kind {
            ResourceKind::Wood => self.accepts_wood,
            ResourceKind::Food => self.accepts_food,
        }
    }

    pub fn used_capacity(&self) -> i32 {
        self.wood * ResourceKind::Wood.unit_size() + self.food * ResourceKind::Food.unit_size()
    }

    pub fn remaining_capacity(&self) -> i32 {
        (self.capacity - self.used_capacity()).max(0)
    }

    pub fn max_addable(&self, kind: ResourceKind) -> i32 {
        if !self.accepts(kind) {
            return 0;
        }

        self.remaining_capacity() / kind.unit_size()
    }

    pub fn add_partial(&mut self, kind: ResourceKind, amount: i32) -> i32 {
        let accepted = amount.max(0).min(self.max_addable(kind));
        match kind {
            ResourceKind::Wood => self.wood += accepted,
            ResourceKind::Food => self.food += accepted,
        }
        accepted
    }

    pub fn add(&mut self, kind: ResourceKind, amount: i32) -> bool {
        if self.max_addable(kind) < amount {
            return false;
        }

        self.add_partial(kind, amount);
        true
    }

    pub fn remove(&mut self, kind: ResourceKind, amount: i32) -> bool {
        if self.amount(kind) < amount {
            return false;
        }

        match kind {
            ResourceKind::Wood => self.wood -= amount,
            ResourceKind::Food => self.food -= amount,
        }

        true
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PublicInventory;

#[derive(Component, Debug, Clone, Copy)]
pub struct CentralStorage;

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub struct ResourceStock {
    pub wood: i32,
    pub food: i32,
}

impl ResourceStock {
    pub fn amount(&self, kind: ResourceKind) -> i32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Food => self.food,
        }
    }
}

pub fn public_stock<'a>(inventories: impl IntoIterator<Item = &'a Inventory>) -> ResourceStock {
    let mut stock = ResourceStock::default();
    for inventory in inventories {
        stock.wood += inventory.wood;
        stock.food += inventory.food;
    }
    stock
}

pub fn carried_amount(kind: ResourceKind, carry_capacity: i32) -> i32 {
    (carry_capacity / kind.unit_size()).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_capacity_uses_resource_sizes() {
        let mut inventory = Inventory::public(20);

        assert!(inventory.add(ResourceKind::Wood, 2));
        assert_eq!(inventory.used_capacity(), 20);
        assert_eq!(inventory.max_addable(ResourceKind::Food), 0);
        assert!(!inventory.add(ResourceKind::Food, 1));
    }

    #[test]
    fn food_can_fill_remaining_capacity() {
        let mut inventory = Inventory::public(12);

        assert!(inventory.add(ResourceKind::Wood, 1));
        assert_eq!(inventory.add_partial(ResourceKind::Food, 5), 2);
        assert_eq!(inventory.food, 2);
        assert_eq!(inventory.used_capacity(), 12);
    }

    #[test]
    fn home_inventory_rejects_wood() {
        let mut inventory = Inventory::home_food(HOUSE_FOOD_CAPACITY);

        assert_eq!(inventory.max_addable(ResourceKind::Wood), 0);
        assert!(!inventory.add(ResourceKind::Wood, 1));
        assert!(inventory.add(ResourceKind::Food, 20));
    }

    #[test]
    fn remove_is_atomic() {
        let mut inventory = Inventory::public(20);
        inventory.add(ResourceKind::Wood, 1);

        assert!(!inventory.remove(ResourceKind::Wood, 2));
        assert_eq!(inventory.wood, 1);
        assert!(inventory.remove(ResourceKind::Wood, 1));
        assert_eq!(inventory.wood, 0);
    }

    #[test]
    fn public_stock_only_sums_given_public_inventories() {
        let mut public = Inventory::public(100);
        let mut home = Inventory::home_food(20);
        public.add(ResourceKind::Food, 5);
        home.add(ResourceKind::Food, 20);

        let stock = public_stock([&public]);

        assert_eq!(stock.food, 5);
        assert_eq!(stock.amount(ResourceKind::Food), 5);
    }
}
