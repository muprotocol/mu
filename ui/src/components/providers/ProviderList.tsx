import {AnchorProvider, Program, utils} from "@project-serum/anchor"
import * as anchor from "@project-serum/anchor";
import {useAnchorWallet, useConnection} from "@solana/wallet-adapter-react"
import {useEffect} from "react";
import {publicKey} from "@project-serum/anchor/dist/cjs/utils";
import {Marketplace} from "../../../../marketplace/target/types/marketplace";
import {marketplace} from "@/constants/marketplace";
import muPublicKey from "@/constants/muPublicKey/muPublicKey";
import {Keypair, PublicKey} from "@solana/web3.js";


export default function ProviderList() {
    const { connection } = useConnection()
    const anchorWallet = useAnchorWallet();
    //marketplace.types[2].type.variants.indexOf({name: "ProviderRegion"}).toString()

    useEffect(() => {
        if (anchorWallet) {
            const provider = new anchor.AnchorProvider(connection, anchorWallet, anchor.AnchorProvider.defaultOptions())
            // @ts-ignore
            const program = new anchor.Program(marketplace, muPublicKey(), provider)

            // let content: Uint8Array = new Uint8Array([34,150,7,69,103,71,242,161,234,197,147,30,217,122,168,184,74,90,125,225,246,175,250,133,60,253,97,250,190,124,188,23,131,18,192,100,141,6,56,14,51,42,3,182,190,52,232,248,94,65,35,199,43,232,201,141,172,13,146,125,7,171,211,175])
            // let text = Buffer.from(content).toString();
            // let json = JSON.parse(text);
            // let bytes = Uint8Array.from(json);
            // console.log(Keypair.fromSecretKey(bytes).publicKey.toBase58())

            const pb = new PublicKey("3s7nmF4GdKRcu616Xva62nJKqD4ePV3WG1K55EGZytDK");

            // const IB = Keypair.fromSecretKey(new Uint8Array([34,150,7,69,103,71,242,161,234,197,147,30,217,122,168,184,74,90,125,225,246,175,250,133,60,253,97,250,190,124,188,23,131,18,192,100,141,6,56,14,51,42,3,182,190,52,232,248,94,65,35,199,43,232,201,141,172,13,146,125,7,171,211,175]))
            // console.log(IB.publicKey.toBase58())

            try {
                program.account.provider.all().then((res) => {
                    console.log(res[0])
                })
            } catch (e) {}

            try {
                program.account.providerRegion.all(
                    [
                        {
                            memcmp: {
                                offset: 8 + 1,
                                bytes: pb.toBase58() // specific providerRegion from publicKey of provider
                            }
                        }
                    ]
                ).then(console.log)
            } catch (e) {
                console.log(e)
            }
            // console.log(program)
            // const statePda = publicKey.findProgramAddressSync(
            //     [anchor.utils.bytes.utf8.encode("state")],
            //     program.programId
            // )[0];


            // const provider = new AnchorProvider(connection, anchorWallet, AnchorProvider.defaultOptions())
            // provider.connection.getProgramAccounts(
            //     muPublicKey(),
            //     {
            //         filters: [
            //             {
            //                 memcmp: {
            //                     offset: 8,
            //                     bytes: utils.bytes.bs58.encode([1]), // get from mu
            //                 },
            //             },
            //         ],
            //     }
            // ).then((response) => {
            //     console.log(response[0].account)
            // })
        } else {
            console.group("anchorWallet")
            console.error(anchorWallet);
            console.table({...connection});
            console.groupEnd();
        }
    }, [anchorWallet, connection])


    return (
        <div className="container mx-auto">
            lol
        </div>
    )
}