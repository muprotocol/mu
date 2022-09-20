import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { publicKey } from "@project-serum/anchor/dist/cjs/utils";
import { Marketplace } from "../target/types/marketplace";
import { Keypair, LAMPORTS_PER_SOL, Transaction, SystemProgram, PublicKey, ComputeBudgetProgram, ComputeBudgetInstruction } from '@solana/web3.js'
import * as spl from '@solana/spl-token';
import { expect } from "chai";

let createMint = async (provider): Promise<Keypair> => {
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

let createAndFundWallet = async (provider: anchor.AnchorProvider, mint: Keypair): Promise<[Keypair, PublicKey]> => {
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


describe("marketplace", () => {

  // Configure the client to use the local cluster.
  let provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Marketplace as Program<Marketplace>;

  let mint: Keypair;

  let providerWallet: Keypair;
  let providerTokenAccount: PublicKey;
  let providerPda: PublicKey;

  let regionPda: PublicKey;
  let authSignerWallet: Keypair;
  let authSignerPda: PublicKey;

  let escrowPda: PublicKey;
  let escrowBump;
  let userWallet: Keypair;
  let stackPda;

  let statePda: PublicKey;
  let depositPda: PublicKey;

  before(async () => {
    mint = await createMint(provider);
    [providerWallet, providerTokenAccount] = await createAndFundWallet(provider, mint);
    userWallet = (await createAndFundWallet(provider, mint))[0];

    statePda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("state")],
      program.programId
    )[0];

    depositPda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("deposit")],
      program.programId
    )[0];

    providerPda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("provider"), providerWallet.publicKey.toBuffer()],
      program.programId
    )[0];

    authSignerWallet = Keypair.generate();
    let fundTx = new Transaction();
    fundTx.add(SystemProgram.transfer({
      fromPubkey: provider.wallet.publicKey,
      toPubkey: authSignerWallet.publicKey,
      lamports: 5 * LAMPORTS_PER_SOL
    }));
    await provider.sendAndConfirm(fundTx);

  });

  it("Initializes", async () => {
    const tx = await program.methods.initialize().accounts({
      authority: provider.wallet.publicKey,
      state: statePda,
      depositToken: depositPda,
      mint: mint.publicKey,
    }).rpc();
  });

  it("Creates a new provider", async () => {
    await program.methods.createProvider("Provider").accounts({
      state: statePda,
      provider: providerPda,
      owner: providerWallet.publicKey,
      ownerToken: providerTokenAccount,
      depositToken: depositPda,
    }).signers([providerWallet]).rpc();


    const providerAccount = await spl.getAccount(provider.connection, providerTokenAccount);
    const depositAccount = await spl.getAccount(provider.connection, depositPda);
    expect(providerAccount.amount).to.equals(9900n);
    expect(depositAccount.amount).to.equals(100n);
  });

  it("Creates a region", async () => {
    regionPda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("region"), providerWallet.publicKey.toBytes(), new Uint8Array([1])],
      program.programId
    )[0];

    await program.methods.createRegion(
      1,
      "Region",
      3,
      {
        mudbGbMonth: new anchor.BN(65535),
        mufunctionCpuMem: new anchor.BN(10),
        bandwidth: new anchor.BN(10),
        gatewayMreqs: new anchor.BN(10)
      },
    ).accounts({
      provider: providerPda,
      region: regionPda,
      owner: providerWallet.publicKey
    }).signers([providerWallet]).rpc();

  });

  it("Creates an Authorized Usage Signer", async () => {
    authSignerPda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("authorized_signer"), regionPda.toBytes()],
      program.programId
    )[0];

    await program.methods.createAuthorizedUsageSigner(authSignerWallet.publicKey, providerTokenAccount).accounts({
      provider: providerPda,
      region: regionPda,
      authorizedSigner: authSignerPda,
      owner: providerWallet.publicKey,
    }).signers([providerWallet]).rpc();
  });

  it("Creates an escrow account", async () => {
    [escrowPda, escrowBump] = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("escrow"), userWallet.publicKey.toBytes(), providerPda.toBytes()],
      program.programId
    );

    await program.methods.createProviderEscrowAccount().accounts({
      escrowAccount: escrowPda,
      mint: mint.publicKey,
      user: userWallet.publicKey,
      provider: providerPda,
      state: statePda,
    }).signers([userWallet]).rpc();

    const escrowAccount = await spl.getAccount(provider.connection, escrowPda);
    expect(escrowAccount.amount).to.equals(0n);
  });

  it("Creates a stack", async () => {
    const stack_seed = new anchor.BN(100);
    stackPda = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("stack"), userWallet.publicKey.toBytes(), regionPda.toBytes(), stack_seed.toBuffer("be", 8)],
      program.programId
    )[0];

    await program.methods.createStack(9, stack_seed, Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8])).accounts({
      user: userWallet.publicKey,
      stack: stackPda,
      region: regionPda,
    }).signers([userWallet]).rpc();
  });

  it("Updates usage on a stack", async () => {
    const updateSeed = new anchor.BN(100);
    const [updatePda, updateBump] = publicKey.findProgramAddressSync(
      [anchor.utils.bytes.utf8.encode("update"), updateSeed.toBuffer("be", 8)],
      program.programId
    );

    //We need to deposit a prepaymet in the escrow account
    let mintToTx = new Transaction();
    mintToTx.add(spl.createMintToInstruction(mint.publicKey, escrowPda, provider.wallet.publicKey, 10000000));
    await provider.sendAndConfirm(mintToTx);

    await program.methods.updateUsage(
      updateSeed,
      escrowBump,
      {
        mudbGbMonth: new anchor.BN(2),
        mufunctionCpuMem: new anchor.BN(5),
        bandwidth: new anchor.BN(10),
        gatewayMreqs: new anchor.BN(100)
      }
    ).accounts({
      state: statePda,
      authorizedSigner: authSignerPda,
      region: regionPda,
      tokenAccount: providerTokenAccount,
      usageUpdate: updatePda,
      escrowAccount: escrowPda,
      stack: stackPda,
      signer: authSignerWallet.publicKey,
    }).signers([authSignerWallet]).rpc();


    const providerAccount = await spl.getAccount(provider.connection, providerTokenAccount);
    expect(providerAccount.amount).to.equals(142120n);

    const escrowAccount = await spl.getAccount(provider.connection, escrowPda);
    expect(escrowAccount.amount).to.equals(9867780n);
  });
});
