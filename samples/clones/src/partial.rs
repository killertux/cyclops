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

pub fn describe(names: &[String]) -> String {
    let mut text = String::new();
    while text.len() < 80 {
        match names.first() {
            Some(name) => text.push_str(name),
            None => break,
        }
    }
    text.make_ascii_uppercase();
    text.push('!');
    text
}
