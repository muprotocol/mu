import useGetMarketplaceAccount from "@/features/marketplace/useGetMarketplaceAccount";
import useProviderList from "@/features/provider/useProviderList/useProviderList";

export default function ProviderList() {
  // const { providers } = useProviderList();
  const { providers } = useGetMarketplaceAccount("provider");

  return (
    <div className="container mx-auto">
      <div className="flex gap-2">
        <h1>Providers</h1>
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
