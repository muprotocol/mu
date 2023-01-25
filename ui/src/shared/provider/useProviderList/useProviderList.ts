import { useEffect, useState } from "react";

import useAnchorProvider from "@/shared/anchor/useAnchorProvider/useAnchorProvider";
import { getMarketplaceProgram } from "@/shared/marketplace/getMarketplaceProgram/getMarketplaceProgram";

export default function useProviderList() {
  const { provider } = useAnchorProvider();
  const marketplaceProgram = getMarketplaceProgram(provider);
  const [providers, setProviders] = useState<any>([]);

  useEffect(() => {
    marketplaceProgram.account.provider
      .all()
      .then((res) => {
        console.log(res);
        setProviders(res);
      })
      .finally(() => {});
  }, [provider.connection]);

  return {
    providers,
  };
}
