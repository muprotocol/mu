import { useMemo } from "react";

import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import { Program } from "@project-serum/anchor";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import { getMarketplaceIdl } from "@/features/marketplace/getMarketplaceIdl/getMarketplaceIdl";

export default function useMarketplace() {
  const { anchorProvider } = useAnchorProvider();
  const marketplaceIdl = getMarketplaceIdl();

  const marketplace = useMemo(
    () =>
      new Program<Marketplace>(
        marketplaceIdl,
        marketplaceIdl.metadata.address,
        anchorProvider,
      ),
    [anchorProvider.connection, marketplaceIdl],
  );

  return { marketplace: marketplace };
}
