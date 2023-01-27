import { AnchorProvider, Wallet } from "@project-serum/anchor";
import { Connection } from "@solana/web3.js";
import { describe, expect, test } from "vitest";

import endpoint from "@/features/endpoint/endpoint";
import { getMarketplaceProgram } from "@/features/marketplace/getMarketplaceProgram/getMarketplaceProgram";

describe("getMarketplaceProgram", () => {
  test("it should return #Program<Marketplace>", () => {
    const marketplaceProgram = getMarketplaceProgram(AnchorProvider.env());
    console.log(marketplaceProgram);

    expect(marketplaceProgram).toBeDefined();
  });
});
