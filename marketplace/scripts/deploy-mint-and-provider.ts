import { AnchorProvider } from "@project-serum/anchor";
import { createAuthorizedUsageSigner, createMint, createProvider, createRegion, initializeMu, ServiceUnits } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
    let anchorProvider = AnchorProvider.local();

    console.log("Deploying mint");
    let mint = await createMint(anchorProvider, true);

    console.log("Initializing Mu smart contract");
    let mu = await initializeMu(anchorProvider, mint);

    console.log("Creating provider");
    let provider = await createProvider(mu, "IB", true);
    console.log(`Provider pubkey: ${provider.pda.toBase58()}`);

    console.log("Creating region and usage signer");
    let serviceRates: ServiceUnits = {
        mudb_gb_month: 100,
        mufunction_cpu_mem: 1000,
        bandwidth: 50,
        gateway_mreqs: 100
    };
    let region = await createRegion(mu, provider, "MiddleEarth", 1, serviceRates, 1);
    console.log(`Region pubkey: ${region.pda.toBase58()}`);

    let usageSigner = await createAuthorizedUsageSigner(mu, provider, region, "usage_signer");
    console.log(`Usage signer pubkey: ${usageSigner.pda.toBase58()}`);
});
