use crate::catalog::{Item, Priced};

pub struct Invoice {
    pub item: Item,
    pub discount: u32,
}

impl Invoice {
    pub fn total(&self) -> u32 {
        let price = self.item.price();
        if self.discount > price {
            0
        } else {
            price - self.discount
        }
    }

    pub fn is_free(&self) -> bool {
        self.total() == 0
    }
}
