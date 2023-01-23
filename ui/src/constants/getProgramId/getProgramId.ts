import {PublicKey} from "@solana/web3.js";
import {getMarketplaceIdl} from "@/constants/getMarketplaceIdl/getMarketplaceIdl";

type getProgramIdProps = NonNullable<ReturnType<typeof getMarketplaceIdl>>;

export default function getProgramId(marketplaceIdl: getProgramIdProps): PublicKey {
    return new PublicKey(marketplaceIdl.metadata.address);
}