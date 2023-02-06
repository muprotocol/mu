import {useState} from "react";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";

import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";

import {MarketplaceAccount, MarketplaceAccountName,} from "../marketplace.type";

export default function useAllAccounts<T extends MarketplaceAccountName>(
    accountName: MarketplaceAccountName,
    onAccountsChanged: (account: MarketplaceAccount<T>[]) => any = () => {
    },
): [accounts: MarketplaceAccount<T>[], isLoading: boolean] {
    const {marketplace} = useMarketplace();
    const allAccounts = marketplace.account[accountName].all() as Promise<
        MarketplaceAccount<T>[]
    >;
    const [accounts, setAccounts] = useState<MarketplaceAccount<T>[]>([]);
    const [isLoading] = useWithLoading(
        allAccounts,
        (accounts) => {
            setAccounts(accounts);
            onAccountsChanged(accounts);
        },
        [marketplace.account[accountName]],
    );

    return [accounts, isLoading];
}

export function useAccount<T extends MarketplaceAccount<any>[] | MarketplaceAccount<any>>(
    getAccountMethod: Promise<T>,
    initialState: T,
    onGetAccountResolved: (account: T) => void = () => {}
): [T, boolean] {
    const [account, setAccount] = useState<T>(initialState);
    const [isLoading] = useWithLoading(
        getAccountMethod,
        (resolvedAccount) => {
            setAccount(resolvedAccount);
            onGetAccountResolved(resolvedAccount);
        },
        []
    );

    return [account, isLoading];
}
