import { useMemo } from "react";

import { AnchorProvider } from "@project-serum/anchor";
import {
  AnchorWallet,
  useAnchorWallet,
  useConnection,
} from "@solana/wallet-adapter-react";

export default function useAnchorProvider(): {
  anchorProvider: AnchorProvider;
} {
  const { connection } = useConnection();
  const anchorWallet = useAnchorWallet() as AnchorWallet;

  const anchorProvider = useMemo(() => {
    return new AnchorProvider(
      connection,
      anchorWallet,
      AnchorProvider.defaultOptions(),
    );
  }, [connection, anchorWallet]);

  return {
    anchorProvider,
  };
}
