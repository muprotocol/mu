import Header from '@/src/components/Header'
import '@/src/styles/globals.css'
import type {AppProps} from 'next/app'

export default function App({Component, pageProps}: AppProps) {
    console.log("called")
    return (
        <>
            <Header></Header>
            <Component {...pageProps} />
        </>
    )
}
