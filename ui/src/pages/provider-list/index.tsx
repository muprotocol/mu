import useAllAccounts, {useAccount} from "@/features/marketplace/useAllAccounts/useAllAccounts";
import useWithLoading from "@/hooks/useWithLoading/useWithLoading";
import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";
import {MarketplaceAccount} from "@/features/marketplace/marketplace.type";
import {useState} from "react";

export default function ProviderList() {
    const [providers, isLoading] = useAllAccounts<"provider">(
        "provider",
        console.log,
    );

    const {marketplace} = useMarketplace();
    const [providers1, isLoading1] = useAccount<MarketplaceAccount<"provider">[]>(marketplace.account.provider.all(), []);

    const [providers2, setProviders2] = useState<MarketplaceAccount<"provider">[]>([]);
    const [isLoading2] = useWithLoading(marketplace.account.provider.all(), setProviders2);

    return (
        <div className="container mx-auto">
            <div className="flex gap-2">
                <h1>Providers</h1> {isLoading && <div>Loading...</div>}
            </div>

            <div className="flex flex-col gap-2">
                {providers2.map((provider) => {
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
