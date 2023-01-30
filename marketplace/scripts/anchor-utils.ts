import * as anchor from "@project-serum/anchor";
import { BN, Program } from "@project-serum/anchor";
import { publicKey } from "@project-serum/anchor/dist/cjs/utils";
import { Marketplace } from "../target/types/marketplace";
import { Keypair, LAMPORTS_PER_SOL, Transaction, SystemProgram, PublicKey } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import path from "path";
import { existsSync, readFileSync, writeFileSync } from "fs";
import promptSync from "prompt-sync";
import { sleep, waitUntilPortUsed } from "./util";
import { env, memoryUsage } from "process";
import { homedir } from "os";

export const canConnectToLocalValidator = async () => {
    try {

        let provider = anchor.AnchorProvider.local();
        await provider.connection.getTransactionCount();
        return true;
    } catch (e) {
        if (e.toString().includes('The "path" argument must be of type string or an instance of Buffer or URL.')) {
            // This error won't be fixed by waiting, it's due to the ANCHOR_WALLET env var being absent
            throw e;
        }

        return false;
    }
}

export const promptForRemovalIfLedgerExists = () => {
    if (existsSync("test-ledger")) {
        let prompt = promptSync();
        if (!process.argv.includes("-y") &&
            prompt("This command will delete the Solana ledger in ./test-ledger, are you sure? [y/n] ") != "y")
            return false;
    }
    return true;
}

export const getSolanaValidatorCommand = () => {
    if (process.argv.includes('-v')) {
        return "export RUST_LOG=solana_runtime::system_instruction_processor=trace," +
            "solana_runtime::message_processor=trace,solana_bpf_loader=debug,solana_rbpf=trace && " +
            "solana-test-validator --log -r";
    } else {
        return "export RUST_LOG=solana_runtime::system_instruction_processor=trace," +
            "solana_runtime::message_processor=info,solana_bpf_loader=debug,solana_rbpf=trace && " +
            "solana-test-validator --log -r"
    }
}

export const waitForLocalValidatorToStart = async () => {
    console.log("Waiting for validator to start");
    await waitUntilPortUsed(8899);
    env.ANCHOR_WALLET = getDefaultWalletPath();
    while (!(await canConnectToLocalValidator())) {
        await sleep(0.5);
    }
}

export const getDefaultWalletPath = () =>
    path.resolve(homedir(), ".config/solana/id.json");


export interface ServiceRates {
    billionFunctionMbInstructions: BN,
    dbGigabyteMonths: BN,
    millionDbReads: BN,
    millionDbWrites: BN,
    millionGatewayRequests: BN,
    gigabytesGatewayTraffic: BN,
}

export interface ServiceUsage {
    functionMbInstructions: BN,
    dbBytesSeconds: BN,
    dbReads: BN,
    dbWrites: BN,
    gatewayRequests: BN,
    gatewayTrafficBytes: BN,
}

export const readKeypair = (path: string): Keypair | undefined => {
    if (existsSync(path)) {
        try {
            let content: Uint8Array = readFileSync(path);
            let text = Buffer.from(content).toString();
            let json = JSON.parse(text);
            let bytes = Uint8Array.from(json);
            return Keypair.fromSecretKey(bytes);
        } catch (e) {
            return undefined;
        }
    } else {
        return undefined;
    }
}

export const readOrCreateKeypair = (name?: string): Keypair => {
    if (!name) {
        return Keypair.generate();
    }

    let walletPath = path.join(__dirname, "test-wallets", name + ".json");
    let keypair = readKeypair(walletPath);

    if (keypair) {
        return keypair;
    }

    keypair = Keypair.generate(); // anchor.Wallet has no constructor
    console.log(`Generated keypair ${name}, public key is:`, keypair.publicKey.toBase58());
    let secretkey = Array.from(keypair.secretKey);
    writeFileSync(walletPath, JSON.stringify(secretkey));
    return keypair;
}

export const createMint = async (provider: anchor.AnchorProvider, useStaticKeypair?: boolean): Promise<Keypair> => {
    const mint = readOrCreateKeypair(useStaticKeypair ? "mint" : undefined);

    const mintRent = await provider.connection.getMinimumBalanceForRentExemption(spl.MintLayout.span);

    const tx = new Transaction();
    tx.add(
        SystemProgram.createAccount({
            programId: spl.TOKEN_PROGRAM_ID,
            space: spl.MintLayout.span,
            fromPubkey: provider.wallet.publicKey,
            newAccountPubkey: mint.publicKey,
            lamports: mintRent,
        })
    );

    tx.add(
        spl.createInitializeMintInstruction(
            mint.publicKey,
            6,
            provider.wallet.publicKey,
            provider.wallet.publicKey,
        )
    );

    await provider.sendAndConfirm(tx, [mint]);

    return mint;
}

export const readMintFromStaticKeypair = () => readOrCreateKeypair("mint");

const createAndFundWallet = async (provider: anchor.AnchorProvider, mint: Keypair, keypairName?: string): Promise<[Keypair, PublicKey]> => {
    let wallet = readOrCreateKeypair(keypairName);

    let account = await provider.connection.getAccountInfo(wallet.publicKey);

    if (!account || account.lamports < 5 * LAMPORTS_PER_SOL) {
        let fundTx = new Transaction();
        fundTx.add(SystemProgram.transfer({
            fromPubkey: provider.wallet.publicKey,
            toPubkey: wallet.publicKey,
            lamports: 5 * LAMPORTS_PER_SOL
        }));
        await provider.sendAndConfirm(fundTx);
    }

    let tokenAccount = await spl.getOrCreateAssociatedTokenAccount(
        provider.connection,
        wallet,
        mint.publicKey,
        wallet.publicKey
    );

    if (tokenAccount.amount < 10000_000000) { // we have 6 decimal places in the mint
        let mintToTx = new Transaction();
        mintToTx.add(spl.createMintToInstruction(mint.publicKey, tokenAccount.address, provider.wallet.publicKey, 10000_000000));
        await provider.sendAndConfirm(mintToTx);
    }

    return [wallet, tokenAccount.address];
};

export const walletFromKeypair = async (wallet: Keypair, mint: Keypair): Promise<[Keypair, PublicKey]> => {
    let tokenAccount = await spl.getAssociatedTokenAddress(mint.publicKey, wallet.publicKey);
    return [wallet, tokenAccount];
}

export const loadWallet = async (name: string, mint: Keypair): Promise<[Keypair, PublicKey]> => {
    let wallet = readOrCreateKeypair(name);
    return walletFromKeypair(wallet, mint);
}

export const mintToAccount = async (provider: anchor.AnchorProvider, account: PublicKey, mint: Keypair, amount: number) => {
    let mintToTx = new Transaction();
    mintToTx.add(spl.createMintToInstruction(
        mint.publicKey,
        account,
        provider.wallet.publicKey,
        amount,
    ));
    await provider.sendAndConfirm(mintToTx);
}

export interface MuProgram {
    anchorProvider: anchor.AnchorProvider,
    mint: Keypair,
    program: anchor.Program<Marketplace>;
    statePda: anchor.web3.PublicKey;
    depositPda: anchor.web3.PublicKey;
    commissionPda: anchor.web3.PublicKey;
}

export const getMu = (anchorProvider: anchor.AnchorProvider, mint: Keypair): MuProgram => {
    anchor.setProvider(anchorProvider);

    const program = anchor.workspace.Marketplace as Program<Marketplace>;

    const statePda = publicKey.findProgramAddressSync(
        [anchor.utils.bytes.utf8.encode("state")],
        program.programId
    )[0];

    const depositPda = publicKey.findProgramAddressSync(
        [anchor.utils.bytes.utf8.encode("deposit")],
        program.programId
    )[0];

    const commissionPda = publicKey.findProgramAddressSync(
        [anchor.utils.bytes.utf8.encode("commission")],
        program.programId
    )[0];

    return {
        anchorProvider,
        mint,
        program,
        statePda,
        depositPda,
        commissionPda,
    };

}

export const initializeMu = async (anchorProvider: anchor.AnchorProvider, mint: Keypair, commission_rate_micros: number): Promise<MuProgram> => {
    let mu = getMu(anchorProvider, mint);

    await mu.program.methods.initialize(commission_rate_micros).accounts({
        authority: anchorProvider.wallet.publicKey,
        state: mu.statePda,
        depositToken: mu.depositPda,
        commissionToken: mu.commissionPda,
        mint: mint.publicKey,
    }).rpc();

    return mu;
}

export interface MuProviderInfo {
    wallet: anchor.web3.Keypair;
    pda: anchor.web3.PublicKey;
    tokenAccount: anchor.web3.PublicKey;
}

export const createProvider = async (mu: MuProgram, name: string, useStaticKeypair?: boolean): Promise<MuProviderInfo> => {
    const [wallet, tokenAccount] =
        await createAndFundWallet(mu.anchorProvider, mu.mint, useStaticKeypair ? `provider_${name}` : undefined);

    const pda = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("provider"),
            wallet.publicKey.toBuffer()
        ],
        mu.program.programId
    )[0];

    await mu.program.methods.createProvider(name).accounts({
        state: mu.statePda,
        provider: pda,
        owner: wallet.publicKey,
        ownerToken: tokenAccount,
        depositToken: mu.depositPda,
    }).signers([wallet]).rpc();

    return { wallet, pda, tokenAccount };
}

function getProviderPda(mu: MuProgram, wallet: Keypair) {
    const [pda, _] = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("provider"),
            wallet.publicKey.toBuffer()
        ],
        mu.program.programId
    );

    return pda;
}

export const loadProviderFromKeypair = async (mu: MuProgram, keypair: Keypair): Promise<MuProviderInfo> => {
    let [wallet, tokenAccount] = await walletFromKeypair(keypair, mu.mint);

    const pda = getProviderPda(mu, wallet);

    return { wallet, pda, tokenAccount };
}

export const loadProviderFromStaticKeypair = async (mu: MuProgram, name: string): Promise<MuProviderInfo> => {
    let [wallet, tokenAccount] = await loadWallet(`provider_${name}`, mu.mint);

    const pda = getProviderPda(mu, wallet);

    return { wallet, pda, tokenAccount };
}

export interface MuProviderAuthorizer {
    keypair: Keypair,
    pda: PublicKey,
}

export const readProviderAuthorizer = (mu: MuProgram, name?: string): MuProviderAuthorizer => {
    let keypair = readOrCreateKeypair(name === undefined ? undefined : `authorizer-${name}`);

    const [authorizerPda, _] = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("authorizer"),
            keypair.publicKey.toBytes(),
        ],
        mu.program.programId,
    );

    return {
        keypair,
        pda: authorizerPda
    }
}

export const createProviderAuthorizer = async (mu: MuProgram, name?: string): Promise<MuProviderAuthorizer> => {
    let authorizer = readProviderAuthorizer(mu, name);

    await mu.program.methods.createProviderAuthorizer().accounts({
        state: mu.statePda,
        providerAuthorizer: authorizer.pda,
        authority: mu.anchorProvider.wallet.publicKey,
        authorizer: authorizer.keypair.publicKey,
    }).signers([authorizer.keypair]).rpc();

    return authorizer;
}

export const authorizeProvider = async (mu: MuProgram, provider: MuProviderInfo, authorizer: MuProviderAuthorizer) => {
    await mu.program.methods.authorizeProvider().accounts({
        authorizer: authorizer.keypair.publicKey,
        providerAuthorizer: authorizer.pda,
        owner: provider.wallet.publicKey,
        provider: provider.pda,
    }).signers([authorizer.keypair]).rpc();
};

export interface MuRegionInfo {
    pda: PublicKey
}

export const getRegion = (mu: MuProgram, provider: MuProviderInfo, regionNum: number): MuRegionInfo => {
    const pda = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("region"),
            provider.wallet.publicKey.toBytes(),
            new anchor.BN(regionNum, 10, "le").toBuffer("le", 4)
        ],
        mu.program.programId
    )[0];

    return { pda };
}

export const createRegion = async (
    mu: MuProgram,
    provider: MuProviderInfo,
    name: string,
    regionNum: number,
    rates: ServiceRates,
    minEscrowBalance: BN,
): Promise<MuRegionInfo> => {
    let region = getRegion(mu, provider, regionNum);

    await mu.program.methods
        .createRegion(regionNum, name, rates, minEscrowBalance)
        .accounts({
            provider: provider.pda,
            region: region.pda,
            owner: provider.wallet.publicKey
        }).signers([provider.wallet]).rpc();

    return region;
}

export interface MuAuthorizedSignerInfo {
    wallet: Keypair,
    pda: PublicKey
}

export const createAuthorizedUsageSigner = async (
    mu: MuProgram,
    provider: MuProviderInfo,
    region: MuRegionInfo,
    keypairName?: string,
): Promise<MuAuthorizedSignerInfo> => {
    const wallet = readOrCreateKeypair(keypairName);

    let fundTx = new Transaction();
    fundTx.add(SystemProgram.transfer({
        fromPubkey: mu.anchorProvider.wallet.publicKey,
        toPubkey: wallet.publicKey,
        lamports: 5 * LAMPORTS_PER_SOL
    }));
    await mu.anchorProvider.sendAndConfirm(fundTx);

    const pda = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("authorized_signer"),
            region.pda.toBytes()
        ],
        mu.program.programId
    )[0];

    await
        mu.program.methods.createAuthorizedUsageSigner(
            wallet.publicKey,
            provider.tokenAccount
        ).accounts({
            provider: provider.pda,
            region: region.pda,
            authorizedSigner: pda,
            owner: provider.wallet.publicKey,
        }).signers([provider.wallet]).rpc();

    return { wallet, pda };
}

export interface UserWallet {
    keypair: Keypair,
    tokenAccount: PublicKey
}

export const readOrCreateWallet = async (mu: MuProgram, name?: string): Promise<UserWallet> => {
    let [keypair, tokenAccount] = await createAndFundWallet(mu.anchorProvider, mu.mint, name);
    return { keypair, tokenAccount };
}

export const readOrCreateUserWallet = async (mu: MuProgram, userIndex?: number): Promise<UserWallet> => {
    let [keypair, tokenAccount] = await createAndFundWallet(mu.anchorProvider, mu.mint, userIndex === undefined ? undefined : `user_${userIndex}`);
    return { keypair, tokenAccount };
}

export interface MuEscrowAccountInfo {
    pda: PublicKey,
    bump: number
}

export const createEscrowAccount = async (
    mu: MuProgram,
    userWallet: Keypair,
    provider: MuProviderInfo,
): Promise<MuEscrowAccountInfo> => {
    // Note: the escrow accounts are SPL token accounts, so we can't store a bump in them
    // and need to calculate it on the client side each time.
    const [pda, bump] = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("escrow"),
            userWallet.publicKey.toBytes(),
            provider.pda.toBytes()
        ],
        mu.program.programId
    );

    await mu.program.methods.createProviderEscrowAccount().accounts({
        escrowAccount: pda,
        mint: mu.mint.publicKey,
        user: userWallet.publicKey,
        provider: provider.pda,
        state: mu.statePda,
    }).signers([userWallet]).rpc();

    return { pda, bump };
}

export const getEscrowAccount = (
    mu: MuProgram,
    userWallet: Keypair,
    provider: MuProviderInfo,
): MuEscrowAccountInfo => {
    // Note: the escrow accounts are SPL token accounts, so we can't store a bump in them
    // and need to calculate it on the client side each time.
    const [pda, bump] = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("escrow"),
            userWallet.publicKey.toBytes(),
            provider.pda.toBytes()
        ],
        mu.program.programId
    );

    return { pda, bump };
}

export interface MuStackInfo {
    pda: PublicKey
}

export const deployStack = async (
    mu: MuProgram,
    userWallet: Keypair,
    provider: MuProviderInfo,
    region: MuRegionInfo,
    stack: Buffer,
    stackSeed: number,
    name: string
): Promise<MuStackInfo> => {
    const stack_seed = new anchor.BN(stackSeed);
    const pda = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("stack"),
            userWallet.publicKey.toBytes(),
            region.pda.toBytes(),
            stack_seed.toBuffer("le", 8)],
        mu.program.programId
    )[0];

    await
        mu.program.methods.createStack(
            stack_seed,
            stack,
            name
        ).accounts({
            user: userWallet.publicKey,
            stack: pda,
            region: region.pda,
            provider: provider.pda,

        }).signers([userWallet]).rpc();

    return { pda };
}

export interface MuStackUsageUpdateInfo {
    pda: PublicKey,
    bump: number
}

export const updateStackUsage = async (
    mu: MuProgram,
    region: MuRegionInfo,
    stack: MuStackInfo,
    authSigner: MuAuthorizedSignerInfo,
    provider: MuProviderInfo,
    escrow: MuEscrowAccountInfo,
    updateSeed: number, // This is actually a 128-bit number, but a float64 is enough for testing purposes
    usage: ServiceUsage
): Promise<MuStackUsageUpdateInfo> => {
    // Providers won't have access to the escrow account in the same way we
    // do here, so they'll have to calculate it from the `user` field of the
    // stack and their own public key.
    const [pda, bump] = publicKey.findProgramAddressSync(
        [
            anchor.utils.bytes.utf8.encode("update"),
            stack.pda.toBytes(),
            region.pda.toBytes(),
            new anchor.BN(updateSeed).toBuffer("le", 16)
        ],
        mu.program.programId
    );

    await mu.program.methods.updateUsage(
        new anchor.BN(updateSeed),
        escrow.bump,
        usage,
    ).accounts({
        state: mu.statePda,
        commissionToken: mu.commissionPda,
        authorizedSigner: authSigner.pda,
        region: region.pda,
        tokenAccount: provider.tokenAccount,
        usageUpdate: pda,
        escrowAccount: escrow.pda,
        stack: stack.pda,
        signer: authSigner.wallet.publicKey,
    }).signers([authSigner.wallet]).rpc();

    return { pda, bump };
}

export const withdrawEscrowBalance = async (
    mu: MuProgram,
    escrowAccount: MuEscrowAccountInfo,
    userWallet: Keypair,
    provider: MuProviderInfo,
    withdrawTo: PublicKey,
    amount: BN
) => {
    await mu.program.methods
        .withdrawEscrowBalance(amount)
        .accounts({
            state: mu.statePda,
            escrowAccount: escrowAccount.pda,
            provider: provider.pda,
            user: userWallet.publicKey,
            withdrawTo
        })
        .signers([userWallet])
        .rpc();
}