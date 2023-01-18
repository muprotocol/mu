import { Keypair, PublicKey } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import { expect } from "chai";
import {
    authorizeProvider,
    createAuthorizedUsageSigner,
    createEscrowAccount,
    createMint,
    createProvider,
    createProviderAuthorizer,
    createRegion,
    deployStack,
    initializeMu,
    mintToAccount,
    MuAuthorizedSignerInfo,
    MuEscrowAccountInfo,
    MuProgram,
    MuProviderAuthorizer,
    MuProviderInfo,
    MuRegionInfo,
    MuStackInfo,
    readOrCreateUserWallet,
    readOrCreateWallet,
    ServiceRates, ServiceUsage,
    updateStackUsage,
    withdrawEscrowBalance
} from "../scripts/anchor-utils";
import { AnchorError, AnchorProvider, BN } from '@project-serum/anchor';

describe("marketplace", () => {
    let mu: MuProgram;
    let providerAuthorizer: MuProviderAuthorizer;

    let provider: MuProviderInfo;
    let region: MuRegionInfo;
    let authSigner: MuAuthorizedSignerInfo;

    let userWallet: Keypair;
    let escrow: MuEscrowAccountInfo;
    let stack: MuStackInfo;

    let usagePrice = 1029044n;

    it("Initializes", async () => {
        let provider = AnchorProvider.env();
        let mint = await createMint(provider);
        mu = await initializeMu(provider, mint, 100_000);
    });

    it("Creates a provider authorizer", async () => {
        providerAuthorizer = await createProviderAuthorizer(mu);
    });

    it("Creates a new provider", async () => {
        provider = await createProvider(mu, "Provider");

        const providerAccount = await spl.getAccount(mu.anchorProvider.connection, provider.tokenAccount);
        const depositAccount = await spl.getAccount(mu.anchorProvider.connection, mu.depositPda);
        expect(providerAccount.amount).to.equals(9900000000n);
        expect(depositAccount.amount).to.equals(100000000n);
    });

    it("Fails to create region when provider isn't authorized", async () => {
        const rates: ServiceRates = {
            billionFunctionMbInstructions: new BN(1), // TODO too cheap to be priced correctly, even with 6 decimal places
            dbGigabyteMonths: new BN(1000),
            gigabytesGatewayTraffic: new BN(100),
            millionDbReads: new BN(500),
            millionDbWrites: new BN(2000),
            millionGatewayRequests: new BN(50)
        };

        try {
            let _ = await createRegion(mu, provider, "Region", 1, rates, new BN(50_000_000));
            throw new Error("Region creation succeeded when it should have failed");
        } catch (e) {
            let anchorError = e as AnchorError;
            expect(anchorError.message).to.contains("Provider is not authorized");
        }
    });

    it("Authorizes a provider", async () => {
        await authorizeProvider(mu, provider, providerAuthorizer);
    });

    it("Creates a region once provider is authorized", async () => {
        const rates: ServiceRates = {
            billionFunctionMbInstructions: new BN(1), // TODO too cheap to be priced correctly, even with 6 decimal places
            dbGigabyteMonths: new BN(1000),
            gigabytesGatewayTraffic: new BN(100),
            millionDbReads: new BN(500),
            millionDbWrites: new BN(2000),
            millionGatewayRequests: new BN(50)
        };

        region = await createRegion(mu, provider, "Region", 1, rates, new BN(50_000_000));
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
            provider,
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

        let commission = usagePrice * 100_000n / 1_000_000n;
        expect(commission).to.equals(102904n);
        let providerShare = usagePrice - commission;
        expect(providerShare).to.equals(926140n);

        const providerAccount = await spl.getAccount(
            mu.anchorProvider.connection, provider.tokenAccount
        );
        // 9900 $MU and 6 digits of decimal places left after paying deposit
        expect(providerAccount.amount).to.equals(9900_000_000n + providerShare);

        const commissionAccount = await spl.getAccount(
            mu.anchorProvider.connection, mu.commissionPda
        );
        expect(commissionAccount.amount).to.equals(commission);

        const escrowAccount = await spl.getAccount(
            mu.anchorProvider.connection,
            escrow.pda
        );
        expect(escrowAccount.amount).to.equals(10_000_000n - usagePrice);
    });

    it("Withdraws escrow balance", async () => {
        let tempWallet = await readOrCreateWallet(mu);

        await withdrawEscrowBalance(mu, escrow, userWallet, provider, tempWallet.tokenAccount, new BN(5_000_000));

        let tempWalletAccount = await spl.getAccount(mu.anchorProvider.connection, tempWallet.tokenAccount);
        expect(tempWalletAccount.amount).to.equals(10_000_000_000n + 5_000_000n); // 10G initial balance

        let escrowAccount = await spl.getAccount(mu.anchorProvider.connection, escrow.pda);
        expect(escrowAccount.amount).to.equals(5_000_000n - usagePrice); // 10M initial balance - 5M withdrawn - usage price
    })
});
