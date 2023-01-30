import {describe, expect,} from "vitest";
import {render, screen, waitFor} from "@testing-library/react";
import WalletButton from "@/features/wallet/WalletButton/WalletButton";

describe("WalletButton", () => {
    test("it should render the default wallet connect button from @solana/wallet-adapter-react-ui", async () => {
        render(<WalletButton/>);

        const getWalletButtonByDefaultText = (): HTMLButtonElement => {
            return screen.getByText<HTMLButtonElement>("Select Wallet");
        }

        await waitFor(() => {
            const walletMultiButton = getWalletButtonByDefaultText();
            expect(walletMultiButton).toBeDefined();
            expect(walletMultiButton.className).toContain("wallet-adapter-button");
            expect(walletMultiButton.type).toBe("button");
        });
    })
})