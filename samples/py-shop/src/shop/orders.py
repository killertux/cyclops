from shop.billing import Invoice

from .catalog import Item


class Order:
    def __init__(self) -> None:
        self.invoices: list[Invoice] = []

    def add(self, item: Item) -> None:
        self.invoices.append(Invoice(item, 0))

    def total(self) -> int:
        total = 0
        for invoice in self.invoices:
            if not invoice.is_free():
                total += invoice.total()
        return total
