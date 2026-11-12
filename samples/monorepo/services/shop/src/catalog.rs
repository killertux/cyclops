pub trait Priced {
    fn price(&self) -> u32;
}

pub struct Item {
    pub cost: u32,
}

impl Priced for Item {
    fn price(&self) -> u32 {
        self.cost
    }
}
