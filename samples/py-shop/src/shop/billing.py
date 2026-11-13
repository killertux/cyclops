from dataclasses import dataclass

from shop.catalog import Item


@dataclass
class Invoice:
    item: Item
    discount: int

    def total(self) -> int:
        price = self.item.price()
        return 0 if self.discount > price else price - self.discount

    def is_free(self) -> bool:
        return self.total() == 0


class DiscountedItem(Item):
    pass
