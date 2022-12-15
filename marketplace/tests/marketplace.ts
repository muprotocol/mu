import {Keypair, PublicKey} from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import {expect} from "chai";
import {
    createAuthorizedUsageSigner,
    createEscrowAccount,
    createMint,
    createProvider,
    createRegion,
    deployStack,
    initializeMu,
    mintToAccount,
    MuAuthorizedSignerInfo,
    MuEscrowAccountInfo,
    MuProgram,
    MuProviderInfo,
    MuRegionInfo,
    MuStackInfo,
    readOrCreateUserWallet,
    ServiceRates, ServiceUsage,
    updateStackUsage
} from "../scripts/anchor-utils";
import {AnchorProvider, BN} from '@project-serum/anchor';

describe("marketplace", () => {
    let mu: MuProgram;

    let provider: MuProviderInfo;
    let region: MuRegionInfo;
    let authSigner: MuAuthorizedSignerInfo;

    let userWallet: Keypair;
    let escrow: MuEscrowAccountInfo;
    let stack: MuStackInfo;

    it("Initializes", async () => {
        let provider = AnchorProvider.env();
        let mint = await createMint(provider);
        mu = await initializeMu(provider, mint);
    });

    it("Creates a new provider", async () => {
        provider = await createProvider(mu, "Provider");

        const providerAccount = await spl.getAccount(mu.anchorProvider.connection, provider.tokenAccount);
        const depositAccount = await spl.getAccount(mu.anchorProvider.connection, mu.depositPda);
        expect(providerAccount.amount).to.equals(9900000000n);
        expect(depositAccount.amount).to.equals(100000000n);
    });

    it("Creates a region", async () => {
        const rates: ServiceRates = {
            billionFunctionMbInstructions: new BN(1), // TODO too cheap to be priced correctly, even with 6 decimal places
            dbGigabyteMonths: new BN(1000),
            gigabytesGatewayTraffic: new BN(100),
            millionDbReads: new BN(500),
            millionDbWrites: new BN(2000),
            millionGatewayRequests: new BN(50)
        };

        region = await createRegion(mu, provider, "Region", 1, rates, 3);
    });

    it("Creates an Authorized Usage Signer", async () => {
        authSigner = await createAuthorizedUsageSigner(mu, provider, region);
    });

    it("Creates an escrow account", async () => {
        userWallet = (await readOrCreateUserWallet(mu)).keypair;
        escrow = await createEscrowAccount(mu, userWallet, provider);

        const escrowAccount = await spl.getAccount(mu.anchorProvider.connection, escrow.pda);
        expect(escrowAccount.amount).to.equals(0n);
    });

    it("Creates a stack", async () => {
        stack = await deployStack(
            mu,
            userWallet,
            region,
            Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8]),
            100,
            "my stack"
        );
    });

    it("Updates usage on a stack", async () => {
        const usage: ServiceUsage = {
            functionMbInstructions: new BN(2000 * 1000000000 * 512),
            dbBytesSeconds: new BN(500 * 1024 * 1024 * 60 * 60 * 24 * 15),
            dbReads: new BN(5000000),
            dbWrites: new BN(800000),
            gatewayRequests: new BN(4000000),
            gatewayTrafficBytes: new BN(5 * 1024 * 1024 * 1024)
        };

        await mintToAccount(mu.anchorProvider, escrow.pda, mu.mint, 10_000_000);

        await updateStackUsage(mu, region, stack, authSigner, provider, escrow, 100, usage);

        const providerAccount = await spl.getAccount(
            mu.anchorProvider.connection, provider.tokenAccount
        );

        // 9900 $MU and 6 digits of decimal places left after paying deposit, 1029044 usage price
        expect(providerAccount.amount).to.equals(9900_000_000n + 1029044n);

        const escrowAccount = await spl.getAccount(
            mu.anchorProvider.connection,
            escrow.pda
        );
        expect(escrowAccount.amount).to.equals(8970956n); // 10_000_000 initial balance - 1029044 used
    });
});
