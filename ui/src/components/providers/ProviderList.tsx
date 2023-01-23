import * as anchor from "@project-serum/anchor"
import {useAnchorWallet, useConnection} from "@solana/wallet-adapter-react"
import {useEffect} from "react";
import getProgramId from "@/constants/getProgramId/getProgramId";
import {PublicKey} from "@solana/web3.js";
import {getMarketplaceIdl} from "@/constants/getMarketplaceIdl/getMarketplaceIdl";
import { Button } from "@mui/material";

export default function ProviderList() {
    const {connection} = useConnection();
    const anchorWallet = useAnchorWallet();

    useEffect(() => {
        if (anchorWallet) {
            const provider = new anchor.AnchorProvider(connection, anchorWallet, anchor.AnchorProvider.defaultOptions())
            // @ts-ignore
           const program = new anchor.Program(getMarketplaceIdl(), getProgramId(getMarketplaceIdl()), provider)
            // const program = new anchor.Program()

            const pb = new PublicKey("3s7nmF4GdKRcu616Xva62nJKqD4ePV3WG1K55EGZytDK");

            try {
                program.account.provider.all().then((res) => {
                    console.log(res[0])
                })
            } catch (e) {
            }

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
            //     getProgramId(),
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
            <Button variant="contained">lol</Button>
        </div>
    )
}