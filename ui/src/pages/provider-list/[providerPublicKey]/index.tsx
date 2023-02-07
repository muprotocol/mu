import { useRouter } from "next/router"

export default function Provider() {
    const router = useRouter()
    const { providerPublicKey } = router.query

    return <p>{providerPublicKey}</p>
}