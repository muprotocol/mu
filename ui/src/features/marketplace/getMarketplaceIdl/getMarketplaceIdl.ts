import marketplaceIdl from "@mu/marketplace/target/idl/marketplace.json";
import { Marketplace } from "@mu/marketplace/target/types/marketplace";

export function getMarketplaceIdl(): typeof marketplaceIdl & Marketplace {
  return marketplaceIdl as typeof marketplaceIdl & Marketplace;
}
