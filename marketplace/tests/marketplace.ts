import { Keypair, PublicKey } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import { expect } from "chai";
import { createAndFundWallet, createAuthorizedUsageSigner, createEscrowAccount, createMint, createProvider, createRegion, deployStack, initializeMu, mintToAccount, MuAuthorizedSignerInfo, MuEscrowAccountInfo, MuProgram, MuProviderInfo, MuRegionInfo, MuStackInfo, updateStackUsage } from "../scripts/anchor-utils";
import { AnchorProvider } from '@project-serum/anchor';

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
		expect(providerAccount.amount).to.equals(9900n);
		expect(depositAccount.amount).to.equals(100n);
	});

	it("Creates a region", async () => {
		const rates = {
			mudb_gb_month: 65535,
			mufunction_cpu_mem: 10,
			bandwidth: 10,
			gateway_mreqs: 10,
		};

		region = await createRegion(mu, provider, "Region", 1, rates, 3);
	});

	it("Creates an Authorized Usage Signer", async () => {
		authSigner = await createAuthorizedUsageSigner(mu, provider, region);
	});

	it("Creates an escrow account", async () => {
		userWallet = await createAndFundWallet(mu.anchorProvider, mu.mint)[0];
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
			100
		);
	});

	it("Updates usage on a stack", async () => {
		const rates = {
			mudb_gb_month: 2,
			mufunction_cpu_mem: 5,
			bandwidth: 10,
			gateway_mreqs: 100,
		};

		await mintToAccount(mu.anchorProvider, escrow.pda, mu.mint, 10_000_000);

		await updateStackUsage(
			mu,
			region,
			stack,
			authSigner,
			provider,
			escrow,
			100,
			rates);

		const providerAccount = await spl.getAccount(
			mu.anchorProvider.connection, provider.tokenAccount
		);
		expect(providerAccount.amount).to.equals(142_120n);

		const escrowAccount = await spl.getAccount(
			mu.anchorProvider.connection,
			escrow.pda
		);
		expect(escrowAccount.amount).to.equals(9_867_780n);
	});
});
