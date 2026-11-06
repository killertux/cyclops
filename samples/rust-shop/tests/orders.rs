use shop::orders::Order;

#[test]
fn starts_empty() {
    let order = Order { invoices: Vec::new() };
    assert_eq!(order.total(), 0);
}
