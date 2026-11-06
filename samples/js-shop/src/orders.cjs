const { DiscountedItem } = require("./billing.mjs");
const { Item } = require("./catalog.js");

function total(items) {
  let sum = 0;
  for (const item of items) {
    if (item instanceof DiscountedItem || item instanceof Item) {
      sum += item.price();
    }
  }
  return sum;
}

module.exports = { total };
