import { useEffect, useState } from "react";

import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import { ProgramAccount } from "@project-serum/anchor/dist/cjs/program/namespace/account";
import {
  AllAccountsMap,
  IdlAccounts,
} from "@project-serum/anchor/dist/cjs/program/namespace/types";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import { getMarketplaceProgram } from "@/features/marketplace/getMarketplaceProgram/getMarketplaceProgram";

export default function useGetMarketplaceAccount(
  providerName: keyof AllAccountsMap<Marketplace>,
) {
  const { anchorProvider } = useAnchorProvider();
  const marketplaceProgram = getMarketplaceProgram(anchorProvider);
  const [providers, setProviders] = useState<
    ProgramAccount<IdlAccounts<Marketplace>["provider"]>[]
  >([]);

  useEffect(() => {
    marketplaceProgram.account.provider.all().then((res) => {
      console.log(res);
      setProviders(res);
    });
  }, [anchorProvider.connection, marketplaceProgram.idl]);

  return { providers };
}
