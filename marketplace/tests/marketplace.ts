import { Keypair, PublicKey } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import { expect } from "chai";
import { MuMarketplaceClient, createAndFundWallet, mintToAccount } from "../scripts/anchor-utils";

describe("marketplace", () => {
	let client: MuMarketplaceClient;

	let providerWallet: Keypair;
	let providerTokenAccount: PublicKey;
	let providerPda: PublicKey;

	let regionPda: PublicKey;
	let authSignerWallet: Keypair;
	let authSignerPda: PublicKey;

	let userWallet: Keypair;

	let escrowPda: PublicKey;
	let escrowBump: number;

	let stackPda: PublicKey;

	it("Initializes", async () => {
		client = await MuMarketplaceClient.initialize();
	});

	it("Creates a new provider", async () => {
		[providerWallet, providerPda, providerTokenAccount] =
			await client.createProvider("Provider");

		const providerAccount = await spl.getAccount(client.anchor_provider.connection, providerTokenAccount);
		const depositAccount = await spl.getAccount(client.anchor_provider.connection, client.depositPda);
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

		regionPda = await client.createRegion("Region", 1, providerWallet, providerPda, rates, 3);
	});

	it("Creates an Authorized Usage Signer", async () => {
		[authSignerWallet, authSignerPda] =
			await client.createAuthorizedUsageSigner(
				providerWallet,
				providerPda,
				providerTokenAccount,
				regionPda
			);
	});

	it("Creates an escrow account", async () => {
		userWallet = (await createAndFundWallet(client.anchor_provider, client.mint))[0];
		[escrowPda, escrowBump] = await client.createEscrowAccount(userWallet, providerPda);

		const escrowAccount = await spl.getAccount(client.anchor_provider.connection, escrowPda);
		expect(escrowAccount.amount).to.equals(0n);
	});

	it("Creates a stack", async () => {
		stackPda = await client.deployStack(
			userWallet,
			regionPda,
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

		await mintToAccount(client.anchor_provider, escrowPda, client.mint, 10000000);

		await client.updateStackUsage(
			regionPda,
			stackPda,
			authSignerWallet,
			authSignerPda,
			providerTokenAccount,
			escrowPda,
			escrowBump,
			100,
			rates);

		const providerAccount = await spl.getAccount(
			client.anchor_provider.connection, providerTokenAccount
		);
		expect(providerAccount.amount).to.equals(142120n);

		const escrowAccount = await spl.getAccount(
			client.anchor_provider.connection,
			escrowPda
		);
		expect(escrowAccount.amount).to.equals(9867780n);
	});
});
