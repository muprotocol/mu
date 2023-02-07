import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";
import useAccount from "@/features/marketplace/useAccount/useAccount";

import {DataTable} from "primereact/datatable"
import {Column} from "primereact/column";
import {MarketplaceAccount} from "@/features/marketplace/marketplace.type";
import {Button} from "primereact/button";
import {useRouter} from "next/router";

export default function ProviderList() {
    const {push, asPath} = useRouter();
    const {marketplace} = useMarketplace();
    const [providers, isLoading] = useAccount<MarketplaceAccount<"provider">[]>(marketplace.account.provider.all(), []);

    const pushToProviderDetail = (provider: MarketplaceAccount<"provider">) => {
        const providerCopy = {...provider};
        push(`${asPath}/${providerCopy.publicKey.toString()}`);
    }

    const ActionColumn = (provider: MarketplaceAccount<"provider">) => {
        return (
            <div className="flex gap-5">
                <Button onClick={() => pushToProviderDetail(provider)} icon="pi pi-eye" label="Regions"></Button>
            </div>
        )
    }

    return (
        <div className="container mx-auto flex flex-col gap-10">
            <div className="flex items-baseline gap-5 !text-3xl font-bold">
                <i className="pi pi-database !text-3xl"></i>
                <h1>Providers</h1>
            </div>

            <div className="p-card">
                <DataTable value={providers} responsiveLayout="scroll" stripedRows removableSort loading={isLoading}>
                    <Column sortable field="account.name" header="Name"></Column>
                    <Column sortable body={(provider: MarketplaceAccount<"provider">) =>
                        <span>{provider.publicKey.toString()}</span>} header="PublicKey"></Column>
                    <Column body={ActionColumn} header="Actions"></Column>
                </DataTable>
            </div>
        </div>
    );
}
