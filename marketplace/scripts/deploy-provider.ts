import { AnchorProvider } from "@project-serum/anchor";
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
        billionFunctionMbInstructions: 300,
        dbGigabyteMonths: 1000,
        gigabytesGatewayTraffic: 100,
        millionDbReads: 500,
        millionDbWrites: 2000,
        millionGatewayRequests: 50
    };
    let region = await createRegion(mu, provider, "MiddleEarth", 1, serviceRates, 1);
    console.log(`Region pubkey: ${region.pda.toBase58()}`);

    let usageSigner = await createAuthorizedUsageSigner(mu, provider, region, "usage_signer");
    console.log(`Usage signer pubkey: ${usageSigner.pda.toBase58()}`);
});
