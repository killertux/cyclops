export function total(prices: number[], discount: number): number {
  let sum = 0;
  for (const price of prices) {
    if (price > discount) {
      sum += price - discount;
    } else {
      sum += 1;
    }
  }
  const shipping = sum > 100 ? 0 : 10;
  return sum + shipping;
}
