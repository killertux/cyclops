use crate::store::Store;

impl Store {
    pub fn grow(&mut self) {
        self.size += 1;
    }
}
