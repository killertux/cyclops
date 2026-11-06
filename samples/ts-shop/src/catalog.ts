export interface Priced {
  price(): number;
}

export class Item implements Priced {
  constructor(private readonly cost: number) {}

  price(): number {
    return this.cost;
  }
}
