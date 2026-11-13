defmodule Shop.Item do
  defstruct [:cost]

  defimpl Shop.Priced do
    def price(item), do: item.cost
  end
end
