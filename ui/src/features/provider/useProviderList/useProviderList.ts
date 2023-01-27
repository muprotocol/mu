import { useEffect, useState } from "react";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import { getMarketplaceProgram } from "@/features/marketplace/getMarketplaceProgram/getMarketplaceProgram";

export default function useProviderList() {
  const { anchorProvider } = useAnchorProvider();
  const marketplaceProgram = getMarketplaceProgram(anchorProvider);
  const [providers, setProviders] = useState<any>([]);

  useEffect(() => {
    console.log(marketplaceProgram.account.provider)
    marketplaceProgram.account.provider
      .all()
      .then((res) => {
        console.log(res);
        setProviders(res);
      })
  }, [anchorProvider.connection]);

  return {
    providers,
  };
}
