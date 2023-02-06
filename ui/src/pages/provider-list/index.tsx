import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";
import useAccount from "@/features/marketplace/useAccount/useAccount";

export default function ProviderList() {
    const {marketplace} = useMarketplace();
    const [providers, isLoading] = useAccount(marketplace.account.provider.all(), []);

    return (
        <div className="container mx-auto">
            <div className="flex gap-2">
                <h1>Providers</h1> {isLoading && <div>Loading...</div>}
            </div>

            <div className="flex flex-col gap-2">
                {providers.map((provider) => {
                    return (
                        <div
                            className="rounded bg-black p-5 text-white"
                            key={provider.publicKey.toString()}
                        >
                            {provider.account.name} {provider.publicKey.toString()}
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
