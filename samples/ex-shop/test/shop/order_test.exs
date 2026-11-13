defmodule Shop.OrderTest do
  use ExUnit.Case

  alias Shop.Order

  test "starts empty" do
    assert Order.total(Order.new()) == 0
  end
end
