import { Keypair } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import chai, { expect } from 'chai';
import chaiAsPromised from 'chai-as-promised';
import {
    activateApiRequestSigner,
    authorizeProvider,
    createApiRequestSigner,
    createAuthorizedUsageSigner,
    createEscrowAccount,
    createMint,
    createProvider,
    createProviderAuthorizer,
    createRegion,
    deactivateApiRequestSigner,
    deleteStack,
    deployStack,
    initializeMu,
    mintToAccount,
    MuApiRequestSigner,
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
    updateStack,
    updateStackUsage,
    withdrawEscrowBalance
} from "../scripts/anchor-utils";
import { AnchorError, AnchorProvider, BN } from '@project-serum/anchor';

chai.use(chaiAsPromised);

describe("marketplace", () => {
    let mu: MuProgram;
    let providerAuthorizer: MuProviderAuthorizer;

    let provider: MuProviderInfo;
    let region: MuRegionInfo;
    let authSigner: MuAuthorizedSignerInfo;

    let userWallet: Keypair;
    let requestSigner: MuApiRequestSigner;
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
            functionMbTeraInstructions: new BN(1),
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
            functionMbTeraInstructions: new BN(1),
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
        const stackData = Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8]);
        stack = await deployStack(
            mu,
            userWallet,
            provider,
            region,
            stackData,
            100,
            "my stack"
        );

        let stackAccount = await mu.program.account.stack.fetch(stack.pda);
        assertActiveStackAccount(stackAccount, "my stack", stackData, 1);
    });

    it("Updates a stack with bigger data", async () => {
        const stackData = Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        await updateStack(
            mu,
            userWallet,
            region,
            stackData,
            100,
            "my stack now has a bigger name"
        );

        let stackAccount = await mu.program.account.stack.fetch(stack.pda);
        assertActiveStackAccount(stackAccount, "my stack now has a bigger name", stackData, 2);
    });

    it("Updates a stack with smaller data", async () => {
        const stackData = Buffer.from([0, 1, 2, 3, 4, 5]);
        await updateStack(
            mu,
            userWallet,
            region,
            stackData,
            100,
            "my s"
        );

        let stackAccount = await mu.program.account.stack.fetch(stack.pda);
        assertActiveStackAccount(stackAccount, "my s", stackData, 3);
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

    it("Deletes a stack", async () => {
        await deleteStack(mu, userWallet, region, 100);

        assertDeletedStackAccount(await mu.program.account.stack.fetch(stack.pda));
    });

    it("Cannot recreate a deleted stack", async () => {
        await expect(deployStack(
            mu,
            userWallet,
            provider,
            region,
            Buffer.from([]),
            100,
            "my stack"
        )).to.be.rejectedWith("custom program error: 0x0");
    });

    it("Cannot update a deleted stack", async () => {
        await expect(updateStack(
            mu,
            userWallet,
            region,
            Buffer.from([]),
            100,
            "my s"
        )).to.be.rejectedWith("CannotOperateOnDeletedStack");
    });

    it("Cannot delete a deleted stack", async () => {
        await expect(deleteStack(
            mu,
            userWallet,
            region,
            100
        )).to.be.rejectedWith("CannotOperateOnDeletedStack");
    });

    it("Can report usage on a deleted stack", async () => {
        const usage: ServiceUsage = {
            functionMbInstructions: new BN(2000 * 1000000000 * 512),
            dbBytesSeconds: new BN(500 * 1024 * 1024 * 60 * 60 * 24 * 15),
            dbReads: new BN(5000000),
            dbWrites: new BN(800000),
            gatewayRequests: new BN(4000000),
            gatewayTrafficBytes: new BN(5 * 1024 * 1024 * 1024)
        };

        await updateStackUsage(mu, region, stack, authSigner, provider, escrow, 101, usage);

        const escrowAccount = await spl.getAccount(
            mu.anchorProvider.connection,
            escrow.pda
        );
        expect(escrowAccount.amount).to.equals(10_000_000n - 2n * usagePrice);
    })

    it("Creates an API request signer", async () => {
        let signer = Keypair.generate(); // Note: can, but doesn't need to be an account on the blockchain
        requestSigner = await createApiRequestSigner(mu, userWallet, signer, region);

        let signerAccount = await mu.program.account.apiRequestSigner.fetch(requestSigner.pda);
        expect(signerAccount.active).to.be.true;
    });

    it("Deactivates an API request signer", async () => {
        await deactivateApiRequestSigner(mu, userWallet, requestSigner, region);

        let signerAccount = await mu.program.account.apiRequestSigner.fetch(requestSigner.pda);
        expect(signerAccount.active).to.be.false;
    });

    it("Re-activates an API request signer", async () => {
        await activateApiRequestSigner(mu, userWallet, requestSigner, region);

        let signerAccount = await mu.program.account.apiRequestSigner.fetch(requestSigner.pda);
        expect(signerAccount.active).to.be.true;
    });

    it("Withdraws escrow balance", async () => {
        let tempWallet = await readOrCreateWallet(mu);

        await withdrawEscrowBalance(mu, escrow, userWallet, provider, tempWallet.tokenAccount, new BN(5_000_000));

        let tempWalletAccount = await spl.getAccount(mu.anchorProvider.connection, tempWallet.tokenAccount);
        expect(tempWalletAccount.amount).to.equals(10_000_000_000n + 5_000_000n); // 10G initial balance

        let escrowAccount = await spl.getAccount(mu.anchorProvider.connection, escrow.pda);
        expect(escrowAccount.amount).to.equals(5_000_000n - 2n * usagePrice); // 10M initial balance - 5M withdrawn - usage price
    })
});

const assertActiveStackAccount = (account: any, name: string, stackData: Buffer, revision: number) => {
    expect(account.state["active"]).to.not.be.undefined;
    expect(account.state["deleted"]).to.be.undefined;
    expect(account.state["active"].name).to.equals(name);
    expect(account.state["active"].stackData).satisfies(bufferEqual(stackData));
    expect(account.state["active"].revision).to.equals(revision);
}

const assertDeletedStackAccount = (account: any) => {
    expect(account.state["active"]).to.be.undefined;
    expect(account.state["deleted"]).to.not.be.undefined;
}

const bufferEqual = (b1: Buffer) => (b2: Buffer) => {
    expect(b1.length).to.equals(b2.length);
    for (let i = 0; i < b1.length; ++i) {
        expect(b1[i]).to.equals(b2[i]);
    }
    return true;
}
