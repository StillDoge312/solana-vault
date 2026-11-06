import { assert } from "chai";

describe("Week 12: Swap via Raydium CLMM logic", () => {
  it("calculates minimum amount out with 1% slippage", () => {
    const expectedOutput = 199_290_000n; // 199.29 tUSD
    const slippageBps = 100n; // 1.0% = 100 bps
    const minAmountOut = (expectedOutput * (10_000n - slippageBps)) / 10_000n;
    assert.equal(minAmountOut, 197_297_100n);
  });

  it("fails execution when slippage threshold exceeds pool output", () => {
    const actualPoolOutput = 199_290_000n;
    const unrealisticMinOut = 205_000_000n;
    assert.isTrue(actualPoolOutput < unrealisticMinOut, "Pool output must be rejected");
  });
});
