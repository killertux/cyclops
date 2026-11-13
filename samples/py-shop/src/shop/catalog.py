from abc import ABC, abstractmethod


class Priced(ABC):
    @abstractmethod
    def price(self) -> int: ...


class Item(Priced):
    def __init__(self, cost: int) -> None:
        self._cost = cost

    def price(self) -> int:
        return self._cost
