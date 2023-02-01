import dynamic from "next/dynamic";

export default function WalletButton() {
  const WalletMultiButtonDynamic = dynamic(
    async () =>
      (await import("@solana/wallet-adapter-react-ui")).WalletMultiButton,
    {
      ssr: false,
      loading: () => <div>Loading...</div>,
    },
  );

  return <WalletMultiButtonDynamic className="bg-purple-800" />;
}
