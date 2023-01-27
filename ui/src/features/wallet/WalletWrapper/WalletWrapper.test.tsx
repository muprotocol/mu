import {describe, expect} from "vitest";
import {render, screen} from "@testing-library/react";
import WalletWrapper from "@/features/wallet/WalletWrapper/WalletWrapper";
import {useContext} from "react";
import {ConnectionContext, WalletContext} from "@solana/wallet-adapter-react";

describe("WalletWrapper", () => {
    test("it should render itself and it's children", async () => {
        const WalletWrapperClient = () => {
            return (
                <div data-testid="WalletWrapperClient"></div>
            );
        };

        render(
            <WalletWrapper>
                <WalletWrapperClient/>
            </WalletWrapper>
        );

        const walletWrapperClient = screen.getByTestId("WalletWrapperClient");
        expect(walletWrapperClient).toBeDefined();
    })

    test("it should provide <ConnectionProvider />", () => {
        const WalletWrapperClient = () => {
            const {connection} = useContext(ConnectionContext)
            expect(connection).toBeDefined()
            return (<></>);
        };

        render(
            <WalletWrapper>
                <WalletWrapperClient/>
            </WalletWrapper>
        );
    })

    test("it should provide <WalletProvider />", () => {
        const WalletWrapperClient = () => {
            const {wallets} = useContext(WalletContext);
            expect(wallets).toBeDefined();
            expect(wallets).not.toEqual([]);
            return (<></>);
        };

        render(
            <WalletWrapper>
                <WalletWrapperClient/>
            </WalletWrapper>
        );
    })
})