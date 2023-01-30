export default function endpoint(): string {
    const {NODE_ENV, VITE_NEXT_PUBLIC_API} = process.env;

    if (NODE_ENV === "test") return VITE_NEXT_PUBLIC_API as string;
    return process.env.NEXT_PUBLIC_API as string;
}