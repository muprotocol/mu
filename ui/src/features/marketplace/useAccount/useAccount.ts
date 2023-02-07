import {useState} from "react";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";

import {MarketplaceAccount,} from "../marketplace.type";

export default function useAccount<T extends MarketplaceAccount<any>[] | MarketplaceAccount<any>>(
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
