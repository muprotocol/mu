import useAccount from "@/features/marketplace/useAccount/useAccount";
import {MarketplaceAccount} from "@/features/marketplace/marketplace.type";
import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";
import {GetServerSidePropsContext} from "next";
import {Chip} from "primereact/chip";
import {DataTable} from "primereact/datatable";
import {Column} from "primereact/column";
import {Tag} from "primereact/tag";
import {Divider} from "primereact/divider";
import {useRouter} from "next/router";

export async function getServerSideProps(context: GetServerSidePropsContext) {
    return {
        props: {params: context.params}
    };
}

type ProviderProps = {
    params: Record<string, string>
}

export default function Provider({params}: ProviderProps) {
    const {providerPublicKey} = params;
    const {marketplace} = useMarketplace();
    const [provider, isProviderLoading] = useAccount<MarketplaceAccount<"provider">["account"] | null>(
        marketplace.account.provider.fetch(providerPublicKey),
        null,
        [providerPublicKey]
    )

    const [providerRegions, isLoading] = useAccount<MarketplaceAccount<"providerRegion">[]>(
        marketplace.account.providerRegion.all([
            {
                memcmp: {
                    offset: 8 /* todo(HIGH): ask how to automate this from mu */,
                    bytes: providerPublicKey,
                },
            },
        ]),
        [],
        [providerPublicKey],
    )

    return (
        <div className="container mx-auto flex flex-col gap-10">
            <div className="flex items-start gap-5 !text-3xl font-bold">
                <i className="pi pi-database !text-3xl"></i>
                <h1>Provider {
                    provider ? (<><Chip className="bg-mu-primary" label={provider.name}/> <Chip
                        label={providerPublicKey}/></>) : (
                        <i className="pi pi-spin pi-spinner"></i>)
                }</h1>
            </div>

            <div className="p-card">
                <div className="!text-xl p-6"><i className="pi pi-cloud !text-xl"></i> Regions</div>
                <DataTable className="border-t" value={providerRegions} responsiveLayout="scroll" stripedRows
                           removableSort
                           loading={isLoading}>
                    <Column sortable field="account.name" header="Name"></Column>
                    <Column
                        header="Min Escrow Balance"
                        dataType="numeric"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                providerRegion.account.minEscrowBalance.toNumber().toLocaleString()
                        }></Column>
                    <Column
                        className="capitalize"
                        header="Function Instructions"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div className="flex gap-2 items-center">
                                    {providerRegion.account.rates.billionFunctionMbInstructions.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="billions"></Tag>
                                    <Tag className="capitalize" severity="info" value="MB"></Tag>
                                </div>
                        }
                    ></Column>
                    <Column
                        className="capitalize"
                        header="Gateway Traffic"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div className="flex gap-2 items-center">
                                    {providerRegion.account.rates.gigabytesGatewayTraffic.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="GB"></Tag>
                                </div>
                        }
                    ></Column>
                    <Column
                        className="capitalize"
                        header="gateway requests"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div className="flex gap-2 items-center">
                                    {providerRegion.account.rates.millionGatewayRequests.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="million"></Tag>
                                </div>
                        }
                    ></Column>
                    <Column
                        className="capitalize"
                        header="DB Months"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div className="flex gap-2 items-center">
                                    {providerRegion.account.rates.dbGigabyteMonths.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="GB"></Tag>
                                </div>
                        }
                    ></Column>
                    <Column
                        className="capitalize"
                        header="DB Reads"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div className="flex gap-2 items-center">
                                    {providerRegion.account.rates.millionDbReads.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="million"></Tag>
                                </div>
                        }
                    ></Column>
                    <Column
                        className="capitalize"
                        header="DB Writes"
                        body={
                            (providerRegion: MarketplaceAccount<"providerRegion">) =>
                                <div
                                    className="flex gap-2 items-center">
                                    {providerRegion.account.rates.millionDbWrites.toNumber().toLocaleString()}
                                    <Tag className="capitalize" severity="info" value="million"></Tag>
                                </div>
                        }
                    ></Column>
                </DataTable>

            </div>
        </div>
    )
}