import * as process from "process";

export default function Header() {
    console.log(process.env.NEXT_PUBLIC_API)
    return (
        <header className="container mx-auto p-5 flex justify-between">
            <div>logo</div>
            <div>
                button
            </div>
        </header>
    )
}