import useWallets from "./useWallets"

describe("useWallets", () => {
    test("it should return an array of wallet adapters", () => {
        const wallets = useWallets();

        console.log(wallets)
    })
})