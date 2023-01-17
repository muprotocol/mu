import endpoint from "@/src/constants/endpoint/endpoint";

export default function Header() {
    console.log(endpoint())
    return (
        <header className="container mx-auto p-5 flex justify-between">
            <div>logo</div>
            <div>
                button
            </div>
        </header>
    )
}