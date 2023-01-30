import { AnchorProvider } from "@project-serum/anchor";
import { renderHook } from "@testing-library/react";
import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import useAnchorProvider from "@/features/anchor/useAnchorProvider/useAnchorProvider";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";

describe("useAnchorProvider", () => {
  test("it should return an instance of #AnchorProvider", () => {
    const Client = () => {
      const { anchorProvider } = useAnchorProvider();
      expect(anchorProvider).toBeTruthy();
      expect(anchorProvider).toBeInstanceOf(AnchorProvider);
      return <></>;
    };

    render(
      <WalletWrapper>
        <Client />
      </WalletWrapper>,
    );
  });
});
