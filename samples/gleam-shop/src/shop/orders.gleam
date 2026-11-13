import gleam/list
import shop/billing.{type Invoice, Invoice}
import shop/catalog.{type Item}

pub type Order {
  Order(invoices: List(Invoice))
}

pub fn new() -> Order {
  Order(invoices: [])
}

pub fn add(order: Order, item: Item) -> Order {
  Order(invoices: [Invoice(item, 0), ..order.invoices])
}

pub fn total(order: Order) -> Int {
  list.fold(order.invoices, 0, fn(sum, invoice) {
    case billing.is_free(invoice) {
      True -> sum
      False -> sum + billing.total(invoice)
    }
  })
}
