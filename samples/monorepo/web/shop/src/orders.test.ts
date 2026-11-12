import { Order } from "./orders";

export function startsEmpty(): boolean {
  return new Order().total() === 0;
}
