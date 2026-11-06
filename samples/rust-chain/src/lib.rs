pub struct Engine;

impl Engine {
    pub fn start(&self) {}
}

pub struct Car {
    pub engine: Engine,
}

impl Car {
    pub fn drive(&self) {
        self.engine.start();
    }

    pub fn race(&self) {
        self.drive();
    }
}
