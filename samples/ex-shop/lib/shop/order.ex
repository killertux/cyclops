defmodule Shop.Order do
  alias Shop.{Invoice, Item}

  defstruct invoices: []

  def new, do: %__MODULE__{}

  def add(%__MODULE__{} = order, %Item{} = item) do
    invoice = item |> Invoice.new(0)
    %{order | invoices: [invoice | order.invoices]}
  end

  def total(%__MODULE__{invoices: invoices}) do
    Enum.reduce(invoices, 0, fn invoice, sum ->
      if Invoice.free?(invoice), do: sum, else: sum + Invoice.total(invoice)
    end)
  end
end
