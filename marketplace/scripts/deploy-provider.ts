import { AnchorProvider, BN } from "@project-serum/anchor";
import { authorizeProvider, createAuthorizedUsageSigner, createProvider, createRegion, getMu, readMintFromStaticKeypair, readProviderAuthorizer, ServiceRates } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);

    console.log("Creating provider");
    let provider = await createProvider(mu, "IB", true);
    console.log(`Provider pubkey: ${provider.pda.toBase58()}`);

    console.log("Authorizing provider");
    let authorizer = readProviderAuthorizer(mu, "1");
    await authorizeProvider(mu, provider, authorizer);

    console.log("Creating region and usage signer");
    let serviceRates: ServiceRates = {
        functionMbTeraInstructions: new BN(300000),
        gigabytesGatewayTraffic: new BN(10000000),
        millionGatewayRequests: new BN(50),
        dbGigabyteMonths: new BN(10000000),
        millionDbReads: new BN(500),
        millionDbWrites: new BN(2000),
    };
    let region = await createRegion(mu, provider, "MiddleEarth", 1, serviceRates, new BN(50_000_000));
    console.log(`Region pubkey: ${region.pda.toBase58()}`);

    let usageSigner = await createAuthorizedUsageSigner(mu, provider, region, "usage_signer");
    console.log(`Usage signer pubkey: ${usageSigner.pda.toBase58()}`);
});
