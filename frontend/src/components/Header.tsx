import dynamic from "next/dynamic";

export default function Header() {
    const WalletMultiButtonDynamic = dynamic(
        async () => (await import('@solana/wallet-adapter-react-ui')).WalletMultiButton,
        { ssr: false }
    );


    return (
        <header className="container mx-auto p-5 flex justify-between">
            <div>logo</div>
            <div>
                <WalletMultiButtonDynamic />
            </div>
        </header>
    )
}