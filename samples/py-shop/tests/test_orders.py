from shop.orders import Order


def test_starts_empty() -> None:
    assert Order().total() == 0
