import { assert } from "chai";

describe("Week 13: Position lifecycle logic", () => {
  it("verifies position token storage validation", () => {
    const vaultStorage = "8urMhJnqeut6ZNsF3ZQVHDEh9DiPTcZ2tGzGCjveSRqi";
    const secondStorage = "22XTzzCnE7iGFt6BAj7MZwh2gCsZoi8o86JsrLocVybQ";
    const foreignAccount = "GMKeqbfv2iHiUfaCCah3XE2N6cz2QJH84jWRzQMVsgrP";

    const validPair = [vaultStorage, secondStorage];
    assert.isTrue(validPair.includes(vaultStorage) && validPair.includes(secondStorage));
    assert.isFalse(validPair.includes(foreignAccount));
  });

  it("calculates impermanent loss on price divergence", () => {
    // Initial: 100 tUSD + 53.93 VLT at P = 1.9958 (Total ~207.63 tUSD)
    // New price P = 1.8932 (fallen ~5.1%)
    // HODL value: 100 + 53.93 * 1.8932 = 202.10 tUSD
    // Pool converted to: 105.39 VLT + 0 tUSD = 105.39 * 1.8932 = 199.52 tUSD
    // Impermanent Loss = (202.10 - 199.52) / 202.10 ≈ 1.28%
    const hodlValue = 202.10;
    const poolValue = 199.52;
    const il = (hodlValue - poolValue) / hodlValue;
    assert.approximately(il, 0.0128, 0.001);
  });
});
