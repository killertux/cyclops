import { Item } from "./catalog.js";

export class DiscountedItem extends Item {
  constructor(cost, discount) {
    super(cost);
    this.discount = discount;
  }

  price() {
    return this.discount > this.cost ? 0 : this.cost - this.discount;
  }
}
