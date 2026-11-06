import { Invoice } from "./billing";
import { Item } from "./catalog";

export class Order {
  invoices: Invoice[] = [];

  add(item: Item): void {
    this.invoices.push(new Invoice(item, 0));
  }

  total(): number {
    let sum = 0;
    for (const invoice of this.invoices) {
      if (!invoice.isFree()) {
        sum += invoice.total();
      }
    }
    return sum;
  }
}
