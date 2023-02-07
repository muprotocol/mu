import {useState} from "react";

import useWithLoading from "@/hooks/useWithLoading/useWithLoading";

import {MarketplaceAccount,} from "../marketplace.type";
import {IdlAccounts} from "@project-serum/anchor/dist/cjs/program/namespace";
import {Marketplace} from "@mu/marketplace/target/types/marketplace";

type useAccountProps<T> = {
    getAccountMethod: Promise<T>,
    initialState: T,
    dependencies?: any[],
    onGetAccountResolved?: (account: T) => void,
}

/*
*     {
        getAccountMethod,
        initialState,
        onGetAccountResolved = () => {},
        dependencies = []
    }: useAccountProps<T>*/

export default function useAccount<T extends MarketplaceAccount<any>[] | MarketplaceAccount<any> | any>(
    getAccountMethod: Promise<T>,
    initialState: T,
    dependencies: any[] = [],
    onGetAccountResolved: (account: T) => void = () => {
    }
): [T, boolean] {
    const [account, setAccount] = useState<T>(initialState);
    const [isLoading] = useWithLoading(
        getAccountMethod,
        (resolvedAccount) => {
            setAccount(resolvedAccount);
            onGetAccountResolved(resolvedAccount);
        },
        [...dependencies]
    );

    return [account, isLoading];
}
