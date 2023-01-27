import { useEffect, useState } from "react";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import { getMarketplaceProgram } from "@/features/marketplace/getMarketplaceProgram/getMarketplaceProgram";
import {IdlAccounts} from "@project-serum/anchor/dist/cjs/program/namespace/types";
import {Marketplace} from "@mu/marketplace/target/types/marketplace";
import {ProgramAccount} from "@project-serum/anchor/dist/cjs/program/namespace/account";

export default function useProviderList() {
  const { anchorProvider } = useAnchorProvider();
  const marketplaceProgram = getMarketplaceProgram(anchorProvider);
  const [providers, setProviders] = useState<ProgramAccount<IdlAccounts<Marketplace>["provider"]>[]>([]);

  useEffect(() => {
    marketplaceProgram.account.provider
      .all()
      .then((res) => {
        console.log(res);
        setProviders(res);
      })
  }, [anchorProvider.connection, marketplaceProgram.idl]);

  return {
    providers,
  };
}
