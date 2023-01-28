import { useEffect, useState } from "react";

import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import {
  AccountNamespace,
  IdlAccounts,
  ProgramAccount,
} from "@project-serum/anchor/dist/cjs/program/namespace";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";

import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";

import { MarketplaceAccount, MarketplaceAccountName } from "./marketplace.type";

export default function useAccounts<T extends MarketplaceAccountName>(
  accountName: MarketplaceAccountName,
  onAccountNameChanged: (account: MarketplaceAccount<T>[]) => any = () => {},
): [accounts: MarketplaceAccount<T>[], isLoading: boolean] {
  const { marketplace } = useMarketplace();
  const allAccounts = marketplace.account[accountName].all() as Promise<
    MarketplaceAccount<T>[]
  >;
  const [accounts, setAccounts] = useState<MarketplaceAccount<T>[]>([]);
  const [isLoading] = useWithLoading(
    allAccounts,
    (accounts) => {
      setAccounts(accounts);
      onAccountNameChanged(accounts);
    },
    [marketplace.account[accountName]],
  );

  return [accounts, isLoading];
}
