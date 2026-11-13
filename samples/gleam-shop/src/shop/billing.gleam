import shop/catalog.{type Item}

pub type Invoice {
  Invoice(item: Item, discount: Int)
}

pub fn total(invoice: Invoice) -> Int {
  let price = catalog.price(invoice.item)
  case invoice.discount > price {
    True -> 0
    False -> price - invoice.discount
  }
}

pub fn is_free(invoice: Invoice) -> Bool {
  total(invoice) == 0
}
