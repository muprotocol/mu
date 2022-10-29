import { AnchorProvider } from "@project-serum/anchor";
import { getMu, readMintFromStaticKeypair, readOrCreateWallet } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
	let walletName = process.argv[2];
	let anchorProvider = AnchorProvider.local();

	let mint = readMintFromStaticKeypair();
	let mu = getMu(anchorProvider, mint);

	await readOrCreateWallet(mu, walletName);
});
