import { useEffect, useState } from "react";

import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import { ProgramAccount } from "@project-serum/anchor/dist/cjs/program/namespace/account";
import {
  AllAccountsMap,
  IdlAccounts,
} from "@project-serum/anchor/dist/cjs/program/namespace/types";

import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";

export default function useGetMarketplaceAccount(
  providerName: keyof AllAccountsMap<Marketplace>,
) {
  const {marketplace} = useMarketplace();
  const [providers, setProviders] = useState<
    ProgramAccount<IdlAccounts<Marketplace>["provider"]>[]
  >([]);

  useEffect(() => {
    marketplace.account.provider.all().then((res) => {
      console.log(res);
      setProviders(res);
    });
  }, [marketplace.account.provider]);

  return { providers };
}