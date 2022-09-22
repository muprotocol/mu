import * as anchor from "@project-serum/anchor";
import { Address, Program } from "@project-serum/anchor";
import { publicKey } from "@project-serum/anchor/dist/cjs/utils";
import { Marketplace } from "../target/types/marketplace";
import { Keypair, LAMPORTS_PER_SOL, Transaction, SystemProgram, PublicKey } from '@solana/web3.js'
import * as spl from '@solana/spl-token';

export class ServiceUnits {
	mudb_gb_month: number;
	mufunction_cpu_mem: number;
	bandwidth: number;
	gateway_mreqs: number;
}

const serviceUnitsToAnchorTypes = (usage: ServiceUnits): any => {
	return {
		mudbGbMonth: new anchor.BN(usage.mudb_gb_month),
		mufunctionCpuMem: new anchor.BN(usage.mufunction_cpu_mem),
		bandwidth: new anchor.BN(usage.bandwidth),
		gatewayMreqs: new anchor.BN(usage.gateway_mreqs)
	};
}

export const createMint = async (provider: anchor.AnchorProvider): Promise<Keypair> => {
	const mint = Keypair.generate();
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

export const createAndFundWallet = async (provider: anchor.AnchorProvider, mint: Keypair): Promise<[Keypair, PublicKey]> => {
	let wallet = Keypair.generate();

	let fundTx = new Transaction();
	fundTx.add(SystemProgram.transfer({
		fromPubkey: provider.wallet.publicKey,
		toPubkey: wallet.publicKey,
		lamports: 5 * LAMPORTS_PER_SOL
	}));
	await provider.sendAndConfirm(fundTx);

	let tokenAccount = await spl.createAssociatedTokenAccount(
		provider.connection,
		wallet,
		mint.publicKey,
		wallet.publicKey
	);

	let mintToTx = new Transaction();
	mintToTx.add(spl.createMintToInstruction(mint.publicKey, tokenAccount, provider.wallet.publicKey, 10000));
	await provider.sendAndConfirm(mintToTx);

	return [wallet, tokenAccount];
};

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

export class MuMarketplaceClient {
	anchor_provider: anchor.AnchorProvider;
	program: Program<Marketplace>;
	mint: Keypair;
	statePda: PublicKey;
	depositPda: PublicKey;

	constructor(
		wallet_provider: anchor.AnchorProvider,
		program: Program<Marketplace>,
		mint: Keypair,
		statePda: PublicKey,
		depositPda: PublicKey,
	) {
		this.anchor_provider = wallet_provider;
		this.program = program;
		this.mint = mint;
		this.statePda = statePda;
		this.depositPda = depositPda;
	}

	static async initialize(): Promise<MuMarketplaceClient> {
		const anchor_provider = anchor.AnchorProvider.env();
		anchor.setProvider(anchor_provider);
		const mint = await createMint(anchor_provider);
		const program = anchor.workspace.Marketplace as Program<Marketplace>;

		const statePda = publicKey.findProgramAddressSync(
			[anchor.utils.bytes.utf8.encode("state")],
			program.programId
		)[0];

		const depositPda = publicKey.findProgramAddressSync(
			[anchor.utils.bytes.utf8.encode("deposit")],
			program.programId
		)[0];

		await program.methods.initialize().accounts({
			authority: anchor_provider.wallet.publicKey,
			state: statePda,
			depositToken: depositPda,
			mint: mint.publicKey,
		}).rpc();

		return new MuMarketplaceClient(
			anchor_provider,
			program,
			mint,
			statePda,
			depositPda,
		);
	}

	async createProvider(name: string): Promise<[Keypair, PublicKey, PublicKey]> {
		const [providerWallet, providerTokenAccount] =
			await createAndFundWallet(this.anchor_provider, this.mint);

		const providerPda = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("provider"),
				providerWallet.publicKey.toBuffer()
			],
			this.program.programId
		)[0];

		await this.program.methods.createProvider(name).accounts({
			state: this.statePda,
			provider: providerPda,
			owner: providerWallet.publicKey,
			ownerToken: providerTokenAccount,
			depositToken: this.depositPda,
		}).signers([providerWallet]).rpc();

		return [providerWallet, providerPda, providerTokenAccount];
	}

	async createRegion(
		name: string,
		regionNum: number,
		providerWallet: Keypair,
		providerPda: Address,
		rates: ServiceUnits,
		zones: number,
	): Promise<PublicKey> {
		const regionPda = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("region"),
				providerWallet.publicKey.toBytes(),
				new Uint8Array([regionNum])
			],
			this.program.programId
		)[0];

		await this.program.methods.createRegion(
			regionNum,
			name,
			zones,
			serviceUnitsToAnchorTypes(rates)
		).accounts({
			provider: providerPda,
			region: regionPda,
			owner: providerWallet.publicKey
		}).signers([providerWallet]).rpc();

		return regionPda;
	}

	async createAuthorizedUsageSigner(
		providerWallet: Keypair,
		providerPda: Address,
		providerTokenAccount: PublicKey,
		regionPda: PublicKey
	): Promise<[Keypair, PublicKey]> {
		const authSignerWallet = Keypair.generate();
		let fundTx = new Transaction();
		fundTx.add(SystemProgram.transfer({
			fromPubkey: this.anchor_provider.wallet.publicKey,
			toPubkey: authSignerWallet.publicKey,
			lamports: 5 * LAMPORTS_PER_SOL
		}));
		await this.anchor_provider.sendAndConfirm(fundTx);

		const authSignerPda = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("authorized_signer"),
				regionPda.toBytes()
			],
			this.program.programId
		)[0];

		await
			this.program.methods.createAuthorizedUsageSigner(
				authSignerWallet.publicKey,
				providerTokenAccount
			).accounts({
				provider: providerPda,
				region: regionPda,
				authorizedSigner: authSignerPda,
				owner: providerWallet.publicKey,
			}).signers([providerWallet]).rpc();

		return [authSignerWallet, authSignerPda];
	}

	async createEscrowAccount(
		userWallet: Keypair,
		providerPda: PublicKey,
	): Promise<[PublicKey, number]> {
		const [escrowPda, escrowBump] = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("escrow"),
				userWallet.publicKey.toBytes(),
				providerPda.toBytes()
			],
			this.program.programId
		);

		await this.program.methods.createProviderEscrowAccount().accounts({
			escrowAccount: escrowPda,
			mint: this.mint.publicKey,
			user: userWallet.publicKey,
			provider: providerPda,
			state: this.statePda,
		}).signers([userWallet]).rpc();

		return [escrowPda, escrowBump];
	}

	async deployStack(
		userWallet: Keypair,
		regionPda: PublicKey,
		stack: Buffer,
		stackSeed: number,
	): Promise<PublicKey> {
		const stack_seed = new anchor.BN(stackSeed);
		const stackPda = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("stack"),
				userWallet.publicKey.toBytes(),
				regionPda.toBytes(),
				stack_seed.toBuffer("be", 8)],
			this.program.programId
		)[0];

		await
			this.program.methods.createStack(
				stack.byteLength,
				stack_seed,
				stack,
			).accounts({
				user: userWallet.publicKey,
				stack: stackPda,
				region: regionPda,
			}).signers([userWallet]).rpc();

		return stackPda;
	}

	async updateStackUsage(
		regionPda: PublicKey,
		stackPda: PublicKey,
		authSignerWallet: Keypair,
		authSignerPda: PublicKey,
		providerTokenAccount: PublicKey,
		escrowPda: PublicKey,
		escrowBump: number,
		updateSeed: number,
		usage: ServiceUnits,
	): Promise<[PublicKey, number]> {
		const [updatePda, updateBump] = publicKey.findProgramAddressSync(
			[
				anchor.utils.bytes.utf8.encode("update"),
				new anchor.BN(updateSeed).toBuffer("be", 8)
			],
			this.program.programId
		);

		await this.program.methods.updateUsage(
			new anchor.BN(updateSeed),
			escrowBump,
			serviceUnitsToAnchorTypes(usage),
		).accounts({
			state: this.statePda,
			authorizedSigner: authSignerPda,
			region: regionPda,
			tokenAccount: providerTokenAccount,
			usageUpdate: updatePda,
			escrowAccount: escrowPda,
			stack: stackPda,
			signer: authSignerWallet.publicKey,
		}).signers([authSignerWallet]).rpc();

		return [updatePda, updateBump];
	}
}
