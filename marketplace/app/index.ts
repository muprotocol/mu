import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { Marketplace } from "../target/types/marketplace";

async function main() {
    const program = anchor.workspace.AnchorTest as Program<Marketplace>;
    console.log(program.programId.toBase58());
    //    const tx = await program.methods.initialize().rpc();
    //    console.log("Your transaction signature", tx);
}

main();