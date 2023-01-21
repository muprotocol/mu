export default function endpoint(): string {
    const {NODE_ENV, NEXT_PUBLIC_API, VITE_NEXT_PUBLIC_API} = process.env;

    if (NODE_ENV === "test") return VITE_NEXT_PUBLIC_API as string;
    return NEXT_PUBLIC_API as string;
}