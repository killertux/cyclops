pub type Item {
  Item(cost: Int)
}

pub fn price(item: Item) -> Int {
  item.cost
}
