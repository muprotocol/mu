import {describe, expect,} from "vitest";
import {render, screen, waitFor} from "@testing-library/react";
import WalletButton from "@/components/wallet/WalletButton/WalletButton";

describe("WalletButton", () => {
    test("it should render", async () => {
        render(<WalletButton/>);

        await waitFor(() => {
            const walletMultiButton = screen.getByTestId("WalletMultiButton");
            expect(walletMultiButton).toBeDefined()
        });
    })
})