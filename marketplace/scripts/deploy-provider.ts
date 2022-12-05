import {AnchorProvider, BN} from "@project-serum/anchor";
import { createAuthorizedUsageSigner, createProvider, createRegion, getMu, readMintFromStaticKeypair, ServiceRates } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);

    console.log("Creating provider");
    let provider = await createProvider(mu, "IB", true);
    console.log(`Provider pubkey: ${provider.pda.toBase58()}`);

    console.log("Creating region and usage signer");
    let serviceRates: ServiceRates = {
        billionFunctionMbInstructions: new BN(300000000000),
        dbGigabyteMonths: new BN(10000000000000),
        gigabytesGatewayTraffic: new BN(10000000000000),
        millionDbReads: new BN(500000000),
        millionDbWrites: new BN(2000000000),
        millionGatewayRequests: new BN(50000000)
    };
    let region = await createRegion(mu, provider, "MiddleEarth", 1, serviceRates, 1);
    console.log(`Region pubkey: ${region.pda.toBase58()}`);

    let usageSigner = await createAuthorizedUsageSigner(mu, provider, region, "usage_signer");
    console.log(`Usage signer pubkey: ${usageSigner.pda.toBase58()}`);
});
