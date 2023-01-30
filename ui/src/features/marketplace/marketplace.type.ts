import { Marketplace } from "@mu/marketplace/target/types/marketplace";

import {
  AccountNamespace,
  IdlAccounts,
  ProgramAccount,
} from "@project-serum/anchor/dist/cjs/program/namespace";

export type MarketplaceAccountName = keyof AccountNamespace<Marketplace>;

export type MarketplaceAccount<T extends MarketplaceAccountName> = ProgramAccount<
  IdlAccounts<Marketplace>[T]
>;
