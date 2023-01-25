import { useMemo } from "react";

import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import { AnchorProvider, Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";

import { getMarketplaceIdl } from "@/shared/marketplace/getMarketplaceIdl/getMarketplaceIdl";

type getProgramIdProps = NonNullable<ReturnType<typeof getMarketplaceIdl>>;

export function getMarketplaceProgram(
  provider: AnchorProvider,
): Program<Marketplace> {
  const marketplaceIdl = getMarketplaceIdl();
  const marketplaceProgram = new Program<Marketplace>(
    marketplaceIdl,
    marketplaceIdl.metadata.address,
    provider,
  );

  return marketplaceProgram;
}
