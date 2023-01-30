import { CircularProgress } from "@mui/material";

import useAllAccounts from "@/features/marketplace/useAllAccounts/useAllAccounts";

export default function ProviderList() {
  const [providers, isLoading] = useAllAccounts<"provider">(
    "provider",
    console.log,
  );

  return (
    <div className="container mx-auto">
      <div className="flex gap-2">
        <h1>Providers</h1> {isLoading && <CircularProgress />}
      </div>

      <div className="flex flex-col gap-2">
        {providers.map((provider) => {
          return (
            <div
              className="rounded bg-black p-5 text-white"
              key={provider.publicKey.toString()}
            >
              {provider.account.name} {provider.publicKey.toString()}
            </div>
          );
        })}
      </div>
    </div>
  );
}