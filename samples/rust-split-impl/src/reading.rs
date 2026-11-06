use crate::store::Store;

impl Store {
    pub fn label(&self) -> &str {
        &self.name
    }
}
