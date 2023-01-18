import {ReactNode} from "react";
import {ConnectionProvider, WalletProvider} from "@solana/wallet-adapter-react";
import {WalletModalProvider} from "@solana/wallet-adapter-react-ui";
import useWallets from "@/src/components/wallet/useWallet/useWallets";
import endpoint from "@/src/constants/endpoint/endpoint";
import '@solana/wallet-adapter-react-ui/styles.css'

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