import {ReactNode} from "react";
import {ConnectionProvider, WalletProvider} from "@solana/wallet-adapter-react";
import {WalletModalProvider} from "@solana/wallet-adapter-react-ui";
import '@solana/wallet-adapter-react-ui/styles.css'
import useWallets from "@/features/wallet/useWallet/useWallets";
import endpoint from "@/features/endpoint/endpoint";

export type WalletWrapperProps = {
    children: ReactNode
}

export default function WalletWrapper({children}: WalletWrapperProps) {
    const wallets = useWallets();

    return (
        <ConnectionProvider endpoint={endpoint()}>
            <WalletProvider wallets={wallets} autoConnect>
                <WalletModalProvider>
                    {children}
                </WalletModalProvider>
            </WalletProvider>
        </ConnectionProvider>
    )
}