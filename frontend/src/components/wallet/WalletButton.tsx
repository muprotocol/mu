import dynamic from "next/dynamic";
import {CircularProgress} from "@mui/material";

export default function WalletButton() {
    const WalletMultiButtonDynamic = dynamic(
        async () => (await import('@solana/wallet-adapter-react-ui')).WalletMultiButton,
        {
            ssr: false,
            loading: () => <CircularProgress size={"2rem"}/>
        }
    );

    return <WalletMultiButtonDynamic className="bg-purple-800"/>
}