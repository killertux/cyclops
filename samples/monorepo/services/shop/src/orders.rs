use crate::billing::Invoice;
use crate::catalog::Item;

pub struct Order {
    pub invoices: Vec<Invoice>,
}

impl Order {
    pub fn add(&mut self, item: Item) {
        self.invoices.push(Invoice { item, discount: 0 });
    }

    pub fn total(&self) -> u32 {
        let mut sum = 0;
        for invoice in &self.invoices {
            if !invoice.is_free() {
                sum += invoice.total();
            }
        }
        sum
    }
}
