import {useMemo} from "react";
import {PhantomWalletAdapter} from "@solana/wallet-adapter-phantom";
import {GlowWalletAdapter} from "@solana/wallet-adapter-glow";
import {SlopeWalletAdapter} from "@solana/wallet-adapter-slope";
import {TorusWalletAdapter} from "@solana/wallet-adapter-torus";
import {Adapter} from "@solana/wallet-adapter-base";

export default function useWallets(): Adapter[] {
    return useMemo(
        () => [
            new PhantomWalletAdapter(),
            new GlowWalletAdapter(),
            new SlopeWalletAdapter(),
            new TorusWalletAdapter()
        ], []);
}