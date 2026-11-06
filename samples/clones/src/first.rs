pub fn total(prices: &[u32], discount: u32) -> u32 {
    let mut sum = 0;
    for price in prices {
        if *price > discount {
            sum += *price - discount;
        } else {
            sum += 1;
        }
    }
    let shipping = if sum > 100 { 0 } else { 10 };
    sum + shipping
}
