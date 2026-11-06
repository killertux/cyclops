export function amount(costs: number[], rebate: number): number {
  let result = 0;
  for (const cost of costs) {
    if (cost > rebate) {
      result += cost - rebate;
    } else {
      result += 1;
    }
  }
  const fee = result > 100 ? 0 : 10;
  return result + fee;
}
