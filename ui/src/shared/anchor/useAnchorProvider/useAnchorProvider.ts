import { useMemo } from "react";

import { AnchorProvider } from "@project-serum/anchor";
import {
  AnchorWallet,
  useAnchorWallet,
  useConnection,
} from "@solana/wallet-adapter-react";

export default function useAnchorProvider() {
  const { connection } = useConnection();
  const anchorWallet = useAnchorWallet() as AnchorWallet;

  const provider = useMemo(() => {
    return new AnchorProvider(
      connection,
      anchorWallet,
      AnchorProvider.defaultOptions(),
    );
  }, [connection, anchorWallet]);

  return {
    provider,
  };
}
