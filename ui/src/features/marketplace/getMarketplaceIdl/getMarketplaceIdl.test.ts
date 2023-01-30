import muMarketplaceIdl from "@mu/marketplace/target/idl/marketplace.json";

import { AnchorProvider, Program } from "@project-serum/anchor";
import { describe, expect } from "vitest";

import { getMarketplaceIdl } from "@/features/marketplace/getMarketplaceIdl/getMarketplaceIdl";


describe("getMarketplaceIdl", () => {
  const marketplaceIdl = getMarketplaceIdl();
  test("it should return an object with type of an IDL", () => {
    expect(marketplaceIdl).toBeDefined();
    expect(marketplaceIdl).toBeTruthy();
    expect(marketplaceIdl).toStrictEqual(muMarketplaceIdl);
  });
});