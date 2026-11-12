import { Item } from "./catalog";

export class Invoice {
  constructor(
    public item: Item,
    public discount: number,
  ) {}

  total(): number {
    const price = this.item.price();
    return this.discount > price ? 0 : price - this.discount;
  }

  isFree(): boolean {
    return this.total() === 0;
  }
}
