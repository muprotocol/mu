import { AnchorProvider, BN } from "@project-serum/anchor";
import { createAuthorizedUsageSigner, loadProviderFromStaticKeypair, createRegion, getMu, readMintFromStaticKeypair, ServiceRates } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let regionNum = parseInt(process.argv[2]);
    if (Number.isNaN(regionNum))
        regionNum = 1;

    let anchorProvider = AnchorProvider.local();
    let mint = readMintFromStaticKeypair();
    let mu = getMu(anchorProvider, mint);

    let provider = await loadProviderFromStaticKeypair(mu, "IB");

    console.log("Creating region and usage signer");
    let serviceRates: ServiceRates = {
        functionMbTeraInstructions: new BN(300000),
        gigabytesGatewayTraffic: new BN(10000000),
        millionGatewayRequests: new BN(50),
        dbGigabyteMonths: new BN(10000000),
        millionDbReads: new BN(500),
        millionDbWrites: new BN(2000),
    };
    let region = await createRegion(mu, provider, `MiddleEarth-${regionNum}`, regionNum, serviceRates, new BN(50_000_000), "http://localhost:12012/");
    console.log(`Region pubkey: ${region.pda.toBase58()}`);
});

