import WalletButton from "@/components/wallet/WalletButton/WalletButton";

export default function Header() {
    return (
        <header className="container mx-auto p-5 flex! flex justify-between">
            <div>mu protocol logo</div>
            <div>
                <WalletButton/>
            </div>
        </header>
    )
}