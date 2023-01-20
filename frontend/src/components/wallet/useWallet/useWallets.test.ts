import {describe, expect, test} from "vitest";
import useWallets from "./useWallets"
import {renderHook} from "@testing-library/react"
import {Adapter} from "@solana/wallet-adapter-base";
import {PhantomWalletAdapter} from "@solana/wallet-adapter-phantom";
import includesType from "@/utils/includesType/includesType";


describe("useWallets", () => {
    test("it should include an instance of #PhantomWalletAdapter", () => {
        const {result} = renderHook(() => useWallets());
        const wallets: Adapter[] = result.current;

        expect(includesType(wallets, PhantomWalletAdapter)).toBeTruthy();
    })
})
