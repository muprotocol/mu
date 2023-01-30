import { AnchorProvider, Program } from "@project-serum/anchor";
import { describe, expect } from "vitest";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import useMarketplace from "@/features/marketplace/useMarketplace/useMarketplace";
import {render} from "@testing-library/react";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";
import {getMarketplaceIdl} from "@/features/marketplace/getMarketplaceIdl/getMarketplaceIdl";

describe("useMarketplace", () => {
  test("it should return an instance of #anchor.Program", () => {
    const Client = () => {
      const { marketplace } = useMarketplace();
      expect(marketplace).toBeTruthy();
      expect(marketplace).toBeInstanceOf(Program);
      return <></>;
    };

    render(
        <WalletWrapper>
          <Client />
        </WalletWrapper>,
    );
  });

  test("it should use #getMarketplaceIdl() for IDL", () => {
    const Client = () => {
      const { marketplace } = useMarketplace();
      expect(marketplace.idl).toBeTruthy();
      expect(marketplace.idl).toStrictEqual(getMarketplaceIdl());
      return <></>;
    };

    render(
        <WalletWrapper>
          <Client />
        </WalletWrapper>,
    );
  })
});
