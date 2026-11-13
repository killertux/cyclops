defmodule Shop.Invoice do
  alias Shop.Item

  defstruct [:item, :discount]

  def new(%Item{} = item, discount), do: %__MODULE__{item: item, discount: discount}

  def total(%__MODULE__{item: item, discount: discount}) do
    price = Shop.Priced.price(item)
    if discount > price, do: 0, else: price - discount
  end

  def free?(invoice), do: total(invoice) == 0
end
