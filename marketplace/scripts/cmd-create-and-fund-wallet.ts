import { AnchorProvider } from "@project-serum/anchor";
import { getMu, readMintFromStaticKeypair, readOrCreateUserWallet } from "./anchor-utils";
import util from "./util"

util.asyncMain(async () => {
	let user_index = process.argv[2];
    let anchorProvider = AnchorProvider.local("http://127.0.0.1:8899");

    let mint = readMintFromStaticKeypair();
	let mu = getMu(anchorProvider, mint);

	await readOrCreateUserWallet(mu, Number.parseInt(user_index));
});
