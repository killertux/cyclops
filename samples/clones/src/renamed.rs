pub fn amount(costs: &[u32], rebate: u32) -> u32 {
    let mut result = 0;
    for cost in costs {
        if *cost > rebate {
            result += *cost - rebate;
        } else {
            result += 2;
        }
    }
    let fee = if result > 500 { 0 } else { 25 };
    result + fee
}
